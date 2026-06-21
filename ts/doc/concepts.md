# Concepts

Background on how the CSS plugin is put together, and why. This is
understanding-oriented reading — for steps see the
[tutorial](tutorial.md) and [how-to guide](guide.md), and for exact
signatures and the full node reference see the
[reference](reference.md).

## A grammar plugin on a shared engine

The plugin has no parser of its own. It is a thin layer on a stack of
three pieces:

- the **Tabnas engine** (`@tabnas/parser`) — a rule-based parser over a
  configurable, matcher-based lexer,
- the **relaxed-JSON grammar** (`@tabnas/jsonic`) — the rules and
  fixed tokens (`{` `}` `:`) and machinery this plugin reuses, and
- **this plugin** (`@tabnas/css`) — the option overrides, one custom
  lex matcher, a small grammar overlay, and the actions that build the
  AST.

Because the engine is configuration-driven, CSS support is mostly an
options change plus a set of grammar rules — not a new parser. The
plugin embeds the canonical grammar text (from the repo-root
`css-grammar.jsonic`) as a string, parses it with a throwaway jsonic
instance to get a grammar object, attaches its option overrides and
node-building actions to that object, and hands the whole thing to the
engine atomically via
`tn.grammar(grammarDef, { rule: { alt: { g: 'css' } } })`.

## The output model: a faithful AST

A stylesheet parses to a **typed, ordered abstract syntax tree** in the
[`reworkcss/css`](https://github.com/reworkcss/css) shape — not a lossy
map. The top is always `{ type: 'stylesheet', rules: [ ...Node ] }`,
and every node carries a `type` discriminator:

- a **rule** has `selectors: string[]` and `declarations: Node[]`;
- a **declaration** has `property` and a raw-string `value`;
- a **comment** has its `comment` text;
- **block at-rules** (`@media`, `@supports`, `@font-face`,
  `@keyframes`, …) are their own node types, holding a nested `rules`
  or `declarations` body;
- **statement at-rules** (`@import`, `@charset`, …) are leaf nodes
  typed for the at-keyword.

Because the tree is built from ordered arrays, it preserves what a map
would lose: declaration order, **duplicate** properties
(`a { color: red; color: blue }` keeps both), rule types, and comment
positions.

The deliberate simplification is that declaration values and selectors
are not parsed into further structure. A `value` is the trimmed text up
to the next top-level `;` or `}` (e.g. `'1px solid #fff'`); a selector
is the trimmed text up to its `{` (or the next top-level `,`). This
keeps the AST faithful to the source and the grammar small — a consumer
that needs to pick a value or selector apart can do so on the returned
strings.

## CSS is not JSON

JSON and CSS share braces but little else, so the plugin reshapes the
jsonic stack rather than accepting both:

| | JSON / jsonic | CSS |
|---|---|---|
| Top level | a single value | a `stylesheet` node over a list of rules |
| Output | nested objects/arrays | a typed, ordered AST |
| Map keys | quoted strings | selectors / property names (text) |
| Key/value separator | `:` | `:` (declarations) |
| Member separator | `,` | `;` |
| Bare `[` `]` | lists | disabled (only inside selectors/values) |
| Strings | `"` `'` `` ` `` | values are raw text, not lexed strings |
| Comments | `#` `//` `/* */` | `/* */` only |

The plugin makes those swaps by **disabling** what JSON allows and
**adding** what CSS needs: it remaps the member separator from `,` to
`;`, disables the `[` `]` openers, turns off jsonic's string/number/
text matchers (one custom matcher owns all text), narrows the key set,
limits comments to `/* */`, and replaces jsonic's object-building
actions with its own AST node constructors.

## The one context-sensitive matcher

The heart of the plugin is a single custom lex matcher, `cssToken`,
registered with a high `order` so it runs ahead of the fixed-token
matcher and owns all non-punctuation text. It defers (returns
`undefined`) on whitespace and on the fixed punctuation (`{` `}` `:`
`;`), letting the builtin matchers handle those.

What it emits depends on position — which it decides from the active
rule alone, so it is stateless:

- **Value mode** — selected when the `declval` rule is open. The
  grammar pushes `declval` exactly at a value position (just after a
  `:`). The matcher reads the run of text up to the next top-level
  `;`/`}` and emits one `#VL` token (comments stripped, trimmed). So
  `1px solid #fff` becomes a single value string.

- **Key mode** — otherwise. It peeks ahead: if a top-level `{` is
  reached before any `;`/`}`, the text is a **selector** (as `#TX`); if
  a `;`/`}`/end-of-input is reached first, it is a **property name**
  (the identifier, as `#TX`). A selector ends at the next top-level `,`
  as well, and the comma is emitted as a `#GC` token, so a group like
  `h1, h2` arrives as two `#TX` keys — never one split string.

- **At-rule** — a leading `@` is classified by the same `{`-before-`;`
  lookahead and by keyword: a block at-rule emits `#ATR` (rules body),
  `#ATD` (declarations body), or `#ATK` (`@keyframes`), carrying its
  prelude; a statement at-rule emits `#ATS`, carrying its params.

- **Comment** — a `/* */` at a statement or declaration-list position
  emits a `#CC` token (so it becomes a `comment` node). Elsewhere the
  matcher declines it and the builtin comment matcher skips it as
  insignificant whitespace.

All the lookahead scans skip over quoted strings, `/* */` comments, and
balanced `()` / `[]`, so a `;` inside `url(...)`, a `{` inside an
attribute selector, or a `,` inside `:not(.a, .b)` does not fool the
classifier.

## From tokens to nodes

Because the classifier ran in the lexer, the grammar only ever sees an
already-disambiguated token shape, and a small set of rules with
node-building actions assembles the AST:

- `#TX #OB` (`selector {`) → a `rule` node; `@cssRule` constructs it,
  `@cssSelector` pushes the selector, and `declbody` fills its
  `declarations`.
- `#TX #GC` (`selector ,`) → one selector of a group; `@cssSelector`
  pushes it and the `sel` rule loops for the next, so all selectors
  land on the **same** rule node — no selector string is ever split,
  and the block is parsed once.
- `#TX #CL` (`property :`) → a `declaration` node; `@cssDecl`
  constructs it and `@cssDeclVal` sets its raw value.
- `#ATR`/`#ATD`/`#ATK` → a block at-rule node (`@cssAtRules`,
  `@cssAtDecls`, `@cssKeyframes`), whose body recurses into `rules`,
  `declarations`, or `keyframes`.
- `#ATS` → a leaf statement-at-rule node (`@cssAtStmt`).
- `#CC` → a `comment` node (`@cssComment`).

The list readers (`items`, `decls`, `kfitems`) build each child and a
pusher action (`@cssPushRule`, `@cssPushDecl`, `@cssPushKf`) appends it
to the parent array, which is what preserves order and duplicates. This
two-token-lookahead design is also why a pseudo-class selector like
`a:hover` is not mistaken for a property named `a`: in key position a
`:` is allowed inside the prelude, and the trailing `{` proves it is a
selector.

## Why comment position matters

A `/* */` comment becomes a `comment` node only where the grammar is
reading the *first token of a list item* — the statement list, the
declaration list, the keyframe list, and the block wrappers that peek
the first body token. A comment seen mid-construct (e.g. between a
property name and its `:`, under a builder rule rather than a list
reader) is declined by `cssToken` and skipped by the builtin matcher.
That is why `a /* x */ { color /* y */ : red }` yields just a clean
`declaration` node, while `a { /* c1 */ color: red }` keeps the `/* c1
*/` as a comment node ahead of the declaration.

## The implicit top-level stylesheet

Unlike a JSON document, a stylesheet has no surrounding braces — it is
just a sequence of statements. The `stylesheet` start rule constructs
the root node (`@cssSheet`) and runs the `items` list reader until
end-of-input (`#ZZ`), appending each built statement node to `rules`.

This is why a **zero-length** source returns `undefined`: with no
input, no rule ever runs — an engine convention. Any non-empty source —
even whitespace or a lone comment — runs the start rule and yields at
least an empty `stylesheet` node (`rules: []`).

## The case-folding option

`lowercaseProperties` acts only in the matcher, on the identifier it is
already about to emit in **key mode** when that identifier resolved to a
property name — never a selector, never a value, never an at-rule
prelude. It is safe to default off and reasonable to turn on because
CSS property names are case-insensitive. Values are left alone because
parts of a value (quoted strings, `url()` contents, custom identifiers)
are case-sensitive and lowercasing would corrupt them.

## Why reuse one instance

Building the CSS grammar — parsing the embedded grammar text, applying
the option overlay, wiring the custom matcher and the node actions —
dominates the cost of a parse; the parse itself, on a typical small
stylesheet, is cheap by comparison. The instance is stateless across
parses (each parse builds its own context and only reads instance
state), so the right pattern is to build the engine once and reuse it
for every input.

## Accepted vs rejected — edge cases

- `''` → `undefined`; `'   '` and `/* c */` → a `stylesheet` node (the
  latter with one `comment` node). A zero-length source runs no rules;
  any non-empty source yields a stylesheet.
- `a {}` → a `rule` node with empty `declarations`.
- `h1, h2 { ... }` → one `rule` node with `selectors: ['h1', 'h2']`.
  Commas inside `:not(...)`, strings, `()`/`[]` are not split.
- `a:hover { ... }` → `selectors: ['a:hover']`, not a property `a`. A
  `:` is allowed inside a selector prelude.
- `a { color: red; color: blue }` → both declarations kept, in order.
- `1px solid #fff` → one raw value string. Values are not parsed
  further.
- `url(http://x/y.png)` → one value; the `:` and `/` inside `()` do not
  terminate it.
- `@media ... { ... }` → a `media` node with a nested `rules` body.
- `@import "base.css";` → `{ type: 'import', import: '"base.css"' }`;
  the value keeps its quotes. A statement at-rule must end with `;`.
- `/* ... */` → a `comment` node at a list position, skipped
  mid-construct; `#` and `//` are **not** comments in CSS.
- CSS Nesting (at-rules/rules nested inside a declaration block) is not
  supported.

## Relationship to the Go port

The plugin ships in two implementations — this TypeScript one and a Go
port — built from the same canonical `css-grammar.jsonic`. The
TypeScript version is the reference. For the Go API shape, value types,
and any accepted differences, see
[../../go/doc/concepts.md](../../go/doc/concepts.md).
