# Tutorial — your first CSS parse (Go)

This walks you from nothing to a working parse, then through nesting,
at-rules, and one option. Follow it in order; each step builds on the
last. When you finish you will have installed the module, parsed a
rule, grouped selectors, nested an at-rule, parsed a statement at-rule,
and switched on an option.

For a recipe-style index of individual tasks, see the
[how-to guide](guide.md). For exhaustive signatures and the full
syntax, see the [reference](reference.md). For how it all works — and
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
it returns the parsed value as `any` plus an `error`:

```go
result, err := tabnascss.Parse(`a { color: red; font-size: 12px }`)
// result: map[string]any{"a": map[string]any{"color": "red", "font-size": "12px"}}
// err:    nil
```

A stylesheet becomes a `map[string]any` keyed by selector; each rule's
block is itself a `map[string]any` of `property → value`. Every
declaration value comes back as a raw `string` — the parser does not
interpret colours, lengths, or numbers. A trailing `;` is optional, so
`a { color: red }` parses the same as `a { color: red; }`.

## 3. Group and combine selectors

A selector is kept verbatim as the map key, including selector lists
(grouping with a comma), combinators, pseudo-classes, and attribute
selectors:

```go
result, err := tabnascss.Parse(`h1, h2 { margin: 0 }`)
// result: map[string]any{"h1, h2": map[string]any{"margin": "0"}}

result, err = tabnascss.Parse(`.nav > li:hover { color: red }`)
// result: map[string]any{".nav > li:hover": map[string]any{"color": "red"}}
```

The whole prelude up to the opening `{` is the key, trimmed of trailing
whitespace, so you never quote or escape it — just write the selector.

## 4. Nest an at-rule

A block at-rule such as `@media` keeps its prelude (`@media screen`) as
the key, and its block recurses into another `map[string]any` of rules:

```go
result, err := tabnascss.Parse(`@media screen { a { color: blue } }`)
// result: map[string]any{
//   "@media screen": map[string]any{
//     "a": map[string]any{"color": "blue"},
//   },
// }
```

This nests to any depth: the inner block is parsed exactly like a
top-level stylesheet.

## 5. Parse a statement at-rule

A statement at-rule such as `@import` has no block — it ends at `;`.
Its at-keyword becomes the key and its parameters become the raw-string
value:

```go
result, err := tabnascss.Parse(`@import "base.css";`)
// result: map[string]any{"@import": "\"base.css\""}
```

Note the value keeps its surrounding quotes: declaration and at-rule
values are never unquoted or otherwise decoded.

## 6. Turn on an option

Options are passed as a `tabnascss.CssOptions` value after the source.
For example, CSS property names are case-insensitive; set
`LowercaseProperties` to normalise them (selectors are left untouched):

```go
yes := true
result, err := tabnascss.Parse(
    `A { COLOR: Red }`,
    tabnascss.CssOptions{LowercaseProperties: &yes},
)
// result: map[string]any{"A": map[string]any{"color": "Red"}}
```

The fields are `*bool` so you can express "leave it at the default"
(nil) versus "set it". There are only two options,
`LowercaseProperties` and `LowercaseValues`; the
[reference](reference.md#options) covers both.

## 7. The empty cases

A zero-length source runs no rules and returns `nil`; any non-empty
source — even pure whitespace or a comment — returns an empty
stylesheet map:

```go
tabnascss.Parse("")                     // nil
tabnascss.Parse("   \n  ")              // map[string]any{}
tabnascss.Parse("/* only a comment */") // map[string]any{}
```

`Parse` returns an `error` rather than panicking, so always check it.

## Where to go next

- [How-to guide](guide.md) — focused recipes for individual tasks.
- [Reference](reference.md) — the public API, every option, the full
  CSS syntax accepted.
- [Concepts](concepts.md) — how the plugin reshapes the engine, and
  how the Go version differs from TypeScript.
