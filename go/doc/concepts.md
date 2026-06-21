# Concepts (Go)

Background on how the Go CSS plugin is put together, and why — plus a
section on how it differs from the TypeScript version. This is
understanding-oriented reading; for steps see the
[tutorial](tutorial.md) and [how-to guide](guide.md), and for exact
signatures and node shapes see the [reference](reference.md).

## A grammar plugin on a shared engine

The plugin has no parser of its own. It is a thin layer on a stack of
two pieces:

- the **jsonic engine** (`github.com/tabnas/jsonic/go`) — a rule-based
  parser over a configurable, matcher-based lexer, carrying the
  relaxed-JSON grammar, its fixed tokens (`{` `}` `:`), and its rule
  machinery, and
- **this plugin** (`github.com/tabnas/css/go`) — the option overrides,
  one custom lex matcher, a grammar overlay of rules, and a set of
  grammar-local actions that build the AST.

Because the engine is configuration-driven, CSS support is mostly an
options change plus a set of rules — not a new parser. The plugin
embeds the canonical grammar text (from the repo-root
`css-grammar.jsonic`, kept in sync with the TypeScript source by the
build), parses it with a throwaway jsonic instance into a
`*jsonic.GrammarSpec`, attaches its `*jsonic.Options` overrides and its
node-building action refs to that spec, and applies the whole thing
atomically via `j.Grammar(gs, &jsonic.GrammarSetting{Rule: ...G:
"css"})`.

## The output model: a reworkcss-style AST

The parser produces a faithful AST — ordered, typed nodes — modelled on
[`reworkcss/css`](https://github.com/reworkcss/css). Nothing is lossy:
declaration order, duplicate properties, rule types, and comment
positions all survive.

The root is always:

```go
map[string]any{"type": "stylesheet", "rules": []any{ /* nodes */ }}
```

Every node is a `map[string]any` with a `"type"` discriminator. The
node kinds:

| `type` | Fields | Source |
|---|---|---|
| `stylesheet` | `rules []any` | the whole sheet |
| `rule` | `selectors []any`, `declarations []any` | `a, b { ... }` |
| `declaration` | `property`, `value` | `color: red` |
| `comment` | `comment` | `/* ... */` at a statement position |
| `media` / `supports` / `document` / `host` | prelude field, `rules []any` | block at-rules with a rules body |
| `font-face` / `page` | `declarations []any` (`page` also `selectors []any`) | block at-rules with a declarations body |
| `keyframes` | `name`, optional `vendor`, `keyframes []any` | `@keyframes` |
| `keyframe` | `values []any`, `declarations []any` | one keyframe block |
| `import` / `charset` / `namespace` | same-named params field | statement at-rules |

Arrays preserve source order. A comma-grouped selector (`h1, h2`) is
collected into the rule's `selectors` list (commas inside `:not(...)`,
strings, `()`/`[]` are not split). Values are never decoded: `12px`,
`"base.css"` (quotes kept), and `1px solid #fff` are all the literal
text between `:`/at-keyword and the next top-level `;`/`}`, trimmed,
with comments stripped. **Every value is a `string`** — there is no
numeric type.

## CSS structure is supplied, not inherited

JSON and CSS look nothing alike, so the plugin **disables** most of what
jsonic allows and **adds** the CSS shapes it needs:

| | JSON / jsonic | CSS |
|---|---|---|
| Top level | a single value | an implicit `stylesheet` node (no braces) |
| Open a block | `{` (a map) | `{` (a declaration block or rules body) |
| Key/value separator | `:` | `:` for declarations; selectors take no separator |
| Member separator | `,` (`#CA`) | `;` (`#CA` is remapped onto `;`) |
| Keys | quoted strings | bare selectors / property names / at-keywords (the `cssToken` matcher) |
| Values | typed scalars | raw text runs (always strings) |
| Comments | `#` `//` `/* */` | `/* */` only |

`Rule.Exclude = "jsonic,imp"` removes jsonic's implicit maps/lists,
top-level commas, and path-dive extensions; `Rule.Start = "stylesheet"`
makes the CSS root rule the entry point.

## The mechanisms

The plugin reshapes the stack with a few cooperating mechanisms, all
applied together through one `GrammarSpec`:

1. **One custom lex matcher.** `cssToken` is registered under
   `Options.Lex.Match` with a high `Order` so it runs ahead of the
   fixed-token matcher and owns every non-fixed run of text — selectors,
   property names, at-keywords, values, and statement-position comments.
   It defers (returns `nil`) on the fixed punctuation and on whitespace
   (and on comments away from statement positions), so those fall
   through to the builtin matchers.

2. **Token remapping.** `#CA` (jsonic's comma / member separator) is
   rebound from `,` to `;`, the CSS declaration terminator; `:` stays
   `#CL`. The default mappings for bare `[` (`#OS`) and `]` (`#CS`) are
   dropped to `nil` — they only ever appear inside selectors/values,
   where `cssToken` swallows them as text. The default string, number,
   text, and value matchers are turned off, since `cssToken` owns all
   text.

3. **Key-set restriction.** The `KEY` token set is narrowed to `#TX`,
   the text token `cssToken` produces for a selector / keyframe value /
   property name.

4. **Grammar overlay.** A set of rules drive the structure: a
   `stylesheet` node whose `items` reader fills `rules[]`; a `statement`
   reader that dispatches on the leading token to build a `comment`, a
   block/statement at-rule, or a style `rule`; a `sel` reader that
   collects a selector group; `declbody`/`decls`/`decl`/`declval` for
   declaration blocks; and `rulesbody`/`kfbody`/`kfitems`/`keyframe` for
   at-rule and keyframe bodies.

5. **Node-building actions.** The AST is assembled entirely by
   grammar-local action refs (e.g. `@cssSheet`, `@cssRule`, `@cssDecl`,
   `@cssComment`, `@cssAtRules`, `@cssKeyframes`) attached to the spec's
   `Ref` map. They fall into three kinds: node constructors (overwrite
   `r.Node` with a fresh typed map), field setters (push a selector /
   value, set a declaration's value), and array pushers (append a
   finished child node to the parent's `rules` / `declarations` /
   `keyframes`).

## One context-sensitive matcher

CSS has no sigil that distinguishes a selector from a property name from
a value — the same characters can begin any of them. The engine allows
only limited lookahead, not enough to tell them apart by grammar alone.
So the decision is pushed into the lexer. `cssToken` is stateless — it
looks only at the active rule name to choose what to emit:

- **Value mode** — when the `declval` rule is open, read a declaration
  value up to the next top-level `;`/`}` and emit `#VL` (comments
  stripped, surrounding space trimmed). The grammar pushes `declval`
  exactly at a value position (after a `:`), so no flag or lookbehind is
  needed.
- **Key mode** — peek ahead to choose between a **selector** (a
  top-level `{` is reached first → emit `#TX`) and a **property name** (a
  top-level `;`/`}` is reached first → the identifier up to `:`, as
  `#TX`). A selector ends at the next top-level `,` as well, with the
  comma emitted as a `#GC` token, so a group (`h1, h2`) arrives as two
  `#TX` keys with a `#GC` between — never a split string.
- **At-rule** — a leading `@` is classified by a `{`-before-`;`
  lookahead and by keyword: a rules-body block (`#ATR`), a
  declarations-body block (`#ATD`), `@keyframes` (`#ATK`), or a
  statement at-rule (`#ATS`). The token carries the keyword in `val` and
  the prelude/params in `use`.
- **Comment** — a `/* */` at a statement-list position (where the
  grammar reads the first token of each item) is emitted as a `#CC`
  node; elsewhere it is deferred and skipped.

While scanning, the matcher skips over strings, `( )`, `[ ]`, and
comments, so the punctuation inside `rgb(1, 2, 3)`, `url(http://…)`,
`[type=text]`, or `:not(.a, .b)` never ends a token prematurely.

## Why reuse one instance

Building the CSS grammar dominates the cost of a parse; the parse itself
is cheap. The default no-options `Parse` path therefore caches a single
instance behind a `sync.Once`, reusing it across calls (safe for
concurrent use, since a parse builds its own context and only reads
instance state). Option-taking calls build a dedicated instance, since
their configuration differs per call — use `MakeJsonic` once and reuse
it for a hot loop with fixed options.

## Differences from the TypeScript version

The TypeScript implementation is the reference; the Go module is a
faithful port built from the same single-sourced `css-grammar.jsonic`.
The differences do **not** change a successful parse's AST *structure* —
they concern the host language's API shape and a couple of engine
internals.

### API shape

| Area | TypeScript | Go |
|---|---|---|
| Convenience entry | none — install the plugin yourself | `tabnascss.Parse(src, opts...)` and `tabnascss.MakeJsonic(opts...)` |
| Build a parser | `new Tabnas().use(jsonic).use(Css, opts)` | `tabnascss.MakeJsonic(opts)` or `j.UseDefaults(tabnascss.Css, tabnascss.Defaults, m)` |
| Options | one object `{ lowercaseProperties, position }` | `CssOptions{ LowercaseProperties, Position *bool }`, or a `map[string]any` (`"lowercaseProperties"`, `"position"`) |
| "Omit vs set" | option present or absent | `*bool` nil vs set |
| Parse failure | **throws** | returns `error`; never panics on parse errors |

The Go side adds the `Parse` / `MakeJsonic` convenience helpers because
Go has no fluent `.use()` chain; the TypeScript side has no such helpers
(you build the engine yourself with `.use(jsonic).use(Css)`).

### AST representation

Both runtimes produce the *same* reworkcss-style AST: ordered, typed
nodes with the same `type` discriminators and the same fields. The only
difference is the host representation of the containers:

| Element | TypeScript | Go |
|---|---|---|
| A node | plain object `Record<string, any>` | `map[string]any` |
| An array (`rules`, `declarations`, `selectors`, `values`, `keyframes`) | JS array | `[]any` |
| A leaf value (`value`, `property`, `comment`, prelude/params) | `string` | `string` |
| Empty input (`""`) | (engine convention) | `nil` |

Because every CSS value is a `string` in both runtimes, there is **no
`float64`-vs-`number` divergence** of the kind a typed-scalar format
would have — there is no number type in the AST at all.

### Internals: context-sensitive lexing and token resolution

The `cssToken` matcher decides value-vs-key-vs-at-rule from the active
rule name alone — it is stateless. The Go port additionally **cannot**
auto-tokenize its custom token names the way the TypeScript plugin can:
an external Go package can't register new token tins implicitly. So the
plugin resolves the custom tins explicitly on the instance —
`j.Token("#CC")`, `j.Token("#GC")`, `j.Token("#ATR")`,
`j.Token("#ATD")`, `j.Token("#ATK")`, `j.Token("#ATS")` — and passes
them to the matcher, so the tokens it emits resolve to the same tins the
grammar's alts expect. The TypeScript plugin reaches the same result
through its host's automatic tokenization. The grammar structure is
otherwise identical.

### The re-invocation guard

`SetOptions` triggers a plugin re-application on the engine. The Go
plugin guards against doing its work twice with a `css-init` decoration:
on entry it checks `j.Decoration("css-init")` and returns early if set,
otherwise calls `j.Decorate("css-init", true)`. The TypeScript plugin is
structured the same way.

### Single-sourced grammar

The grammar is written once in the repo-root `css-grammar.jsonic` and
embedded verbatim into both `go/css.go` and the TypeScript source by the
build. Edit the grammar there, not in the generated source.

## Accepted vs rejected — edge cases

- `""` → `nil`; `"   "` or `"/* c */"` → a `stylesheet` node. Only a
  zero-length source yields `nil`.
- `a {}` → a `rule` node with empty `declarations`.
- `a { color: red }` parses the same as `a { color: red; }`. The
  trailing `;` is optional.
- `a { color: red; color: blue }` → two `declaration` nodes, in order.
  Order and duplicates are preserved.
- `border: 1px solid #fff` → one value string. A value runs to the next
  top-level `;`/`}`.
- `color: rgb(1, 2, 3)` → `"rgb(1, 2, 3)"`. Commas and `:` inside `()`,
  `[]`, strings, and comments are skipped.
- `color: red !important` → `"red !important"`. `!important` is just
  part of the value text.
- `h1, h2 { margin: 0 }` → one `rule` node, `selectors: ["h1", "h2"]`.
- `@import "base.css";` → `{type: "import", import: "\"base.css\""}`.
  The value keeps its quotes.
- `@media screen { … }` → a `media` node whose `rules` recurse.
- `@keyframes x { … }` → a `keyframes` node of `keyframe` nodes;
  `@-webkit-keyframes` adds `vendor: "-webkit-"`.
- `/* c */` at a statement position → a `comment` node; mid-construct it
  is skipped. `//` and `#` are not comments in CSS.
- `a { color: red; & b { top: 0 } }` → nesting is supported: the nested
  `rule` (or block at-rule) is appended to the parent's `declarations`,
  interleaved in source order. An identifier before `:` is a
  declaration; before `{` or `,` it is a nested style rule.
- Source positions are available: set the `Position` option to attach a
  1-based `start`/`end` line/column `"position"` to every node (off by
  default, in which case no `"position"` key is present).
