# Concepts (Go)

Background on how the Go CSS plugin is put together, and why — plus a
section on how it differs from the TypeScript version. This is
understanding-oriented reading; for steps see the
[tutorial](tutorial.md) and [how-to guide](guide.md), and for exact
signatures and syntax see the [reference](reference.md).

## A grammar plugin on a shared engine

The plugin has no parser of its own. It is a thin layer on a stack of
two pieces:

- the **jsonic engine** (`github.com/tabnas/jsonic/go`) — a rule-based
  parser over a configurable, matcher-based lexer, carrying the
  relaxed-JSON grammar, its fixed tokens (`{` `}` `:`), and its helper
  actions (`@object$`, `@key$`, `@setval$`, the `val`/`block`/`pair`
  rules), and
- **this plugin** (`github.com/tabnas/css/go`) — the option overrides,
  one custom lex matcher, and a small grammar overlay that retune that
  stack to read CSS instead of JSON.

Because the engine is configuration-driven, CSS support is mostly an
options change plus a handful of rules — not a new parser. The plugin
embeds the canonical grammar text (from the repo-root
`css-grammar.jsonic`, kept in sync with the TypeScript source by the
build), parses it with a throwaway jsonic instance into a
`*jsonic.GrammarSpec`, attaches its `*jsonic.Options` overrides to that
spec, and applies the whole thing atomically via `j.Grammar(gs,
&jsonic.GrammarSetting{Rule: ...G: "css"})`.

## The output model

A stylesheet is parsed into a nested `map[string]any`:

| CSS | Result |
|---|---|
| Stylesheet | top-level map of `selector → block` |
| Ruleset block | nested `map[string]any` of `property → value` |
| Declaration value | raw `string` |
| Block at-rule (`@media …`) | prelude key → recursively-parsed block map |
| Statement at-rule (`@import …`) | at-keyword key → raw-string value |

Selectors and at-rule preludes are kept verbatim as map keys (including
grouping such as `"h1, h2"`). Values are never decoded: `12px`,
`"base.css"` (quotes kept), and `1px solid #fff` are all returned as
the literal text between `:`/at-keyword and the next top-level `;`/`}`.

## CSS structure is supplied, not inherited

JSON and CSS look nothing alike, so the plugin **disables** most of what
jsonic allows and **adds** the CSS shapes it needs:

| | JSON / jsonic | CSS |
|---|---|---|
| Top level | a single value | an implicit map of rules (no braces) |
| Open a block | `{` (a map) | `{` (a declaration block or nested ruleset) |
| Key/value separator | `:` | `:` for declarations; selectors take no separator |
| Member separator | `,` (`#CA`) | `;` (`#CA` is remapped onto `;`) |
| Keys | quoted strings | bare selectors / property names (the `cssToken` matcher) |
| Values | typed scalars | raw text runs |
| Comments | `#` `//` `/* */` | `/* */` only |

`Rule.Exclude = "jsonic,imp"` removes jsonic's implicit maps/lists,
top-level commas, and path-dive extensions; `Rule.Start = "stylesheet"`
makes the implicit top-level rule map the entry point.

## The mechanisms

The plugin reshapes the stack with a few cooperating mechanisms, all
applied together through one `GrammarSpec`:

1. **One custom lex matcher.** `cssToken` is registered under
   `Options.Lex.Match` with a high `Order` so it runs ahead of the
   fixed-token matcher and owns every non-fixed run of text — selectors,
   property names, at-keywords, and values. It defers (returns `nil`) on
   the fixed punctuation and on whitespace/comments, so those fall
   through to the builtin matchers.

2. **Token remapping.** `#CA` (jsonic's comma / member separator) is
   rebound from `,` to `;`, the CSS declaration terminator; `:` stays
   `#CL`. The default mappings for bare `[` (`#OS`) and `]` (`#CS`) are
   dropped to `nil` — they only ever appear inside selectors/values,
   where `cssToken` swallows them as text. The default string, number,
   text, and value matchers are turned off, since `cssToken` owns all
   text.

3. **Key-set restriction.** The `KEY` token set is narrowed to `#TX`
   alone — the text token `cssToken` produces for a key.

4. **Grammar overlay.** Four rules — `stylesheet`, `block`, `pair`,
   `val` — drive the structure. `pair` has three open shapes,
   disambiguated by the token after the key: `#TX #CL` (declaration),
   `#TX #OB` (nested ruleset), `#TX #VL` (statement at-rule). Its close
   alts handle `;`-separated declarations, trailing `;`, block/sheet end,
   and an implicit next ruleset.

## One context-sensitive matcher

CSS has no sigil that distinguishes a selector from a property name from
a value — the same characters can begin any of them. The engine allows
only limited lookahead, not enough to tell them apart by grammar alone.
So the decision is pushed into the lexer. `cssToken` looks at the active
rule and a per-parse flag to choose what to emit:

- **Value mode** — read a declaration value up to the next top-level
  `;`/`}` and emit `#VL`. Selected when the `val` rule is open (right
  after a `:`), or when the previous key was a statement at-keyword.
- **Key mode** — emit `#TX`, peeking ahead to choose between a
  **selector** (a top-level `{` is reached first → the whole prelude,
  trimmed) and a **property name / at-keyword** (a top-level `;`/`}` is
  reached first → the identifier up to `:` or whitespace).

While scanning, the matcher skips over strings, `( )`, `[ ]`, and
comments, so the punctuation inside `rgb(1, 2, 3)`, `url(http://…)`, or
`[type=text]` never ends a key or value prematurely.

## The statement-at-rule flag

A statement at-rule (`@import "x";`) needs the matcher to emit the
at-keyword (`#TX`) and *then*, on the next call, the params as a value
(`#VL`) — even though the `val` rule is not yet open. The matcher
records "the next token is a value" in a flag. That flag lives in the
**per-parse context bag** (`ctx.U`, a `map[string]any`), not on the
rule, because the key `#TX` may be lexed under any of the
`stylesheet`/`pair`/`val` rules, and the bag is both stable across them
and isolated per parse (so concurrent parses don't interfere).

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
faithful port built from the same `css-grammar.jsonic`. The differences
do **not** change a successful parse's *structure* — they concern the
host language's API shape and a couple of engine internals.

### API shape

| Area | TypeScript | Go |
|---|---|---|
| Convenience entry | none — install the plugin yourself | `tabnascss.Parse(src, opts...)` and `tabnascss.MakeJsonic(opts...)` |
| Build a parser | `new Tabnas().use(jsonic).use(Css, opts)` | `tabnascss.MakeJsonic(opts)` or `j.UseDefaults(tabnascss.Css, tabnascss.Defaults, m)` |
| Options | one object `{ lowercaseProperties, lowercaseValues }` | `CssOptions{ LowercaseProperties *bool, LowercaseValues *bool }`, or a `map[string]any` |
| "Omit vs set" | option present or absent | `*bool` nil vs set |
| Parse failure | **throws** | returns `error`; never panics on parse errors |

The Go side adds the `Parse` / `MakeJsonic` convenience helpers because
Go has no fluent `.use()` chain; the TypeScript side has no such helpers
(you build the engine yourself with `.use(jsonic).use(Css)`).

### Value types

Both runtimes return the same nested-map structure with raw-string
values — there is no numeric distinction to worry about, since every CSS
value is a string in both. The only difference is the host
representation of the containers:

| Value | TypeScript | Go |
|---|---|---|
| Stylesheet / block | object (null-prototype) | `map[string]any` |
| Declaration / at-rule value | `string` | `string` |
| Empty input (`""`) | (engine convention) | `nil` |

Because values are never parsed into numbers, there is no `float64`-vs-
`number` divergence of the kind a typed-scalar format would have.

### Internals: context-sensitive lexing

The context-sensitive `cssToken` matcher uses `rule.Name` / `rule.State`
plus the per-parse `ctx.U` flag to decide what to emit. The Go port
**cannot** read the grammar's expected-token columns from an external
package, so it relies on the rule name and that flag — and the
TypeScript plugin deliberately uses the *same* rule-name approach (rather
than reading token columns) to keep the two implementations in parity.

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

- `""` → `nil`; `"   "` or `"/* c */"` → `map[string]any{}`. Only a
  zero-length source yields `nil`.
- `a {}` → `map[string]any{"a": map[string]any{}}`. An empty block is an
  empty map.
- `a { color: red }` parses the same as `a { color: red; }`. The
  trailing `;` is optional.
- `border: 1px solid #fff` → one value string. A value runs to the next
  top-level `;`/`}`.
- `color: rgb(1, 2, 3)` → `"rgb(1, 2, 3)"`. Commas and `:` inside `()`,
  `[]`, strings, and comments are skipped.
- `color: red !important` → `"red !important"`. `!important` is just part
  of the value text.
- `@import "base.css";` → `{"@import": "\"base.css\""}`. Statement
  at-rule; the value keeps its quotes.
- `@media screen { … }` → nested map under the prelude key. Block
  at-rule; its block recurses.
- `/* c */` discarded; `//` and `#` are not comments in CSS.
