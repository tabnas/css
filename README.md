# @tabnas/css

A grammar plugin that teaches the [Tabnas](https://github.com/tabnas/parser)
parser to read [CSS](https://developer.mozilla.org/en-US/docs/Web/CSS)
(Cascading Style Sheets) and produce a faithful **abstract syntax tree** —
ordered, typed nodes that preserve declaration order, duplicate properties,
rule types, and comments. The AST shape follows the widely-used
[`reworkcss/css`](https://github.com/reworkcss/css) model. Available for both
TypeScript and Go, built on the same grammar.

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

const ast = c.parse('a { color: red; font-size: 12px }')
ast.rules[0].type                  // => 'rule'
ast.rules[0].selectors             // => ['a']
ast.rules[0].declarations[1]       // => { type: 'declaration', property: 'font-size', value: '12px' }
```

The full tree:

```js
c.parse('h1, h2 { margin: 0 }')
// => { type: 'stylesheet', rules: [ { type: 'rule', selectors: ['h1','h2'], declarations: [ { type: 'declaration', property: 'margin', value: '0' } ] } ] }
```

**Go** — `tabnascss.Parse` is the one-call entry point:

```go
import tabnascss "github.com/tabnas/css/go"

ast, _ := tabnascss.Parse(`a { color: red }`)
// ast is map[string]any{"type":"stylesheet","rules":[]any{
//   map[string]any{"type":"rule","selectors":[]any{"a"},"declarations":[]any{
//     map[string]any{"type":"declaration","property":"color","value":"red"}}}}}
```

## The AST

A stylesheet is `{ type: 'stylesheet', rules: [ ...nodes ] }`. Each node has a
`type` discriminator:

| `type` | Fields |
|---|---|
| `rule` | `selectors: string[]`, `declarations: Node[]` |
| `declaration` | `property: string`, `value: string` (raw text) |
| `comment` | `comment: string` |
| `media` / `supports` / `document` / `host` | prelude field (e.g. `media`), `rules: Node[]` |
| `font-face` / `page` | `declarations: Node[]` (`page` also `selectors`) |
| `keyframes` | `name`, optional `vendor`, `keyframes: Node[]` (each a `keyframe` with `values` + `declarations`) |
| `import` / `charset` / `namespace` | the at-keyword field (e.g. `import`) |

Order and duplicates are preserved (arrays), comments are nodes, and selector
groups become a list:

```js
c.parse('@media screen { a { color: blue } }')
// => { type: 'stylesheet', rules: [ { type: 'media', media: 'screen', rules: [ { type: 'rule', selectors: ['a'], declarations: [ { type: 'declaration', property: 'color', value: 'blue' } ] } ] } ] }
```

**CSS Nesting** is supported — a style rule or at-rule nested inside a
declaration block is appended to the parent's `declarations`, in source order:

```js
c.parse('a { color: red; & b { top: 0 } }')
// rule 'a' declarations: [ {declaration color:red}, {rule selectors:['& b'] ...} ]
```

## Options

Pass options as the third `use` argument (TS) or to `Parse`/`MakeJsonic` (Go):

| Option | Default | Effect |
|---|---|---|
| `lowercaseProperties` | `false` | Lowercase declaration property names (only). |
| `position` | `false` | Attach `position: { start, end }` (1-based `{ line, column }`) to every node. |

```js
new Tabnas().use(jsonic).use(Css, { position: true })
  .parse('a {\n  color: red;\n}')
// every node gains e.g. position: { start: { line, column }, end: { line, column } }
```

## Documentation

Full documentation follows the [Diátaxis](https://diataxis.fr)
framework — one file per quadrant, per language:

| | TypeScript | Go |
|---|---|---|
| **Tutorial** (learning) | [ts/doc/tutorial.md](ts/doc/tutorial.md) | [go/doc/tutorial.md](go/doc/tutorial.md) |
| **How-to guide** (tasks) | [ts/doc/guide.md](ts/doc/guide.md) | [go/doc/guide.md](go/doc/guide.md) |
| **Reference** (API + options + AST) | [ts/doc/reference.md](ts/doc/reference.md) | [go/doc/reference.md](go/doc/reference.md) |
| **Concepts** (explanation) | [ts/doc/concepts.md](ts/doc/concepts.md) | [go/doc/concepts.md](go/doc/concepts.md) |

Per-language hubs: [`ts/README.md`](ts/README.md),
[`go/README.md`](go/README.md).

## Grammar diagram

The grammar is defined once in the top-level
[`css-grammar.jsonic`](css-grammar.jsonic) and embedded into both
implementations — TypeScript ([`ts/src/css.ts`](ts/src/css.ts)) and Go
([`go/css.go`](go/css.go)) — by [`ts/embed-grammar.js`](ts/embed-grammar.js)
during the TypeScript build. Edit the grammar there, not in the generated
sources.

As a railroad/syntax diagram, generated from the live grammar with
[`@tabnas/railroad`](https://github.com/tabnas/railroad):

![css grammar railroad diagram](ts/doc/grammar.svg)

ASCII version: [`ts/doc/grammar.txt`](ts/doc/grammar.txt).

## License

MIT. Copyright (c) Richard Rodger.
