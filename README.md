# @tabnas/css

A grammar plugin that teaches the [Tabnas](https://github.com/tabnas/parser)
parser to read [CSS](https://developer.mozilla.org/en-US/docs/Web/CSS)
(Cascading Style Sheets), turning a stylesheet into a plain nested object of
`selector → { property → value }`. Available for both TypeScript and Go,
built on the same grammar.

CSS looks like this:

```css
body {
  margin: 0;
  font-family: "Helvetica Neue", Arial, sans-serif;
}
.nav > li { display: inline-block; padding: 0 10px; }
@media (min-width: 768px) {
  .nav > li { padding: 0 20px; }
}
```

## Install

```bash
# TypeScript / JavaScript
npm install @tabnas/parser @tabnas/jsonic @tabnas/css

# Go
go get github.com/tabnas/css/go@latest
```

## One tiny example

**TypeScript** — the plugin layers onto a Tabnas engine:

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

c.parse('a { color: red; font-size: 12px }')
// => { a: { color: 'red', 'font-size': '12px' } }

c.parse('h1, h2 { margin: 0 }')
// => { 'h1, h2': { margin: '0' } }
```

**Go** — `tabnascss.Parse` is the one-call entry point:

```go
import tabnascss "github.com/tabnas/css/go"

result, _ := tabnascss.Parse(`a { color: red; font-size: 12px }`)
// map[string]any{"a": map[string]any{"color": "red", "font-size": "12px"}}
```

## What it produces

A stylesheet parses to a nested map:

- each **rule** is a key (the selector text, verbatim) mapping to a map of
  its declarations;
- each **declaration** is a key (the property name) mapping to its value as
  a raw string (e.g. `'1px solid #fff'`);
- **nested at-rules** (e.g. `@media`) recurse — the prelude is the key and
  the block is a nested map of rules;
- **statement at-rules** (e.g. `@import`) become a key (the at-keyword)
  mapping to the rest of the statement as a string.

```js
c.parse('@media screen { a { color: blue } }')
// => { '@media screen': { a: { color: 'blue' } } }

c.parse('@import "base.css";')
// => { '@import': '"base.css"' }
```

## Documentation

Full documentation follows the [Diátaxis](https://diataxis.fr)
framework — one file per quadrant, per language:

| | TypeScript | Go |
|---|---|---|
| **Tutorial** (learning) | [ts/doc/tutorial.md](ts/doc/tutorial.md) | [go/doc/tutorial.md](go/doc/tutorial.md) |
| **How-to guide** (tasks) | [ts/doc/guide.md](ts/doc/guide.md) | [go/doc/guide.md](go/doc/guide.md) |
| **Reference** (API + options + syntax) | [ts/doc/reference.md](ts/doc/reference.md) | [go/doc/reference.md](go/doc/reference.md) |
| **Concepts** (explanation) | [ts/doc/concepts.md](ts/doc/concepts.md) | [go/doc/concepts.md](go/doc/concepts.md) |

Per-language hubs: [`ts/README.md`](ts/README.md),
[`go/README.md`](go/README.md).

## Grammar diagram

The grammar is defined once in the top-level
[`css-grammar.jsonic`](css-grammar.jsonic) and embedded into both
implementations — TypeScript ([`ts/src/css.ts`](ts/src/css.ts)) and Go
([`go/css.go`](go/css.go)) — by [`ts/embed-grammar.js`](ts/embed-grammar.js)
during the TypeScript build. Edit the grammar there, not in the
generated sources.

As a railroad/syntax diagram, generated from the live grammar with
[`@tabnas/railroad`](https://github.com/tabnas/railroad):

![css grammar railroad diagram](ts/doc/grammar.svg)

ASCII version: [`ts/doc/grammar.txt`](ts/doc/grammar.txt).

## License

MIT. Copyright (c) Richard Rodger.
