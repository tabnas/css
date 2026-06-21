# css (Go)

A jsonic grammar plugin that parses [CSS](https://developer.mozilla.org/en-US/docs/Web/CSS)
(Cascading Style Sheets) into Go values — a nested map of
`selector → { property → value }`.

## Install

```bash
go get github.com/tabnas/css/go@latest
```

```go
import tabnascss "github.com/tabnas/css/go"
```

## One example

`tabnascss.Parse` is the one-call entry point — pass source, get a value and
an `error`:

```go
result, err := tabnascss.Parse(`a { color: red; font-size: 12px }`)
// result: map[string]any{"a": map[string]any{"color": "red", "font-size": "12px"}}

result, err = tabnascss.Parse(`@media screen { a { color: blue } }`)
// result: map[string]any{"@media screen": map[string]any{"a": map[string]any{"color": "blue"}}}
```

Maps come back as `map[string]any`; declaration values are raw strings. The
no-options `Parse` path reuses a cached instance internally and is safe for
concurrent use; for hot loops with options, build one instance with
`tabnascss.MakeJsonic` and reuse it.

## Documentation

Full documentation follows the [Diátaxis](https://diataxis.fr)
framework:

- [Tutorial](doc/tutorial.md) — a guided first parse, start to finish.
- [How-to guide](doc/guide.md) — short recipes for individual tasks.
- [Reference](doc/reference.md) — the public API, every option, and the
  complete CSS syntax accepted.
- [Concepts](doc/concepts.md) — how the plugin reshapes the engine, and
  how the Go version differs from TypeScript.

For the canonical TypeScript implementation, see
[`../ts/README.md`](../ts/README.md).

## Grammar

The grammar is defined once in the top-level
[`css-grammar.jsonic`](../css-grammar.jsonic) and embedded into this Go
source ([`css.go`](css.go)) and the TypeScript source during the build.
Edit the grammar there, not in the generated source.

A railroad/syntax diagram of the grammar is in
[`../ts/doc/grammar.svg`](../ts/doc/grammar.svg) (ASCII version:
[`../ts/doc/grammar.txt`](../ts/doc/grammar.txt)).

## License

Copyright (c) 2025 Richard Rodger and other contributors, MIT License.
