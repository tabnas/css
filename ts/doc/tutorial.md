# Tutorial — your first CSS parse

This walks you from nothing to a working parse, then through one
option and one error. Follow it in order; each step builds on the
last. When you finish you will have installed the plugin, parsed a
rule and a grouped selector, nested an at-rule, switched on an option,
and handled a parse error.

For a recipe-style index of individual tasks, see the
[how-to guide](guide.md). For exhaustive signatures and the full
syntax, see the [reference](reference.md). For how it all works, see
[concepts](concepts.md).

## 1. Install

`@tabnas/css` is a grammar plugin: it has no parser of its own. It runs
on the Tabnas engine, with the relaxed-JSON grammar from
`@tabnas/jsonic` underneath. Install all three:

```bash
npm install @tabnas/parser @tabnas/jsonic @tabnas/css
```

`@tabnas/parser` (>= 2) and `@tabnas/jsonic` (>= 2) are peer
dependencies.

## 2. Build a parser

Create a Tabnas engine, layer the jsonic grammar onto it, then layer
the CSS plugin on top. The result is a reusable parser instance:

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

c.parse('a { color: red; font-size: 12px }') // => { a: { color: 'red', 'font-size': '12px' } }
```

You wrote an ordinary CSS rule — a selector, a brace-delimited block,
and `property: value` declarations — and got back a plain nested
object. That is the point: the plugin teaches the engine to read CSS.

## 3. Group and nest selectors

Selectors are kept verbatim as keys, including grouping (`h1, h2`) and
combinators (`.foo > .bar`):

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

c.parse('h1, h2 { margin: 0 }')     // => { 'h1, h2': { margin: '0' } }
c.parse('.foo > .bar { top: 0 }')   // => { '.foo > .bar': { top: '0' } }
```

The plugin never breaks a selector into components — the whole prelude,
trimmed, is the key. That keeps the output faithful to the source.

## 4. Nest an at-rule

A nested at-rule like `@media` recurses: its prelude is the key and its
block is itself a map of rules:

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

c.parse('@media screen { a { color: blue } }') // => { '@media screen': { a: { color: 'blue' } } }
```

A *statement* at-rule — one with no block, terminated by `;` — instead
becomes a single key/value pair. The at-keyword is the key, and the
rest of the statement (quotes and all) is the value:

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

c.parse('@import "base.css";') // => { '@import': '"base.css"' }
```

## 5. Turn on an option

The plugin is configured through its second `use()` argument. CSS
property names are case-insensitive, so you may want them normalised.
Set `lowercaseProperties: true` to lowercase declaration property names
(selectors and values are left untouched):

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css, { lowercaseProperties: true })

c.parse('A { COLOR: Red }') // => { A: { color: 'Red' } }
```

There are only two options, `lowercaseProperties` and
`lowercaseValues`; the [reference](reference.md#options) lists both
with their defaults.

## 6. Catch an error

A malformed stylesheet — for instance a rule whose block is never
closed — throws the engine's standard parse error:

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

let threw = false
try {
  c.parse('a { color: red') // unterminated block
} catch (e) {
  threw = true
}
threw // => true
```

The thrown error is the engine's standard parse error, with a code,
a source location, and a formatted message you can show a user.

## Where to go next

- [How-to guide](guide.md) — focused recipes for individual tasks.
- [Reference](reference.md) — the public API, every option, the full
  CSS syntax accepted.
- [Concepts](concepts.md) — how the plugin reshapes the engine, and
  why.
