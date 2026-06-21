# How-to guide

Short, task-focused recipes. Each is self-contained and assumes you
have the plugin installed (see the [tutorial](tutorial.md) for the
basics). For the full API, every option, and the complete syntax,
follow the links into the [reference](reference.md).

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

c.parse('a { color: red }') // => { a: { color: 'red' } }
```

The instance is reusable — build it once and call `.parse()` as many
times as you like. (Building the grammar is the expensive part; do not
reconstruct the instance per parse.)

## Parse a realistic stylesheet

A real stylesheet mixes plain rules, grouped and combinator selectors,
multi-part values, and nested at-rules:

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

const sheet = c.parse(`
  body {
    margin: 0;
    font-family: "Helvetica Neue", Arial, sans-serif;
  }
  .nav > li {
    display: inline-block;
    padding: 0 10px;
  }
  @media (min-width: 768px) {
    .nav > li { padding: 0 20px; }
  }
`)

sheet // => { body: { margin: '0', 'font-family': '"Helvetica Neue", Arial, sans-serif' }, '.nav > li': { display: 'inline-block', padding: '0 10px' }, '@media (min-width: 768px)': { '.nav > li': { padding: '0 20px' } } }
```

## Read compound declaration values

A declaration value is kept as one raw string, up to the next top-level
`;` or `}`. Commas, spaces, hashes, `!important`, and `url(...)` are all
part of that string — they are not parsed further:

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

c.parse('p { border: 1px solid #fff }')           // => { p: { border: '1px solid #fff' } }
c.parse('a { color: red !important }')             // => { a: { color: 'red !important' } }
c.parse('a { background: url(http://x/y.png) }')   // => { a: { background: 'url(http://x/y.png)' } }
c.parse('a { color: rgb(1, 2, 3); top: 0 }')       // => { a: { color: 'rgb(1, 2, 3)', top: '0' } }
```

## Keep selectors verbatim

Grouping, combinators, pseudo-classes, pseudo-elements, and attribute
selectors are all kept exactly as written and used as the rule key:

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

c.parse('h1, h2 { margin: 0 }')              // => { 'h1, h2': { margin: '0' } }
c.parse('.foo > .bar { top: 0 }')            // => { '.foo > .bar': { top: '0' } }
c.parse('a:hover { color: red }')            // => { 'a:hover': { color: 'red' } }
c.parse('a::before { content: "x" }')        // => { 'a::before': { content: '"x"' } }
c.parse('input[type=text] { border: 0 }')    // => { 'input[type=text]': { border: '0' } }
```

Note that `a:hover` is read as one selector, not as a property named
`a` — a leading `:` in key position is treated as a pseudo-class.

## Handle at-rules

A nested at-rule (one with a block) recurses into a map of rules; a
statement at-rule (terminated by `;`) becomes a single key/value pair
whose value is the rest of the statement, quotes included:

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

c.parse('@media screen { a { color: blue } }')   // => { '@media screen': { a: { color: 'blue' } } }
c.parse('@import "base.css";')                    // => { '@import': '"base.css"' }
c.parse('@charset "utf-8"; a { color: red }')     // => { '@charset': '"utf-8"', a: { color: 'red' } }
```

A statement at-rule **must** end with `;`. Without it the parser reads
the following content as part of the same statement.

## Normalise property name and value case

CSS property names are case-insensitive. Set `lowercaseProperties` to
lowercase declaration property names (selectors and values are
untouched), and `lowercaseValues` to lowercase declaration values:

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const props = new Tabnas().use(jsonic).use(Css, { lowercaseProperties: true })
props.parse('A { COLOR: Red }') // => { A: { color: 'Red' } }

const vals = new Tabnas().use(jsonic).use(Css, { lowercaseValues: true })
vals.parse('a { color: RED }') // => { a: { color: 'red' } }
```

`lowercaseValues` is off by default because parts of a value (strings,
`url()` contents, custom identifiers) are case-sensitive.

## Strip comments

Only `/* ... */` block comments are recognised; they are discarded
wherever they appear:

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

const sheet = c.parse(`/* header */ a {
  color: red; /* the colour */
  /* a gap */
  top: 0;
}`)

sheet // => { a: { color: 'red', top: '0' } }
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
