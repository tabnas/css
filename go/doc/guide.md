# How-to guide (Go)

Short, task-focused recipes. Each is self-contained and assumes you
have the module installed (see the [tutorial](tutorial.md) for the
basics). For the full API, every option, and the complete AST node
reference, follow the links into the [reference](reference.md).

```go
import tabnascss "github.com/tabnas/css/go"
```

The parser produces a reworkcss-style AST: every node is a
`map[string]any` with a `"type"` field, and arrays (`rules`,
`declarations`, `selectors`, `values`, `keyframes`) are `[]any`. All
values are `string`.

## Parse a single stylesheet

`tabnascss.Parse` is the simplest entry point — pass source, get the
AST and an error:

```go
ast, err := tabnascss.Parse(`a { color: red }`)
// ast: {type: "stylesheet", rules: [
//   {type: "rule", selectors: ["a"], declarations: [
//     {type: "declaration", property: "color", value: "red"} ] } ] }
```

The no-options path reuses a single cached parser instance internally,
so repeated `tabnascss.Parse(src)` calls do not rebuild the engine each
time. It is safe for concurrent use.

## Walk the AST

The result is a `map[string]any` (or `nil` for empty input). Dispatch
on each node's `"type"` field and type-assert the arrays as you go:

```go
ast, _ := tabnascss.Parse(`a { color: red } b { top: 0 }`)
sheet := ast.(map[string]any)
for _, n := range sheet["rules"].([]any) {
    node := n.(map[string]any)
    switch node["type"] {
    case "rule":
        sels := node["selectors"].([]any)
        for _, d := range node["declarations"].([]any) {
            decl := d.(map[string]any)
            _ = sels
            _ = decl["property"].(string)
            _ = decl["value"].(string)
        }
    case "comment":
        _ = node["comment"].(string)
    }
}
```

## Preserve declaration order and duplicates

The AST keeps declarations in source order, including repeated
properties — nothing is collapsed:

```go
tabnascss.Parse(`a { color: red; color: blue }`)
// rule declarations: [
//   {type: "declaration", property: "color", value: "red"},
//   {type: "declaration", property: "color", value: "blue"} ]
```

## Keep compound and function values intact

A declaration value runs to the next top-level `;` or `}`, trimmed,
with comments stripped, so space-separated shorthands, `!important`,
and functions (whose inner commas and `:` are skipped) all stay as one
raw string:

```go
tabnascss.Parse(`p { border: 1px solid #fff; color: rgb(1, 2, 3) }`)
// rule declarations: [
//   {type: "declaration", property: "border", value: "1px solid #fff"},
//   {type: "declaration", property: "color",  value: "rgb(1, 2, 3)"} ]

tabnascss.Parse(`a { color: red !important }`)
// declaration: {property: "color", value: "red !important"}
```

## Keep comments as nodes

`/* ... */` comments at statement positions (the stylesheet body, a
declaration list, a keyframe list, right after a `{`) become `comment`
nodes holding the raw inner text. Comments seen mid-construct (e.g.
between a property name and its `:`) are skipped.

```go
tabnascss.Parse(`/* head */ a { /* c1 */ color: red; /* c2 */ }`)
// stylesheet rules: [
//   {type: "comment", comment: " head "},
//   {type: "rule", selectors: ["a"], declarations: [
//     {type: "comment", comment: " c1 "},
//     {type: "declaration", property: "color", value: "red"},
//     {type: "comment", comment: " c2 "} ] } ]

tabnascss.Parse(`a /* x */ { color /* y */ : red }`)
// the mid-construct comments are skipped:
// {type: "rule", selectors: ["a"], declarations: [
//   {type: "declaration", property: "color", value: "red"} ] }
```

(Only `/* ... */` block comments exist in CSS — there are no `//` line
comments, and `#` is not a comment.)

## Read a grouped selector

A comma-grouped selector becomes a list on a single `rule` node; the
block is not duplicated. Commas inside `:not(...)`, strings, `()` or
`[]` are not split:

```go
tabnascss.Parse(`h1, h2 { margin: 0 }`)
// {type: "rule", selectors: ["h1", "h2"], declarations: [
//   {type: "declaration", property: "margin", value: "0"} ] }

tabnascss.Parse(`a:not(.x, .y), b { top: 0 }`)
// selectors: ["a:not(.x, .y)", "b"]
```

## Parse a block at-rule

`@media`, `@supports`, `@document`, and `@host` become typed nodes with
a prelude field and a recursively-parsed `rules []any` body:

```go
tabnascss.Parse(`@media screen { a { color: blue } }`)
// {type: "media", media: "screen", rules: [
//   {type: "rule", selectors: ["a"], declarations: [
//     {type: "declaration", property: "color", value: "blue"} ] } ] }

tabnascss.Parse(`@supports (display: grid) { a { x: 1 } }`)
// {type: "supports", supports: "(display: grid)", rules: [ ... ] }
```

## Parse @font-face and @page

These at-rules carry a `declarations []any` body instead of `rules`.
`@page` also carries a `selectors []any` (its prelude, if any):

```go
tabnascss.Parse(`@font-face { font-family: "A"; src: url(a.woff) }`)
// {type: "font-face", declarations: [
//   {type: "declaration", property: "font-family", value: `"A"`},
//   {type: "declaration", property: "src",         value: "url(a.woff)"} ] }
```

## Parse a statement at-rule

A statement at-rule (no block, terminated by `;`) becomes a leaf node
whose `type` is the at-keyword, with a same-named field holding the raw
params. It can be followed by ordinary rules:

```go
tabnascss.Parse(`@import "base.css";`)
// {type: "import", import: `"base.css"`}

tabnascss.Parse(`@charset "utf-8"; a { x: 1 }`)
// stylesheet rules: [
//   {type: "charset", charset: `"utf-8"`},
//   {type: "rule", selectors: ["a"], declarations: [
//     {type: "declaration", property: "x", value: "1"} ] } ]
```

The params keep their quotes: values are never unquoted.

## Parse @keyframes

`@keyframes` becomes a `keyframes` node with a `name` and a `keyframes
[]any` of `keyframe` nodes. Each keyframe has a `values []any` (its
selectors) and a `declarations []any`:

```go
tabnascss.Parse(`@keyframes slide { from { left: 0 } 50%, 100% { left: 10px } }`)
// {type: "keyframes", name: "slide", keyframes: [
//   {type: "keyframe", values: ["from"],        declarations: [ {property: "left", value: "0"} ]},
//   {type: "keyframe", values: ["50%", "100%"], declarations: [ {property: "left", value: "10px"} ]} ] }
```

A vendor-prefixed form additionally carries a `vendor` field:

```go
tabnascss.Parse(`@-webkit-keyframes x { to { opacity: 1 } }`)
// {type: "keyframes", name: "x", vendor: "-webkit-", keyframes: [
//   {type: "keyframe", values: ["to"], declarations: [ {property: "opacity", value: "1"} ]} ] }
```

## Lowercase property names

CSS property names are case-insensitive. Set `LowercaseProperties` to
normalise them; selectors and values are left untouched:

```go
yes := true
tabnascss.Parse(`A { COLOR: Red }`, tabnascss.CssOptions{LowercaseProperties: &yes})
// {type: "rule", selectors: ["A"], declarations: [
//   {type: "declaration", property: "color", value: "Red"} ] }
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
    ast, err := j.Parse(src)
    _ = ast
    _ = err
}
```

(With *no* options, plain `tabnascss.Parse(src)` already reuses a
cached instance, so you do not need `MakeJsonic` for that case.)

## Handle the empty cases

A zero-length source returns `nil`; any non-empty source — even pure
whitespace or a comment — returns a `stylesheet` node:

```go
tabnascss.Parse("")           // nil
tabnascss.Parse("   \n  ")    // {type: "stylesheet", rules: []}
tabnascss.Parse("/* only */") // {type: "stylesheet", rules: [ {type: "comment", comment: " only "} ]}
```

## Handle a parse error

`Parse` never panics on bad input; it returns an `error`:

```go
ast, err := tabnascss.Parse(src)
if err != nil {
    // handle the syntax error; ast is nil
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
