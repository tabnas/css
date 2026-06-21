# @tabnas/css

A [Tabnas](https://github.com/tabnas/parser) grammar plugin that parses
[CSS](https://developer.mozilla.org/en-US/docs/Web/CSS) into a faithful
abstract syntax tree (the [`reworkcss/css`](https://github.com/reworkcss/css)
model): ordered, typed nodes that preserve declaration order, duplicate
properties, rule types, and comments.

## Install

```bash
npm install @tabnas/parser @tabnas/jsonic @tabnas/css
```

Requires `@tabnas/parser` >= 2 and `@tabnas/jsonic` >= 2 as peer
dependencies.

## One example

The plugin layers onto a Tabnas engine that already has the jsonic
grammar:

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

c.parse('a { color: red }')
// => { type: 'stylesheet', rules: [ { type: 'rule', selectors: ['a'], declarations: [ { type: 'declaration', property: 'color', value: 'red' } ] } ] }
```

Build the instance once and reuse it — constructing the grammar is the
expensive part.

## Documentation

Full documentation follows the [Diátaxis](https://diataxis.fr)
framework:

- [Tutorial](doc/tutorial.md) — a guided first parse, start to finish.
- [How-to guide](doc/guide.md) — short recipes for individual tasks.
- [Reference](doc/reference.md) — the public API, every option, and the
  complete AST node reference.
- [Concepts](doc/concepts.md) — how the plugin reshapes the engine, and
  why.

For the Go port, see [`../go/README.md`](../go/README.md).

## Grammar diagram

The grammar is defined in the top-level
[`css-grammar.jsonic`](../css-grammar.jsonic) and embedded into this
implementation (and the Go port) by [`embed-grammar.js`](embed-grammar.js)
during the build.

The installed grammar as a railroad/syntax diagram, generated with
[`@tabnas/railroad`](https://github.com/tabnas/railroad):

![css grammar railroad diagram](doc/grammar.svg)

A vertical ASCII version is in [`doc/grammar.txt`](doc/grammar.txt).

## License

Copyright (c) 2025 Richard Rodger and other contributors,
[MIT License](LICENSE).
