# How-to guide

Short, task-focused recipes. Each is self-contained and assumes you
have the plugin installed (see the [tutorial](tutorial.md) for the
basics). For the full API, the option, and the complete AST node
reference, follow the links into the [reference](reference.md).

Every recipe starts from the same three imports:

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'
```

## Use it as a plugin

`Css` is a plugin, not a standalone parser. Layer it onto a Tabnas
engine that already has the jsonic grammar, then call `.parse()`:

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

c.parse('a { color: red }')
// => { type: 'stylesheet', rules: [ { type: 'rule', selectors: ['a'], declarations: [ { type: 'declaration', property: 'color', value: 'red' } ] } ] }
```

The instance is reusable — build it once and call `.parse()` as many
times as you like. (Building the grammar is the expensive part; do not
reconstruct the instance per parse.)

## Walk the AST

The result is always a `stylesheet` node with a `rules` array. Each
entry has a `type` you switch on; drill in with array indices and field
names:

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

const ast = c.parse('a { color: red } b { top: 0 }')
ast.rules.length                 // => 2
ast.rules[0].type                // => 'rule'
ast.rules[0].selectors           // => ['a']
ast.rules[1].declarations[0]     // => { type: 'declaration', property: 'top', value: '0' }
```

## Preserve order and duplicates

The AST keeps declarations in source order and never collapses repeated
properties — both `color` declarations survive:

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

const decls = c.parse('a { color: red; color: blue }').rules[0].declarations
decls.length    // => 2
decls[0].value  // => 'red'
decls[1].value  // => 'blue'
```

## Read compound declaration values

A declaration's `value` is one raw, trimmed string, up to the next
top-level `;` or `}`. Commas, spaces, hashes, `!important`, and
`url(...)` are all part of that string — they are not parsed further,
and quotes are kept:

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

const d = (src) => c.parse(src).rules[0].declarations[0].value
d('p { border: 1px solid #fff }')         // => '1px solid #fff'
d('a { color: red !important }')           // => 'red !important'
d('a { background: url(http://x/y.png) }') // => 'url(http://x/y.png)'
d('a { font-family: "A B", sans-serif }')  // => '"A B", sans-serif'
```

## Keep selectors verbatim

Combinators, pseudo-classes, pseudo-elements, and attribute selectors
are all kept exactly as written in the `selectors` array (a leading `:`
is read as a pseudo-class, not a property):

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

const sel = (src) => c.parse(src).rules[0].selectors
sel('.foo > .bar { top: 0 }')         // => ['.foo > .bar']
sel('a:hover { color: red }')         // => ['a:hover']
sel('a::before { content: "x" }')     // => ['a::before']
sel('input[type=text] { border: 0 }') // => ['input[type=text]']
```

A comma-grouped selector becomes a list of selectors on one rule node.
Commas inside `:not(...)` and the like are not split:

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

c.parse('h1, h2 { margin: 0 }').rules[0].selectors          // => ['h1', 'h2']
c.parse('a:not(.x, .y), b { top: 0 }').rules[0].selectors   // => ['a:not(.x, .y)', 'b']
```

## Handle block at-rules

A block at-rule whose body is rules (e.g. `@media`, `@supports`)
becomes a node typed for the keyword, with the prelude in a same-named
field and the body in a nested `rules` array:

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

c.parse('@media screen { a { color: blue } }').rules[0].type       // => 'media'
c.parse('@media screen { a { color: blue } }').rules[0].media      // => 'screen'
c.parse('@supports (display: grid) { a { x: 1 } }').rules[0].type  // => 'supports'
```

`@font-face` and `@page` instead carry a `declarations` array (no
`rules`); `@page` also has a `selectors` array:

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

c.parse('@font-face { font-family: "A"; src: url(a.woff) }').rules[0]
// => { type: 'font-face', declarations: [ { type: 'declaration', property: 'font-family', value: '"A"' }, { type: 'declaration', property: 'src', value: 'url(a.woff)' } ] }
```

## Handle statement at-rules

A statement at-rule (terminated by `;`) becomes a leaf node: its `type`
is the at-keyword, and a same-named field holds the rest of the
statement, quotes included:

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

c.parse('@import "base.css";').rules[0]   // => { type: 'import', import: '"base.css"' }
c.parse('@charset "utf-8"; a { x: 1 }').rules[0] // => { type: 'charset', charset: '"utf-8"' }
```

A statement at-rule **must** end with `;`. Without it the parser reads
the following content as part of the same statement.

## Handle @keyframes

`@keyframes` becomes a node with a `name` and a `keyframes` array; each
entry is a `keyframe` node with a `values` list (`from`, `to`, `50%`,
…) and its own `declarations`. A vendor prefix (e.g. `-webkit-`) is
surfaced in a `vendor` field:

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

const kf = c.parse('@keyframes slide { from { left: 0 } 50%, 100% { left: 10px } }').rules[0]
kf.name                  // => 'slide'
kf.keyframes[0].values   // => ['from']
kf.keyframes[1].values   // => ['50%', '100%']

c.parse('@-webkit-keyframes x { to { opacity: 1 } }').rules[0].vendor // => '-webkit-'
```

## Nest rules and at-rules

A style rule **or** an at-rule may appear inside another style rule's
declaration block (CSS Nesting). The nested node is appended to the
parent rule's `declarations` array, interleaved with declarations in
source order. (A `#TX` identifier followed by `:` is a declaration; one
followed by `{` or `,` is a nested style rule.)

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

c.parse('a { color: red; & b { top: 0 } }').rules[0].declarations
// => [ { type: 'declaration', property: 'color', value: 'red' }, { type: 'rule', selectors: ['& b'], declarations: [ { type: 'declaration', property: 'top', value: '0' } ] } ]

c.parse('a { color: red; @media x { b { y: 1 } } }').rules[0].declarations[1]
// => { type: 'media', media: 'x', rules: [ { type: 'rule', selectors: ['b'], declarations: [ { type: 'declaration', property: 'y', value: '1' } ] } ] }
```

## Capture comments

`/* ... */` block comments at a statement or declaration-list position
become `comment` nodes (carrying the text between `/*` and `*/`), in
order. Comments seen mid-construct — e.g. between a property and its
`:` — are skipped:

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

c.parse('/* head */ a { color: red }').rules[0]  // => { type: 'comment', comment: ' head ' }

const ast = c.parse('a { /* c1 */ color: red }')
ast.rules[0].declarations[0]  // => { type: 'comment', comment: ' c1 ' }
ast.rules[0].declarations[1]  // => { type: 'declaration', property: 'color', value: 'red' }

c.parse('a /* x */ { color /* y */ : red }').rules[0].declarations
// => [ { type: 'declaration', property: 'color', value: 'red' } ]
```

Only `/* ... */` block comments are recognised; jsonic's `#` and `//`
comments are disabled by the plugin.

## Normalise property name case

CSS property names are case-insensitive. Set `lowercaseProperties` to
lowercase declaration property names (selectors and values are
untouched):

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const props = new Tabnas().use(jsonic).use(Css, { lowercaseProperties: true })
props.parse('A { COLOR: Red }').rules[0].declarations[0]
// => { type: 'declaration', property: 'color', value: 'Red' }
```

## Get source positions

Set `position: true` to attach a `position` to every node. Each carries
`start` and `end` objects with 1-based `line` and `column`: `start` is
the node's first character, and `end` is just past its last (the closing
`}` of a block, the end of a value, the end of a comment's text).
Without the option, nodes have no `position` field at all.

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css, { position: true })

const ast = c.parse('a {\n  color: red;\n}')
ast.position                            // => { start: { line: 1, column: 1 }, end: { line: 3, column: 2 } }
ast.rules[0].position                   // => { start: { line: 1, column: 1 }, end: { line: 3, column: 2 } }
ast.rules[0].declarations[0].position   // => { start: { line: 2, column: 3 }, end: { line: 2, column: 13 } }
```

## Handle a parse error

A malformed stylesheet throws the engine's parse error; catch it and
read its fields:

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

let threw = false
try {
  c.parse('a { color: red') // unterminated block
} catch (err) {
  threw = true
  // err.code, err.row, err.col, err.message are available here.
}
threw // => true
```

## Re-enable strict JSON while the plugin is loaded

Every grammar alternate the plugin adds carries the group tag `css`.
To switch those alts off — restoring the plain jsonic grammar while
the plugin stays registered — exclude that tag:

```typescript
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css).options({
  rule: { exclude: 'css' },
})
```

This is rarely useful — you would normally just not load the plugin —
but it is the supported way to peel the CSS layer back off.
