# Reference

The complete public surface of `@tabnas/css` (TypeScript): exports,
the parse entry, the two options, and the exact CSS syntax accepted.
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

## Exports

| Export | Kind | Description |
|---|---|---|
| `Css` | `Plugin` | The plugin function. Register with `engine.use(Css, options)`. |
| `CssOptions` | type | The options object shape (see [Options](#options)). |

`Css.defaults` (a `CssOptions`) holds the merged default options:

```typescript
Css.defaults = {
  lowercaseProperties: false,
  lowercaseValues: false,
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

c.parse('a { color: red }') // => { a: { color: 'red' } }
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

Parses a CSS source string and returns the resulting JavaScript value.
A stylesheet comes back as a nested map of `selector → { property →
value }`; maps are built with `Object.create(null)` (no prototype),
and declaration values are raw strings. A failed parse throws (see
[Errors](#errors)).

The empty-input quirk: a zero-length source (`''`) returns `undefined`,
because a zero-length source runs no rules (an engine convention). Any
non-empty source — even whitespace or a lone comment — yields an empty
stylesheet object `{}`.

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

c.parse('')                    // => undefined
c.parse('   ')                 // => {}
c.parse('/* only a comment */') // => {}
```

## Options

`CssOptions` has exactly two fields:

```typescript
type CssOptions = {
  lowercaseProperties: boolean
  lowercaseValues: boolean
}
```

### `lowercaseProperties`

- **Type:** `boolean`
- **Default:** `false`
- **Effect:** When `true`, lowercases declaration **property names**
  only. CSS property names are case-insensitive, so this normalises
  them. Selectors and values are left untouched.

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css, { lowercaseProperties: true })
c.parse('A { COLOR: Red }') // => { A: { color: 'Red' } }
```

### `lowercaseValues`

- **Type:** `boolean`
- **Default:** `false`
- **Effect:** When `true`, lowercases declaration **values**. Off by
  default because parts of a value (quoted strings, `url()` contents,
  custom identifiers) are case-sensitive.

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css, { lowercaseValues: true })
c.parse('a { color: RED }') // => { a: { color: 'red' } }
```

## CSS syntax

A stylesheet is an implicit, brace-free top-level map of rules. Each
rule is a key (the selector or at-rule prelude) mapping to a value.

### Rules and declarations

A rule is `selector { declaration; declaration; ... }`. Each
declaration is `property: value`. The selector becomes a key, its block
a nested map, and each property a key with its value as a raw string.
The trailing `;` is optional before the closing `}`.

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

c.parse('a { color: red; font-size: 12px }') // => { a: { color: 'red', 'font-size': '12px' } }
c.parse('a { color: red }')                  // => { a: { color: 'red' } }
c.parse('a {}')                              // => { a: {} }
```

### Selectors

The selector text is kept **verbatim** as the key — it is not parsed
into components. Grouping (`,`), combinators (`>`, `+`, `~`),
descendant whitespace, pseudo-classes (`:hover`), pseudo-elements
(`::before`), and attribute selectors (`[type=text]`) are all kept
exactly as written (with surrounding whitespace trimmed).

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

c.parse('h1, h2 { margin: 0 }')           // => { 'h1, h2': { margin: '0' } }
c.parse('.foo > .bar { top: 0 }')         // => { '.foo > .bar': { top: '0' } }
c.parse('input[type=text] { border: 0 }') // => { 'input[type=text]': { border: '0' } }
```

### Declaration values

A value is the run of text after `:` up to the next top-level `;` or
`}` (trimmed). It is kept as **one raw string** and is not parsed
further. Internal commas, hashes, spaces, `!important`, and balanced
`()` / `[]` (so a `;` inside `url(...)` or a function does not
terminate the value) are all part of the string.

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

c.parse('p { border: 1px solid #fff }')        // => { p: { border: '1px solid #fff' } }
c.parse('a { color: red !important }')          // => { a: { color: 'red !important' } }
c.parse('a { color: rgb(1, 2, 3); top: 0 }')    // => { a: { color: 'rgb(1, 2, 3)', top: '0' } }
```

### Nested at-rules

An at-rule with a block (e.g. `@media`, `@supports`) recurses: its
prelude (kept verbatim) is the key and its block is a nested map of
rules.

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

c.parse('@media screen { a { color: blue } }') // => { '@media screen': { a: { color: 'blue' } } }
```

### Statement at-rules

An at-rule with no block (e.g. `@import`, `@charset`) is a statement
**terminated by `;`**. It becomes a single key/value pair: the
at-keyword is the key, the rest of the statement (quotes included) is
the value.

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

c.parse('@import "base.css";')                // => { '@import': '"base.css"' }
c.parse('@charset "utf-8"; a { color: red }') // => { '@charset': '"utf-8"', a: { color: 'red' } }
```

A statement at-rule that is **not** terminated with `;` does not close,
and the parser reads following content as part of the same statement.

### Comments

Only `/* ... */` block comments are recognised. They are discarded
wherever they appear.

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

(Jsonic's `#` hash comments and `//` line comments are disabled by the
plugin.)

### Empty input

A zero-length source returns `undefined` (no rules run). Any non-empty
source — whitespace or a comment alone — yields an empty stylesheet
`{}`. An empty rule block yields an empty map.

```js
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '@tabnas/css'

const c = new Tabnas().use(jsonic).use(Css)

c.parse('')      // => undefined
c.parse('   ')   // => {}
c.parse('a {}')  // => { a: {} }
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
| `#TX` | text | a key: a selector or property name |
| `#AT` | text | a key: a statement at-keyword (e.g. `@import`) |
| `#VL` | text | a value: a declaration value (raw text) |

`#TX`, `#AT` and `#VL` are produced by the custom `cssToken` matcher,
which owns all non-punctuation text. The fixed `{`, `}`, `:` lex as
`#OB`, `#CB`, `#CL`; `;` is remapped to `#CA`. Bare `[` and `]` are
disabled as structure — they only ever appear inside selectors and
values, which the matcher consumes as text.

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

- A statement at-rule **must** be terminated with `;`.
- Declaration values are **not** parsed further — each is kept as one
  raw string.
- Selector text is kept **verbatim** — it is not parsed into its
  component parts.
- Only `/* ... */` block comments are recognised.
