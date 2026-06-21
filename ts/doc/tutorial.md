# Tutorial — your first CSS parse

This walks you from nothing to a working parse, then through the AST,
an at-rule, an option, and a parse error. Follow it in order; each step
builds on the last. When you finish you will have installed the plugin,
parsed a rule into a typed syntax tree, read a grouped selector,
wrapped an at-rule, switched on an option, and handled a parse error.

For a recipe-style index of individual tasks, see the
[how-to guide](guide.md). For exhaustive signatures and the full node
reference, see the [reference](reference.md). For how it all works, see
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

const ast = c.parse('a { color: red; font-size: 12px }')
ast.type                       // => 'stylesheet'
ast.rules[0].type              // => 'rule'
ast.rules[0].selectors         // => ['a']
ast.rules[0].declarations[1]   // => { type: 'declaration', property: 'font-size', value: '12px' }
```

You wrote an ordinary CSS rule — a selector, a brace-delimited block,
and `property: value` declarations — and got back an **abstract syntax
tree**: a `stylesheet` node whose `rules` array holds one typed `rule`
node, itself holding typed `declaration` nodes. That is the point: the
plugin teaches the engine to read CSS into the
[`reworkcss/css`](https://github.com/reworkcss/css) AST shape.

## 3. Read the tree

Every node has a `type` discriminator and a small set of fields. The
top is always a `stylesheet` with a `rules` array. Drill in by index:

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

const ast = c.parse('a { color: red; color: blue }')
ast.rules[0].declarations.length   // => 2
ast.rules[0].declarations[0]       // => { type: 'declaration', property: 'color', value: 'red' }
ast.rules[0].declarations[1]       // => { type: 'declaration', property: 'color', value: 'blue' }
```

Because the AST uses ordered arrays, declaration order and even
**duplicate** properties (`color` twice, here) are preserved — nothing
is collapsed into a map.

## 4. Group selectors

A single selector is kept verbatim, including combinators
(`.foo > .bar`). A comma-**grouped** selector becomes a list of
selectors on one rule node — the block is not duplicated:

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

const ast = c.parse('h1, h2 { margin: 0 }')
ast.rules[0].selectors   // => ['h1', 'h2']
```

Apart from splitting a top-level group, the plugin never breaks a
selector into components — the whole selector text, trimmed, is one
string. That keeps the output faithful to the source.

## 5. Wrap an at-rule

A *block* at-rule like `@media` becomes its own typed node. Its prelude
goes in a field named for the keyword (`media`), and its body parses
into a nested `rules` array:

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

const ast = c.parse('@media screen { a { color: blue } }')
ast.rules[0].type                       // => 'media'
ast.rules[0].media                      // => 'screen'
ast.rules[0].rules[0].selectors         // => ['a']
```

A *statement* at-rule — one with no block, terminated by `;` — instead
becomes a leaf node. Its `type` is the at-keyword, and a field of the
same name carries the rest of the statement (quotes and all):

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

c.parse('@import "base.css";').rules[0] // => { type: 'import', import: '"base.css"' }
```

## 6. Turn on an option

The plugin is configured through its second `use()` argument. CSS
property names are case-insensitive, so you may want them normalised.
Set `lowercaseProperties: true` to lowercase declaration property names
(selectors and values are left untouched):

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css, { lowercaseProperties: true })

const ast = c.parse('A { COLOR: Red }')
ast.rules[0].selectors        // => ['A']
ast.rules[0].declarations[0]  // => { type: 'declaration', property: 'color', value: 'Red' }
```

`lowercaseProperties` is the plugin's one option; the
[reference](reference.md#options) lists it with its default.

## 7. Catch an error

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
- [Reference](reference.md) — the public API, the option, and the full
  AST node reference.
- [Concepts](concepts.md) — how the plugin reshapes the engine, and
  why.
