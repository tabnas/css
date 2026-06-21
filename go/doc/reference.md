# Reference (Go)

The complete public surface of the Go `css` module: exports, the parse
entry points, the two options, and the exact CSS syntax accepted. For a
guided introduction see the [tutorial](tutorial.md); for task recipes
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

Parses a CSS string and returns the resulting value. Convenience
wrapper around `MakeJsonic(opts...).Parse(src)`.

With **no** options it reuses a single lazily-created instance, so
repeated calls do not rebuild the engine + grammar. The shared instance
is safe for concurrent use (each parse builds its own context and only
reads instance state). With options, a dedicated instance is built per
call, since the configuration differs per call.

```go
result, err := tabnascss.Parse(`a { color: red }`)
// result: map[string]any{"a": map[string]any{"color": "red"}}
```

### `func MakeJsonic(opts ...CssOptions) *jsonic.Jsonic`

Returns a reusable `*jsonic.Jsonic` instance configured for CSS
parsing. Use this when parsing many strings with the same options:
build once, call `.Parse()` per input.

```go
j := tabnascss.MakeJsonic()
result, err := j.Parse(`@media screen { a { color: blue } }`)
// result: map[string]any{"@media screen": map[string]any{"a": map[string]any{"color": "blue"}}}
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
result, err := j.Parse(`a { color: red }`)
```

### `var Defaults map[string]any`

The default option map, paired with `Css` for `UseDefaults`:

```go
var Defaults = map[string]any{
    "lowercaseProperties": false,
    "lowercaseValues":     false,
}
```

### `type CssOptions struct`

A typed wrapper over the option map. Fields are `*bool` so callers can
express "omit" (nil) vs "set":

```go
type CssOptions struct {
    // When non-nil and true, lowercases declaration property names (CSS
    // property names are case-insensitive). Selectors are left untouched.
    LowercaseProperties *bool

    // When non-nil and true, lowercases declaration values. Off by default
    // because parts of a value are case-sensitive.
    LowercaseValues *bool
}
```

## Options

### `LowercaseProperties`

- **Type:** `*bool`
- **Default:** `false` (nil)
- **Effect:** Controls the case of declaration property names (the
  identifier before `:`). Selectors and values are unaffected.
  - nil / `false` — the property name is kept verbatim. `COLOR` →
    `COLOR`.
  - `true` — the property name is lowercased. `COLOR` → `color`.

```go
yes := true
tabnascss.Parse(`A { COLOR: Red }`, tabnascss.CssOptions{LowercaseProperties: &yes})
// map[string]any{"A": map[string]any{"color": "Red"}}
```

### `LowercaseValues`

- **Type:** `*bool`
- **Default:** `false` (nil)
- **Effect:** Controls the case of declaration values (the raw string
  after `:`). Off by default because string contents, `url()` contents,
  and custom idents are case-sensitive.
  - nil / `false` — the value is kept verbatim. `Red` → `Red`.
  - `true` — the value is lowercased. `Red` → `red`.

```go
yes := true
tabnascss.Parse(`a { color: RED }`, tabnascss.CssOptions{LowercaseValues: &yes})
// map[string]any{"a": map[string]any{"color": "red"}}
```

## Value types

`Parse` returns `any`; the concrete Go types are predictable:

| CSS construct | Go type |
|---|---|
| Stylesheet | `map[string]any` (selector → block) |
| Declaration block / nested at-rule block | `map[string]any` |
| Declaration value | `string` (raw, untrimmed-internally text) |
| Statement at-rule parameters | `string` |
| Empty input (`""`) | `nil` |
| Non-empty whitespace / comment-only input | `map[string]any{}` |

There is no numeric type in the result tree: every value, including
`12px` or `0`, is a `string`.

## CSS syntax

The plugin parses a stylesheet as an implicit top-level map of rules
(no surrounding braces). It supplies all CSS structure through its own
rules, disabling jsonic's implicit maps/lists and rebinding the
punctuation tokens.

### Rulesets

A ruleset is `selector { declaration; declaration; ... }`. The selector
prelude — everything up to the opening `{`, trimmed — becomes the map
key verbatim; the block becomes a nested `map[string]any`.

```
a { color: red }            => {"a": {"color": "red"}}
a {}                        => {"a": {}}
a { color: red } b { ... }  => {"a": {...}, "b": {...}}
```

Selectors are kept exactly as written, including:

| Selector kind | Example key |
|---|---|
| Type | `a` |
| Class / id | `.foo`, `#bar` |
| Grouping (list) | `h1, h2` |
| Combinator | `.foo > .bar` |
| Pseudo-class | `a:hover` |
| Pseudo-element | `a::before` |
| Attribute | `input[type=text]` |

### Declarations

A declaration is `property : value`. The property is an identifier of
`[A-Za-z0-9_-]` characters (a leading `@` makes it a statement at-rule;
see below). The value runs to the next top-level `;` or `}`, trimmed of
trailing whitespace, and is returned as a raw `string`.

```
color: red                  => "color": "red"
border: 1px solid #fff      => "border": "1px solid #fff"
color: red !important       => "color": "red !important"
```

A trailing `;` before `}` (or end of input) is optional.

```
a { color: red }            same as  a { color: red; }
```

Inside a value, the scanner skips over strings, `( )`, `[ ]`, and
comments, so colons, commas, and braces nested in them do not end the
value:

```
background: url(http://x/y.png)   => "background": "url(http://x/y.png)"
color: rgb(1, 2, 3)               => "color": "rgb(1, 2, 3)"
font-family: "Helvetica Neue", Arial, sans-serif
                                  => one string value, commas included
```

### At-rules

There are two shapes.

**Block at-rule** — a prelude followed by `{ ... }`. The prelude
(`@media screen`, `@media (max-width: 600px)`) is the key; the block
recurses into a nested `map[string]any` of rules:

```
@media screen { a { color: blue } }
  => {"@media screen": {"a": {"color": "blue"}}}

@media (max-width: 600px) { a { color: red } }
  => {"@media (max-width: 600px)": {"a": {"color": "red"}}}
```

**Statement at-rule** — an at-keyword followed by parameters and a `;`,
with no block. It becomes a `property → value` pair: the at-keyword is
the key, the raw parameter text the value:

```
@import "base.css";         => {"@import": "\"base.css\""}
@charset "utf-8"; a { ... } => {"@charset": "\"utf-8\"", "a": {...}}
```

Note the parameters keep their quotes: values are never unquoted.

### Comments

Only `/* ... */` block comments are recognised, and they are discarded
wherever they appear. CSS has no `//` line comments, and `#` is not a
comment.

```
/* header */ a { color: red; /* note */ top: 0 }
  => {"a": {"color": "red", "top": "0"}}
```

### Empty input

A zero-length source runs no rules (an engine convention) and yields
`nil`. Any non-empty source — even pure whitespace or a comment-only
source — yields an empty stylesheet, `map[string]any{}`.

```
""                  => nil
"   \n  "           => map[string]any{}
"/* only */"        => map[string]any{}
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
| `#TX` | selector / property / at-keyword | a key |
| `#VL` | raw text run | a declaration or at-rule value |

Bare `[` and `]` are **not** structural tokens — they only appear
inside selectors/values, where the matcher consumes them as text.

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
