# Reference (Go)

The complete public surface of the Go `css` module: exports, the parse
entry points, the one option, and the complete AST node reference. For
a guided introduction see the [tutorial](tutorial.md); for task recipes
see the [how-to guide](guide.md); for how it works (and how it differs
from TypeScript) see [concepts](concepts.md).

## Module

```bash
go get github.com/tabnas/css/go@latest
```

```go
import tabnascss "github.com/tabnas/css/go"
```

| | |
|---|---|
| Module | `github.com/tabnas/css/go` |
| Package | `tabnascss` |
| Engine | `github.com/tabnas/jsonic/go` (pulled in transitively) |
| `Version` | exported `const` string of the module version (`"0.1.0"`) |

## Public API

### `func Parse(src string, opts ...CssOptions) (any, error)`

Parses a CSS string and returns its AST. Convenience wrapper around
`MakeJsonic(opts...).Parse(src)`.

With **no** options it reuses a single lazily-created instance (behind a
`sync.Once`), so repeated calls do not rebuild the engine + grammar.
The shared instance is safe for concurrent use (each parse builds its
own context and only reads instance state). With options, a dedicated
instance is built per call, since the configuration differs per call.

```go
ast, err := tabnascss.Parse(`a { color: red }`)
// ast: map[string]any{"type": "stylesheet", "rules": []any{
//   map[string]any{"type": "rule", "selectors": []any{"a"}, "declarations": []any{
//     map[string]any{"type": "declaration", "property": "color", "value": "red"}}}}}
```

### `func MakeJsonic(opts ...CssOptions) *jsonic.Jsonic`

Returns a reusable `*jsonic.Jsonic` instance configured for CSS
parsing. Use this when parsing many strings with the same options:
build once, call `.Parse()` per input.

```go
j := tabnascss.MakeJsonic()
ast, err := j.Parse(`@media screen { a { color: blue } }`)
```

A plugin-registration failure (a programming error with static inputs)
panics rather than misbehaving silently.

### `func Css(j *jsonic.Jsonic, options map[string]any) error`

The raw plugin function. Usually invoked indirectly through
`j.UseDefaults(tabnascss.Css, tabnascss.Defaults, opts...)` or via
`Parse` / `MakeJsonic`. It is idempotent: a re-invocation guard (a
`css-init` decoration) makes re-application during `SetOptions` a no-op.

```go
j := jsonic.Make()
j.UseDefaults(tabnascss.Css, tabnascss.Defaults)
ast, err := j.Parse(`a { color: red }`)
```

### `var Defaults map[string]any`

The default option map, paired with `Css` for `UseDefaults`:

```go
var Defaults = map[string]any{
    "lowercaseProperties": false,
}
```

### `type CssOptions struct`

A typed wrapper over the option map. The field is `*bool` so callers can
express "omit" (nil) vs "set":

```go
type CssOptions struct {
    // LowercaseProperties, when non-nil and true, lowercases declaration
    // property names (CSS property names are case-insensitive). Selectors,
    // values and at-rule preludes are untouched.
    LowercaseProperties *bool
}
```

## Options

### `LowercaseProperties`

- **Type:** `*bool`
- **Default:** `false` (nil)
- **Effect:** Controls the case of declaration property names (the
  identifier before `:`). Selectors, values, and at-rule preludes are
  unaffected.
  - nil / `false` — the property name is kept verbatim. `COLOR` →
    `COLOR`.
  - `true` — the property name is lowercased. `COLOR` → `color`.

```go
yes := true
tabnascss.Parse(`A { COLOR: Red }`, tabnascss.CssOptions{LowercaseProperties: &yes})
// the rule node: selectors ["A"], declarations [
//   {type: "declaration", property: "color", value: "Red"} ]
```

`LowercaseProperties` is the only option.

## The AST

Every node is a `map[string]any` with a `"type"` discriminator. Arrays
(`rules`, `declarations`, `selectors`, `values`, `keyframes`) are
`[]any` and preserve source order (including duplicate properties). All
leaf values are `string` — there is no numeric type; `12px` and `0` are
strings.

### `stylesheet` (top)

The root node. `rules` holds the ordered child nodes (style rules,
at-rule nodes, and `comment` nodes).

```go
map[string]any{"type": "stylesheet", "rules": []any{ /* nodes */ }}
```

### `rule`

A style rule. `selectors` is the list of selector strings (a grouped
selector becomes multiple entries; the block is not duplicated).
`declarations` is the ordered list of `declaration` and `comment`
nodes.

```go
map[string]any{"type": "rule",
    "selectors":    []any{"h1", "h2"},
    "declarations": []any{ /* declaration / comment nodes */ }}
```

### `declaration`

A `property : value` pair. `value` is the raw text run up to the next
top-level `;`/`}`, trimmed, with comments stripped and quotes kept.

```go
map[string]any{"type": "declaration", "property": "color", "value": "red"}
```

### `comment`

A `/* ... */` comment captured at a statement position. `comment` holds
the raw inner text (the bytes between `/*` and `*/`, untrimmed).

```go
map[string]any{"type": "comment", "comment": " note "}
```

### Block at-rules with a rules body

`@media`, `@supports`, `@document`, and `@host` carry a recursively
parsed `rules []any`. Each carries its prelude under a field named for
its type (except `@host`, which has no prelude):

| At-rule | Node |
|---|---|
| `@media screen` | `{type: "media", media: "screen", rules: [...]}` |
| `@supports (display: grid)` | `{type: "supports", supports: "(display: grid)", rules: [...]}` |
| `@document url(...)` | `{type: "document", document: "...", rules: [...]}` |
| `@host` | `{type: "host", rules: [...]}` |

A generic block at-rule (e.g. `@layer base`) follows the same shape:
`{type: "layer", layer: "base", rules: [...]}`. A vendor-prefixed
`@document` additionally carries a `vendor` field.

### Block at-rules with a declarations body

`@font-face` and `@page` carry a `declarations []any` body instead of
`rules`. `@page` also carries a `selectors []any` (its prelude, if any;
empty otherwise):

```go
map[string]any{"type": "font-face", "declarations": []any{ /* declarations */ }}
map[string]any{"type": "page", "selectors": []any{":first"}, "declarations": []any{ /* ... */ }}
```

Generic declaration at-rules (`@viewport`, `@counter-style`,
`@property`, `@font-palette-values`) follow the `font-face` shape:
`{type: kw, declarations: [...]}`.

### `keyframes` / `keyframe`

`@keyframes` becomes a `keyframes` node with a `name` and a `keyframes
[]any` of `keyframe` nodes. Each `keyframe` has a `values []any` (its
selectors, e.g. `from`, `to`, `50%`) and a `declarations []any`. A
vendor-prefixed `@-webkit-keyframes` additionally carries a `vendor`
field.

```go
map[string]any{"type": "keyframes", "name": "slide", "keyframes": []any{
    map[string]any{"type": "keyframe", "values": []any{"from"},
        "declarations": []any{ /* declarations */ }},
    map[string]any{"type": "keyframe", "values": []any{"50%", "100%"},
        "declarations": []any{ /* declarations */ }}}}

// vendor-prefixed:
map[string]any{"type": "keyframes", "name": "x", "vendor": "-webkit-",
    "keyframes": []any{ /* keyframe nodes */ }}
```

### Statement at-rules

`@import`, `@charset`, `@namespace`, and other block-less at-rules
(terminated by `;`) become leaf nodes whose `type` is the at-keyword,
with a same-named field holding the raw params (quotes kept):

```go
map[string]any{"type": "import",  "import":  `"base.css"`}
map[string]any{"type": "charset", "charset": `"utf-8"`}
```

## CSS syntax accepted

The plugin parses a stylesheet without surrounding braces, supplying all
CSS structure through its own rules (disabling jsonic's implicit
maps/lists and rebinding the punctuation tokens).

### Rulesets

A ruleset is `selector { declaration; declaration; ... }`. The selector
prelude becomes one or more entries in `selectors`; the block becomes
`declarations`.

```
a { color: red }            => rule, selectors ["a"], one declaration
a {}                        => rule, selectors ["a"], declarations []
a { x: 1 } b { y: 2 }       => two rule nodes, in order
```

A single selector is kept exactly as written (trimmed), including:

| Selector kind | Example |
|---|---|
| Type | `a` |
| Class / id | `.foo`, `#bar` |
| Combinator | `.foo > .bar` |
| Pseudo-class | `a:hover` |
| Pseudo-element | `a::before` |
| Attribute | `input[type=text]` |

A comma-**grouped** selector (`h1, h2`) becomes multiple `selectors`
entries on one rule. Commas nested inside `:not(...)`/`:is(...)`,
strings, `()` or `[]` are not split.

### Declarations

A declaration is `property : value`. The property is an identifier of
`[A-Za-z0-9_-]` characters. The value runs to the next top-level `;` or
`}`, trimmed, with comments stripped, returned as a raw `string`. A
trailing `;` before `}` (or end of input) is optional. Declaration
order and duplicate properties are preserved.

```
color: red                  => {property: "color", value: "red"}
border: 1px solid #fff      => {property: "border", value: "1px solid #fff"}
color: red !important       => {property: "color", value: "red !important"}
```

Inside a value, the scanner skips over strings, `( )`, `[ ]`, and
comments, so colons, commas, and braces nested in them do not end the
value:

```
background: url(http://x/y.png)   => value "url(http://x/y.png)"
color: rgb(1, 2, 3)               => value "rgb(1, 2, 3)"
```

### At-rules

See [The AST](#the-ast) above for the exact node per at-rule. In
summary: `@media`/`@supports`/`@document`/`@host` (and generic block
at-rules) carry a `rules` body; `@font-face`/`@page` (and generic
declaration at-rules) carry a `declarations` body; `@keyframes` carries
a `keyframes` body; statement at-rules (`@import`, `@charset`,
`@namespace`) are leaf nodes.

### Comments

Only `/* ... */` block comments are recognised. At a statement position
they become `comment` nodes (raw inner text); mid-construct (e.g.
between a property name and its `:`) they are skipped. CSS has no `//`
line comments, and `#` is not a comment.

### Empty input

A zero-length source runs no rules (an engine convention) and yields
`nil`. Any non-empty source — even pure whitespace or a comment-only
source — yields a `stylesheet` node.

```
""            => nil
"   \n  "     => {type: "stylesheet", rules: []}
"/* only */"  => {type: "stylesheet", rules: [ {type: "comment", comment: " only "} ]}
```

## Tokens

The custom `cssToken` lex matcher owns all non-fixed text; the fixed
punctuation lexes as a small set of tokens:

| Token | Source | Meaning |
|---|---|---|
| `#OB` | `{` | start of a block |
| `#CB` | `}` | end of a block |
| `#CL` | `:` | declaration separator |
| `#CA` | `;` | declaration terminator (remapped from jsonic's comma) |
| `#TX` | selector / keyframe value / property name | a key |
| `#GC` | `,` | selector-group separator |
| `#VL` | raw text run | a declaration value |
| `#CC` | `/* ... */` | a comment (at a statement position) |
| `#ATR` | `@media`, `@supports`, … | a block at-rule with a rules body |
| `#ATD` | `@font-face`, `@page`, … | a block at-rule with a declarations body |
| `#ATK` | `@keyframes` (incl. vendor) | a keyframes at-rule |
| `#ATS` | `@import`, `@charset`, … | a statement at-rule |

The at-rule tokens carry the keyword in `val` and the prelude/params in
`use`. Bare `[` and `]` are **not** structural tokens — they only
appear inside selectors/values, where the matcher consumes them as
text.

## Grammar group tag

Every grammar alternate the plugin adds carries the group tag `css`.
Callers can switch the CSS alts off (restoring plain jsonic) via
`Options{Rule: &RuleOptions{Exclude: "css"}}`:

```go
j := jsonic.Make()
j.UseDefaults(tabnascss.Css, tabnascss.Defaults)
j.SetOptions(jsonic.Options{Rule: &jsonic.RuleOptions{Exclude: "css"}})
```

## Errors

`Parse` and `Jsonic.Parse` return an `error` rather than panicking. The
error is jsonic's parse error, reporting an error code and the source
location (row, column, position). A `MakeJsonic` / `Css` plugin-setup
failure (a programming error with static inputs) panics instead, since
it cannot arise from user input.
