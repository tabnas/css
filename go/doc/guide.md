# How-to guide (Go)

Short, task-focused recipes. Each is self-contained and assumes you
have the module installed (see the [tutorial](tutorial.md) for the
basics). For the full API, every option, and the complete syntax,
follow the links into the [reference](reference.md).

```go
import tabnascss "github.com/tabnas/css/go"
```

## Parse a single stylesheet

`tabnascss.Parse` is the simplest entry point — pass source, get a
value and an error:

```go
result, err := tabnascss.Parse(`a { color: red; font-size: 12px }`)
// result: map[string]any{"a": map[string]any{"color": "red", "font-size": "12px"}}
```

The no-options path reuses a single cached parser instance internally,
so repeated `tabnascss.Parse(src)` calls do not rebuild the engine each
time. It is safe for concurrent use.

## Read the parsed shape

The result is always a `map[string]any` (or `nil` for empty input).
Selectors are keys; each block is a nested `map[string]any`; each
declaration value is a raw `string`. Type-assert as you walk it:

```go
result, _ := tabnascss.Parse(`a { color: red }`)
sheet := result.(map[string]any)
rule := sheet["a"].(map[string]any)
color := rule["color"].(string) // "red"
```

## Parse a realistic stylesheet

Rules, selector lists, combinators, and a nested `@media` block all
mix freely:

```go
src := `
    body {
        margin: 0;
        font-family: "Helvetica Neue", Arial, sans-serif;
    }
    .nav > li {
        display: inline-block;
        padding: 0 10px;
    }
    @media (min-width: 768px) {
        .nav > li { padding: 0 20px; }
    }
`

result, err := tabnascss.Parse(src)
// result: map[string]any{
//   "body": map[string]any{
//     "margin":      "0",
//     "font-family": "\"Helvetica Neue\", Arial, sans-serif",
//   },
//   ".nav > li": map[string]any{
//     "display": "inline-block",
//     "padding": "0 10px",
//   },
//   "@media (min-width: 768px)": map[string]any{
//     ".nav > li": map[string]any{"padding": "0 20px"},
//   },
// }
```

## Keep compound and function values intact

A declaration value runs to the next top-level `;` or `}`, so
space-separated shorthands, `!important`, and functions (whose inner
commas and `:` are skipped) all stay as one raw string:

```go
tabnascss.Parse(`p { border: 1px solid #fff }`)
// map[string]any{"p": map[string]any{"border": "1px solid #fff"}}

tabnascss.Parse(`a { color: rgb(1, 2, 3); top: 0 }`)
// map[string]any{"a": map[string]any{"color": "rgb(1, 2, 3)", "top": "0"}}

tabnascss.Parse(`a { background: url(http://x/y.png) }`)
// map[string]any{"a": map[string]any{"background": "url(http://x/y.png)"}}

tabnascss.Parse(`a { color: red !important }`)
// map[string]any{"a": map[string]any{"color": "red !important"}}
```

## Parse a statement at-rule

A statement at-rule (no block, terminated by `;`) becomes a
`property → value` pair: the at-keyword is the key, its parameters the
raw-string value. It can be followed by ordinary rules:

```go
tabnascss.Parse(`@import "base.css";`)
// map[string]any{"@import": "\"base.css\""}

tabnascss.Parse(`@charset "utf-8"; a { color: red }`)
// map[string]any{"@charset": "\"utf-8\"", "a": map[string]any{"color": "red"}}
```

## Lowercase property names

CSS property names are case-insensitive. Set `LowercaseProperties` to
normalise them; selectors are left untouched:

```go
yes := true
tabnascss.Parse(`A { COLOR: Red }`, tabnascss.CssOptions{LowercaseProperties: &yes})
// map[string]any{"A": map[string]any{"color": "Red"}}
```

## Lowercase values

Off by default, because parts of a value (string contents, `url()`
contents, custom idents) are case-sensitive. Turn it on only when you
know the values are safe to fold:

```go
yes := true
tabnascss.Parse(`a { color: RED }`, tabnascss.CssOptions{LowercaseValues: &yes})
// map[string]any{"a": map[string]any{"color": "red"}}
```

## Skip comments

Only `/* ... */` block comments are recognised; they are discarded
wherever they appear. (CSS has no `//` line comments.)

```go
src := `/* header */ a {
    color: red; /* the colour */
    /* a gap */
    top: 0;
}`
tabnascss.Parse(src)
// map[string]any{"a": map[string]any{"color": "red", "top": "0"}}
```

## Reuse a parser for many inputs (with options)

`tabnascss.Parse(src, opts)` builds a dedicated instance per call when
you pass options, since the configuration differs per call. For a hot
loop with fixed options, build one instance with `MakeJsonic` and reuse
it:

```go
yes := true
j := tabnascss.MakeJsonic(tabnascss.CssOptions{LowercaseProperties: &yes})
for _, src := range inputs {
    result, err := j.Parse(src)
    _ = result
    _ = err
}
```

(With *no* options, plain `tabnascss.Parse(src)` already reuses a
cached instance, so you do not need `MakeJsonic` for that case.)

## Handle a parse error

`Parse` never panics on bad input; it returns an `error`:

```go
result, err := tabnascss.Parse(src)
if err != nil {
    // handle the syntax error; result is nil
}
```

## Switch the CSS grammar off while the plugin is loaded

Every grammar alternate the plugin adds carries the group tag `css`.
To switch those alts off — restoring the plain jsonic grammar while the
plugin stays registered — exclude that tag through the underlying
jsonic instance:

```go
import (
    jsonic "github.com/tabnas/jsonic/go"
    tabnascss "github.com/tabnas/css/go"
)

j := jsonic.Make()
j.UseDefaults(tabnascss.Css, tabnascss.Defaults)
j.SetOptions(jsonic.Options{Rule: &jsonic.RuleOptions{Exclude: "css"}})
```

This is rarely useful — you would normally just not load the plugin —
but it is the supported way to peel the CSS layer back off.
