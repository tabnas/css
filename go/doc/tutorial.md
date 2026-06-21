# Tutorial — your first CSS parse (Go)

This walks you from nothing to a working parse, then through selector
groups, comments, at-rules, keyframes, and one option. Follow it in
order; each step builds on the last. When you finish you will have
installed the module, parsed a rule into an AST, read its nodes,
grouped selectors, kept comments, nested a block at-rule, parsed a
statement at-rule and a `@keyframes` block, and switched on an option.

The parser produces a faithful **reworkcss-style AST**: ordered, typed
nodes that preserve declaration order, duplicate properties, rule
types, and comments — not a lossy map.

For a recipe-style index of individual tasks, see the
[how-to guide](guide.md). For exhaustive signatures and the full node
reference, see the [reference](reference.md). For how it all works — and
how the Go version differs from TypeScript — see
[concepts](concepts.md).

## 1. Install

`css` is a jsonic plugin. The convenience helpers pull in the jsonic
engine for you, so a single `go get` is enough:

```bash
go get github.com/tabnas/css/go@latest
```

```go
import tabnascss "github.com/tabnas/css/go"
```

## 2. Parse a rule

`tabnascss.Parse` is the one-call entry point. Give it CSS source and
it returns the AST as `any` plus an `error`:

```go
ast, err := tabnascss.Parse(`a { color: red }`)
// ast: map[string]any{"type": "stylesheet", "rules": []any{
//   map[string]any{"type": "rule", "selectors": []any{"a"}, "declarations": []any{
//     map[string]any{"type": "declaration", "property": "color", "value": "red"}}}}}
// err: nil
```

Every node is a `map[string]any` with a `"type"` discriminator. The top
node is always a `stylesheet`, whose `rules` is a `[]any` of child
nodes. A style rule has a `selectors []any` and a `declarations []any`;
each declaration has a `property` and a raw-string `value`. The parser
does not interpret colours, lengths, or numbers — every value is a
`string`. A trailing `;` is optional, so `a { color: red }` parses the
same as `a { color: red; }`.

## 3. Read the AST

Walk it with ordinary type assertions:

```go
ast, _ := tabnascss.Parse(`a { color: red }`)
sheet := ast.(map[string]any)
rule := sheet["rules"].([]any)[0].(map[string]any)
decl := rule["declarations"].([]any)[0].(map[string]any)
color := decl["value"].(string) // "red"
```

Because the arrays preserve order, duplicate properties survive too —
`a { color: red; color: blue }` yields two `declaration` nodes in the
order written, not a single overwritten entry.

## 4. Group selectors

A single selector is kept verbatim, including combinators,
pseudo-classes, and attribute selectors. A comma-**grouped** selector
becomes a list of selector strings on one `rule` node — the block is
*not* duplicated:

```go
ast, _ := tabnascss.Parse(`h1, h2 { margin: 0 }`)
// the rule node: map[string]any{"type": "rule",
//   "selectors": []any{"h1", "h2"},
//   "declarations": []any{
//     map[string]any{"type": "declaration", "property": "margin", "value": "0"}}}
```

Each selector is trimmed of surrounding whitespace. Commas nested
inside `:not(...)`, strings, `()` or `[]` are not split, so
`a:not(.x, .y), b` becomes `selectors: ["a:not(.x, .y)", "b"]`.

## 5. Keep comments

`/* ... */` comments at statement positions become `comment` nodes
(holding the raw inner text), preserving where they appeared. Comments
mid-construct (e.g. between a property and its `:`) are skipped.

```go
ast, _ := tabnascss.Parse(`/* head */ a { /* c1 */ color: red }`)
// rules: [ {type: "comment", comment: " head "},
//          {type: "rule", selectors: ["a"], declarations: [
//            {type: "comment", comment: " c1 "},
//            {type: "declaration", property: "color", value: "red"} ] } ]
```

## 6. Nest a block at-rule

A block at-rule such as `@media` becomes a typed node carrying its
prelude and a `rules []any` body that recurses exactly like the
top-level stylesheet:

```go
ast, _ := tabnascss.Parse(`@media screen { a { color: blue } }`)
// the media node: map[string]any{"type": "media", "media": "screen",
//   "rules": []any{
//     map[string]any{"type": "rule", "selectors": []any{"a"}, "declarations": []any{
//       map[string]any{"type": "declaration", "property": "color", "value": "blue"}}}}}
```

`@supports`, `@document`, and `@host` work the same way (with their own
prelude field). `@font-face` and `@page` instead carry a
`declarations []any` body.

## 7. Parse a statement at-rule

A statement at-rule such as `@import` has no block — it ends at `;`. It
becomes a leaf node whose `type` is the at-keyword, with a field of the
same name holding the raw params:

```go
ast, _ := tabnascss.Parse(`@import "base.css";`)
// the import node: map[string]any{"type": "import", "import": `"base.css"`}
```

The params keep their surrounding quotes: values are never unquoted or
otherwise decoded. `@charset` and `@namespace` follow the same shape.

## 8. Parse a @keyframes block

`@keyframes` becomes a `keyframes` node with a `name` and a `keyframes
[]any` of `keyframe` nodes; each keyframe has a `values []any` (the
`from` / `to` / `50%` selectors) and its own `declarations []any`:

```go
ast, _ := tabnascss.Parse(`@keyframes slide { from { left: 0 } 50%, 100% { left: 10px } }`)
// the keyframes node: map[string]any{"type": "keyframes", "name": "slide",
//   "keyframes": []any{
//     map[string]any{"type": "keyframe", "values": []any{"from"},
//       "declarations": []any{ {type: "declaration", property: "left", value: "0"} }},
//     map[string]any{"type": "keyframe", "values": []any{"50%", "100%"},
//       "declarations": []any{ {type: "declaration", property: "left", value: "10px"} }}}}
```

A vendor-prefixed `@-webkit-keyframes` additionally carries
`"vendor": "-webkit-"`.

## 9. Turn on an option

Options are passed as a `tabnascss.CssOptions` value after the source.
CSS property names are case-insensitive; set `LowercaseProperties` to
normalise them (selectors and values are left untouched):

```go
yes := true
ast, _ := tabnascss.Parse(
    `A { COLOR: Red }`,
    tabnascss.CssOptions{LowercaseProperties: &yes},
)
// the rule node: selectors ["A"], declarations [
//   {type: "declaration", property: "color", value: "Red"} ]
```

The field is `*bool` so you can express "leave it at the default"
(nil) versus "set it". A second option, `Position`, attaches a
1-based `start`/`end` line/column `"position"` to every node when set;
the [reference](reference.md#options) covers both in full.

You can also nest a rule (or a block at-rule) inside another rule's
declaration block — the nested node is appended to the parent's
`declarations` in source order:

```go
tabnascss.Parse(`a { color: red; & b { top: 0 } }`)
// rule declarations: [
//   {type: "declaration", property: "color", value: "red"},
//   {type: "rule", selectors: ["& b"], declarations: [
//     {type: "declaration", property: "top", value: "0"} ] } ]
```

## 10. The empty cases

A zero-length source returns `nil` (an engine convention); any
non-empty source — even pure whitespace or a comment — returns a
`stylesheet` node:

```go
tabnascss.Parse("")              // nil
tabnascss.Parse("   \n  ")       // {type: "stylesheet", rules: []}
tabnascss.Parse("/* only */")    // {type: "stylesheet", rules: [ {type: "comment", comment: " only "} ]}
```

`Parse` returns an `error` rather than panicking, so always check it.

## Where to go next

- [How-to guide](guide.md) — focused recipes for individual tasks.
- [Reference](reference.md) — the public API, every option, the full
  AST node reference.
- [Concepts](concepts.md) — how the plugin reshapes the engine, and
  how the Go version differs from TypeScript.
