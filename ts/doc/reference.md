# Reference

The complete public surface of `@tabnas/css` (TypeScript): exports,
the parse entry, the option, and every AST node the parser produces.
For a guided introduction see the [tutorial](tutorial.md); for task
recipes see the [how-to guide](guide.md); for how it works see
[concepts](concepts.md).

## Package

```bash
npm install @tabnas/parser @tabnas/jsonic @tabnas/css
```

| | |
|---|---|
| Package | `@tabnas/css` |
| Module type | CommonJS (`main: dist/css.js`, types `dist/css.d.ts`) |
| Peer deps | `@tabnas/parser` >= 2, `@tabnas/jsonic` >= 2 |
| Engine | `@tabnas/parser` (Tabnas) |
| Underlying grammar | `@tabnas/jsonic` |
| AST model | [`reworkcss/css`](https://github.com/reworkcss/css) |

## Exports

| Export | Kind | Description |
|---|---|---|
| `Css` | `Plugin` | The plugin function. Register with `engine.use(Css, options)`. |
| `CssOptions` | type | The options object shape (see [Options](#options)). |

`Css.defaults` (a `CssOptions`) holds the merged default options:

```typescript
Css.defaults = {
  lowercaseProperties: false,
}
```

## Parse entry

The plugin has **no convenience `parse()` function** of its own. You
parse by building a Tabnas engine, layering the jsonic grammar, then
the `Css` plugin, and calling the engine's `.parse()`:

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

c.parse('a { color: red }')
// => { type: 'stylesheet', rules: [ { type: 'rule', selectors: ['a'], declarations: [ { type: 'declaration', property: 'color', value: 'red' } ] } ] }
```

### `engine.use(Css, options?)`

Registers and immediately applies the plugin. Returns the engine, so
registrations chain (`new Tabnas().use(jsonic).use(Css, opts)`). The
plugin merges `options` over `Css.defaults`, installs the embedded CSS
grammar, and re-applies its jsonic option overrides (the `;` member
separator, disabled `[` `]` openers, the narrowed key set, `/* */`-only
comments, and the custom `cssToken` lex matcher).

The instance is reusable and stateless across parses; build it once
and reuse it. Building the grammar dominates a parse, so do not
reconstruct the engine per call.

### `engine.parse(src)`

Parses a CSS source string and returns the AST: a `stylesheet` node
(see [The AST](#the-ast)). A failed parse throws (see [Errors](#errors)).

The empty-input quirk: a zero-length source (`''`) returns `undefined`,
because a zero-length source runs no rules (an engine convention). Any
non-empty source — even whitespace or a lone comment — yields a
`stylesheet` node.

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

c.parse('')                     // => undefined
c.parse('   ')                  // => { type: 'stylesheet', rules: [] }
c.parse('/* only a comment */') // => { type: 'stylesheet', rules: [ { type: 'comment', comment: ' only a comment ' } ] }
```

## Options

`CssOptions` has exactly one field:

```typescript
type CssOptions = {
  lowercaseProperties: boolean
}
```

### `lowercaseProperties`

- **Type:** `boolean`
- **Default:** `false`
- **Effect:** When `true`, lowercases declaration **property names**
  only. CSS property names are case-insensitive, so this normalises
  them. Selectors, values, and at-rule preludes are left untouched.

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css, { lowercaseProperties: true })
c.parse('A { COLOR: Red }').rules[0].declarations[0]
// => { type: 'declaration', property: 'color', value: 'Red' }
```

## The AST

A stylesheet parses to `{ type: 'stylesheet', rules: [ ...Node ] }`.
Every node has a `type` discriminator and a small set of fields. Arrays
preserve source order (and duplicate properties).

| `type` | Fields |
|---|---|
| `stylesheet` | `rules: Node[]` |
| `rule` | `selectors: string[]`, `declarations: Node[]` |
| `declaration` | `property: string`, `value: string` (raw text) |
| `comment` | `comment: string` |
| `media` / `supports` / `document` / `host` | prelude field (e.g. `media`), `rules: Node[]` |
| `font-face` / `page` | `declarations: Node[]` (`page` also `selectors: string[]`) |
| `keyframes` | `name: string`, optional `vendor: string`, `keyframes: Node[]` |
| `keyframe` | `values: string[]`, `declarations: Node[]` |
| `import` / `charset` / `namespace` | the at-keyword field (e.g. `import`) |

### Rules and declarations

A style rule is `selector { property: value; ... }`. It becomes a
`rule` node: the selector(s) in `selectors`, each declaration a
`declaration` node in `declarations`. The trailing `;` is optional; an
empty block gives an empty `declarations` array.

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

c.parse('a { color: red; font-size: 12px }').rules[0].declarations.length // => 2
c.parse('a { color: red }').rules[0].declarations[0]  // => { type: 'declaration', property: 'color', value: 'red' }
c.parse('a {}').rules[0]                               // => { type: 'rule', selectors: ['a'], declarations: [] }
```

Order and duplicates are preserved (`color` appears twice here):

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

c.parse('a { color: red; color: blue }').rules[0].declarations
// => [ { type: 'declaration', property: 'color', value: 'red' }, { type: 'declaration', property: 'color', value: 'blue' } ]
```

### Selectors

Selector text is kept **verbatim** (trimmed) — it is not parsed into
components. Combinators (`>`, `+`, `~`), descendant whitespace,
pseudo-classes (`:hover`), pseudo-elements (`::before`), and attribute
selectors (`[type=text]`) all survive as-is.

A comma-**grouped** selector is the exception: it expands into a list
of selectors on one `rule` node. Commas nested inside `:not(...)` (or
strings, `()`, `[]`) are not split.

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

c.parse('h1, h2 { margin: 0 }').rules[0].selectors        // => ['h1', 'h2']
c.parse('.foo > .bar { top: 0 }').rules[0].selectors      // => ['.foo > .bar']
c.parse('a:not(.x, .y), b { top: 0 }').rules[0].selectors // => ['a:not(.x, .y)', 'b']
```

### Declaration values

A declaration `value` is the run of text after `:` up to the next
top-level `;` or `}` — trimmed, with comments stripped, kept as **one
raw string** and not parsed further. Internal commas, hashes, spaces,
`!important`, and balanced `()` / `[]` (so a `;` inside `url(...)` does
not terminate the value) are all part of the string. Quotes are kept.

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

const d = (src) => c.parse(src).rules[0].declarations[0].value
d('p { border: 1px solid #fff }')      // => '1px solid #fff'
d('a { color: red !important }')        // => 'red !important'
d('a { color: rgb(1, 2, 3) }')          // => 'rgb(1, 2, 3)'
d('a { src: "base.css" }')              // => '"base.css"'
```

### Block at-rules (rules body)

`@media`, `@supports`, `@document`, and `@host` become a node typed for
the keyword, with the prelude in a same-named field (`media`,
`supports`, `document`) and the body parsed into a nested `rules`
array. (`@host` carries no prelude field.)

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

c.parse('@media screen { a { color: blue } }').rules[0]
// => { type: 'media', media: 'screen', rules: [ { type: 'rule', selectors: ['a'], declarations: [ { type: 'declaration', property: 'color', value: 'blue' } ] } ] }
c.parse('@supports (display: grid) { a { x: 1 } }').rules[0].supports // => '(display: grid)'
```

### Block at-rules (declarations body)

`@font-face` and `@page` carry a `declarations` array instead of
`rules`. `@page` also has a `selectors` array for its prelude.

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

c.parse('@font-face { font-family: "A"; src: url(a.woff) }').rules[0]
// => { type: 'font-face', declarations: [ { type: 'declaration', property: 'font-family', value: '"A"' }, { type: 'declaration', property: 'src', value: 'url(a.woff)' } ] }
c.parse('@page :first { margin: 0 }').rules[0].selectors // => [':first']
```

### @keyframes

`@keyframes` becomes a node with `name` and a `keyframes` array; each
entry is a `keyframe` node with `values` (the selectors `from`, `to`,
`50%`, …) and its own `declarations`. A vendor prefix is surfaced in an
optional `vendor` field.

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

c.parse('@keyframes slide { from { left: 0 } 50%, 100% { left: 10px } }').rules[0]
// => { type: 'keyframes', name: 'slide', keyframes: [ { type: 'keyframe', values: ['from'], declarations: [ { type: 'declaration', property: 'left', value: '0' } ] }, { type: 'keyframe', values: ['50%', '100%'], declarations: [ { type: 'declaration', property: 'left', value: '10px' } ] } ] }
c.parse('@-webkit-keyframes x { to { opacity: 1 } }').rules[0].vendor // => '-webkit-'
```

### Statement at-rules

An at-rule with no block (e.g. `@import`, `@charset`, `@namespace`) is a
statement **terminated by `;`**. It becomes a leaf node whose `type` is
the at-keyword and whose same-named field holds the rest of the
statement (quotes included).

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

c.parse('@import "base.css";').rules[0]    // => { type: 'import', import: '"base.css"' }
c.parse('@charset "utf-8"; a { x: 1 }').rules[0] // => { type: 'charset', charset: '"utf-8"' }
```

A statement at-rule that is **not** terminated with `;` does not close,
and the parser reads following content as part of the same statement.

### Comments

Only `/* ... */` block comments are recognised. At a statement or
declaration-list position they become `comment` nodes (the text between
`/*` and `*/`), in order. A comment seen mid-construct — e.g. between a
property name and its `:` — is skipped.

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

c.parse('/* head */ a { x: 1 }').rules[0]            // => { type: 'comment', comment: ' head ' }
c.parse('a { /* c1 */ color: red }').rules[0].declarations[0] // => { type: 'comment', comment: ' c1 ' }
c.parse('a /* x */ { color /* y */ : red }').rules[0].declarations
// => [ { type: 'declaration', property: 'color', value: 'red' } ]
```

(Jsonic's `#` hash comments and `//` line comments are disabled by the
plugin.)

### Empty input

A zero-length source returns `undefined` (no rules run). Any non-empty
source — whitespace or a comment alone — yields a `stylesheet` node. An
empty rule block yields an empty `declarations` array.

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

c.parse('')      // => undefined
c.parse('   ')   // => { type: 'stylesheet', rules: [] }
c.parse('a {}').rules[0]  // => { type: 'rule', selectors: ['a'], declarations: [] }
```

## Tokens

The plugin's lexer produces these tokens (as surfaced in the railroad
diagram legend):

| Token | Source | Meaning |
|---|---|---|
| `#OB` | `{` | start of a block |
| `#CB` | `}` | end of a block |
| `#CL` | `:` | declaration separator |
| `#CA` | `;` | declaration terminator |
| `#TX` | text | a selector, keyframe value, or property name |
| `#GC` | `,` | a selector-group separator |
| `#VL` | text | a declaration value (raw text) |
| `#CC` | `/* */` | a comment (at a statement/declaration position) |
| `#ATR` | `@…` | an at-rule with a rules body (`@media`, `@supports`, …) |
| `#ATD` | `@…` | an at-rule with a declarations body (`@font-face`, `@page`) |
| `#ATK` | `@…` | `@keyframes` |
| `#ATS` | `@…` | a statement at-rule (`@import`, `@charset`, …) |

`#TX`, `#GC`, `#VL`, `#CC` and the `#AT*` tokens are produced by the
custom `cssToken` matcher, which owns all non-punctuation text. The
fixed `{`, `}`, `:` lex as `#OB`, `#CB`, `#CL`; `;` is remapped to
`#CA`. Bare `[` and `]` are disabled as structure — they only ever
appear inside selectors and values, which the matcher consumes as text.

## Grammar group tag

Every grammar alternate the plugin adds carries the group tag `css`.
Callers can switch the CSS alts off (restoring plain jsonic) via
`rule.exclude: 'css'`:

```typescript
const c = new Tabnas().use(jsonic).use(Css).options({
  rule: { exclude: 'css' },
})
```

## Errors

A failed parse throws the engine's standard parse error. It carries
the usual fields — an error `code`, the source location (`row`, `col`,
`pos`), the offending `src` fragment, and a formatted multi-line
`message` with a source-context extract. Malformed input — for example
an unterminated block — is an error.

## Limitations

- Declaration values are **not** parsed further — each is kept as one
  raw string.
- Selector text is kept **verbatim** — it is not parsed into its
  component parts (a top-level group is split into the `selectors`
  list).
- A statement at-rule **must** be terminated with `;`.
- Only `/* ... */` block comments are recognised.
- CSS Nesting (an at-rule or style rule nested inside a style rule's
  declaration block) is not supported.
