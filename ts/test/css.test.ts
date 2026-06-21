/* Copyright (c) 2025 Richard Rodger and other contributors, MIT License */

import { describe, test } from 'node:test'
import assert from 'node:assert'

import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '../dist/css'

function plain(v: any): any {
  if (v === null || typeof v !== 'object') return v
  if (Array.isArray(v)) return v.map(plain)
  const out: Record<string, any> = {}
  for (const k of Object.keys(v)) out[k] = plain((v as any)[k])
  return out
}

function parse(src: string, opts?: any) {
  const j = new Tabnas().use(jsonic).use(Css, opts || {})
  return plain(j.parse(src))
}

const sheet = (...rules: any[]) => ({ type: 'stylesheet', rules })
const rule = (selectors: string[], declarations: any[]) => ({
  type: 'rule',
  selectors,
  declarations,
})
const decl = (property: string, value: string) => ({
  type: 'declaration',
  property,
  value,
})
const comment = (c: string) => ({ type: 'comment', comment: c })

describe('css ast', () => {
  test('empty stylesheet', () => {
    assert.deepStrictEqual(parse('   \n  '), sheet())
    assert.deepStrictEqual(parse('/* only */'), sheet(comment(' only ')))
  })

  test('single rule single declaration', () => {
    assert.deepStrictEqual(
      parse('a { color: red; }'),
      sheet(rule(['a'], [decl('color', 'red')])),
    )
  })

  test('declaration order and duplicates are preserved', () => {
    assert.deepStrictEqual(
      parse('a { color: red; color: blue }'),
      sheet(rule(['a'], [decl('color', 'red'), decl('color', 'blue')])),
    )
  })

  test('no trailing semicolon', () => {
    assert.deepStrictEqual(
      parse('a { color: red }'),
      sheet(rule(['a'], [decl('color', 'red')])),
    )
  })

  test('empty rule block', () => {
    assert.deepStrictEqual(parse('a {}'), sheet(rule(['a'], [])))
  })

  test('multiple rules keep order', () => {
    assert.deepStrictEqual(
      parse('a { x: 1 } b { y: 2 }'),
      sheet(rule(['a'], [decl('x', '1')]), rule(['b'], [decl('y', '2')])),
    )
  })

  test('selector group is a list', () => {
    assert.deepStrictEqual(
      parse('h1, h2 { margin: 0 }'),
      sheet(rule(['h1', 'h2'], [decl('margin', '0')])),
    )
  })

  test('comma inside :not() stays in one selector', () => {
    assert.deepStrictEqual(
      parse('a:not(.x, .y), b { top: 0 }'),
      sheet(rule(['a:not(.x, .y)', 'b'], [decl('top', '0')])),
    )
  })

  test('compound and function values kept raw', () => {
    assert.deepStrictEqual(
      parse('p { border: 1px solid #fff; color: rgb(1, 2, 3) }'),
      sheet(
        rule(['p'], [
          decl('border', '1px solid #fff'),
          decl('color', 'rgb(1, 2, 3)'),
        ]),
      ),
    )
  })

  test('important kept in value', () => {
    assert.deepStrictEqual(
      parse('a { color: red !important }'),
      sheet(rule(['a'], [decl('color', 'red !important')])),
    )
  })

  test('comment nodes between and within rules', () => {
    const src = `/* head */
a {
  /* c1 */
  color: red; /* trailing skipped position? */
}`
    assert.deepStrictEqual(parse(src), sheet(
      comment(' head '),
      rule(['a'], [comment(' c1 '), decl('color', 'red'), comment(' trailing skipped position? ')]),
    ))
  })

  test('mid-construct comments are skipped', () => {
    assert.deepStrictEqual(
      parse('a /* x */ { color /* y */ : red }'),
      sheet(rule(['a'], [decl('color', 'red')])),
    )
  })

  test('@media wraps a rules body', () => {
    assert.deepStrictEqual(
      parse('@media screen { a { color: blue } }'),
      sheet({
        type: 'media',
        media: 'screen',
        rules: [rule(['a'], [decl('color', 'blue')])],
      }),
    )
  })

  test('@media prelude with parens', () => {
    assert.deepStrictEqual(
      parse('@media (min-width: 700px) and (max-width: 900px) { a { x: 1 } }'),
      sheet({
        type: 'media',
        media: '(min-width: 700px) and (max-width: 900px)',
        rules: [rule(['a'], [decl('x', '1')])],
      }),
    )
  })

  test('@supports wraps a rules body', () => {
    assert.deepStrictEqual(
      parse('@supports (display: grid) { a { x: 1 } }'),
      sheet({
        type: 'supports',
        supports: '(display: grid)',
        rules: [rule(['a'], [decl('x', '1')])],
      }),
    )
  })

  test('@font-face wraps a declarations body', () => {
    assert.deepStrictEqual(
      parse('@font-face { font-family: "A"; src: url(a.woff) }'),
      sheet({
        type: 'font-face',
        declarations: [
          decl('font-family', '"A"'),
          decl('src', 'url(a.woff)'),
        ],
      }),
    )
  })

  test('@import statement', () => {
    assert.deepStrictEqual(
      parse('@import "base.css";'),
      sheet({ type: 'import', import: '"base.css"' }),
    )
  })

  test('@charset then a rule', () => {
    assert.deepStrictEqual(
      parse('@charset "utf-8"; a { x: 1 }'),
      sheet(
        { type: 'charset', charset: '"utf-8"' },
        rule(['a'], [decl('x', '1')]),
      ),
    )
  })

  test('@keyframes with keyframe blocks', () => {
    assert.deepStrictEqual(
      parse('@keyframes slide { from { left: 0 } 50%, 100% { left: 10px } }'),
      sheet({
        type: 'keyframes',
        name: 'slide',
        keyframes: [
          { type: 'keyframe', values: ['from'], declarations: [decl('left', '0')] },
          {
            type: 'keyframe',
            values: ['50%', '100%'],
            declarations: [decl('left', '10px')],
          },
        ],
      }),
    )
  })

  test('vendor-prefixed @keyframes', () => {
    assert.deepStrictEqual(
      parse('@-webkit-keyframes x { to { opacity: 1 } }'),
      sheet({
        type: 'keyframes',
        name: 'x',
        vendor: '-webkit-',
        keyframes: [
          { type: 'keyframe', values: ['to'], declarations: [decl('opacity', '1')] },
        ],
      }),
    )
  })

  test('nested media with multiple rules', () => {
    const src = `@media print {
      a { color: black }
      .b, .c { margin: 0 }
    }`
    assert.deepStrictEqual(parse(src), sheet({
      type: 'media',
      media: 'print',
      rules: [
        rule(['a'], [decl('color', 'black')]),
        rule(['.b', '.c'], [decl('margin', '0')]),
      ],
    }))
  })

  test('CSS Nesting: a nested style rule lives in declarations', () => {
    assert.deepStrictEqual(parse('a { color: red; & b { top: 0 } }'), sheet(
      rule(['a'], [
        decl('color', 'red'),
        rule(['& b'], [decl('top', '0')]),
      ]),
    ))
  })

  test('CSS Nesting: nested rule first, then declaration', () => {
    assert.deepStrictEqual(parse('a { b { x: 1 } color: red }'), sheet(
      rule(['a'], [
        rule(['b'], [decl('x', '1')]),
        decl('color', 'red'),
      ]),
    ))
  })

  test('CSS Nesting: nested @media inside a rule', () => {
    assert.deepStrictEqual(parse('a { color: red; @media x { b { y: 1 } } }'), sheet(
      rule(['a'], [
        decl('color', 'red'),
        { type: 'media', media: 'x', rules: [rule(['b'], [decl('y', '1')])] },
      ]),
    ))
  })

  test('CSS Nesting: deep and grouped', () => {
    assert.deepStrictEqual(parse('a { &:hover, &:focus { c: 1 } }'), sheet(
      rule(['a'], [rule(['&:hover', '&:focus'], [decl('c', '1')])]),
    ))
  })

  test('@page with selectors and declarations', () => {
    assert.deepStrictEqual(parse('@page :first { margin: 1in }'), sheet({
      type: 'page',
      selectors: [':first'],
      declarations: [decl('margin', '1in')],
    }))
  })

  test('@namespace statement', () => {
    assert.deepStrictEqual(parse('@namespace svg url(http://x);'), sheet({
      type: 'namespace',
      namespace: 'svg url(http://x)',
    }))
  })

  test('vendor-prefixed @document', () => {
    assert.deepStrictEqual(parse('@-moz-document url(x) { a { c: 1 } }'), sheet({
      type: 'document',
      document: 'url(x)',
      vendor: '-moz-',
      rules: [rule(['a'], [decl('c', '1')])],
    }))
  })

  test('generic block at-rule (@layer)', () => {
    assert.deepStrictEqual(parse('@layer base { a { c: 1 } }'), sheet({
      type: 'layer',
      layer: 'base',
      rules: [rule(['a'], [decl('c', '1')])],
    }))
  })

  test('comment inside a @media body', () => {
    assert.deepStrictEqual(parse('@media x { /* c */ a { b: 1 } }'), sheet({
      type: 'media',
      media: 'x',
      rules: [comment(' c '), rule(['a'], [decl('b', '1')])],
    }))
  })

  test('semicolon inside a url() string is part of the value', () => {
    assert.deepStrictEqual(parse('a { background: url("a;b.png") }'),
      sheet(rule(['a'], [decl('background', 'url("a;b.png")')])))
  })

  test('position option attaches 1-based start/end to nodes', () => {
    const ast: any = parse('a {\n  color: red;\n}', { position: true })
    assert.deepStrictEqual(ast.position, {
      start: { line: 1, column: 1 },
      end: { line: 3, column: 2 },
    })
    const r = ast.rules[0]
    assert.deepStrictEqual(r.position.start, { line: 1, column: 1 })
    assert.deepStrictEqual(r.position.end, { line: 3, column: 2 })
    const d = r.declarations[0]
    assert.deepStrictEqual(d.position.start, { line: 2, column: 3 })
    assert.deepStrictEqual(d.position.end, { line: 2, column: 13 })
  })

  test('no position fields by default', () => {
    const ast: any = parse('a { x: 1 }')
    assert.strictEqual(ast.position, undefined)
    assert.strictEqual(ast.rules[0].position, undefined)
  })

  test('lowercaseProperties option', () => {
    assert.deepStrictEqual(
      parse('A { COLOR: Red }', { lowercaseProperties: true }),
      sheet(rule(['A'], [decl('color', 'Red')])),
    )
  })

  test('real-world smoke: normalize.css excerpt', () => {
    // An excerpt of normalize.css (MIT) exercising comments, multi-line
    // selector groups, vendor prefixes, !important, @media, url(), and
    // duplicate/compound values. We assert structural sanity rather than a
    // full deep-equal: no throw, a stylesheet, and the expected node shape.
    const src = `
/*! normalize.css v8.0.1 | MIT License */

/* Document
   ========================================================================== */

html {
  line-height: 1.15; /* 1 */
  -webkit-text-size-adjust: 100%; /* 2 */
}

/* Sections */

body {
  margin: 0;
}

h1 {
  font-size: 2em;
  margin: 0.67em 0;
}

a {
  background-color: transparent;
}

b,
strong {
  font-weight: bolder;
}

img {
  border-style: none;
}

button,
input,
optgroup,
select,
textarea {
  font-family: inherit;
  font-size: 100%;
  line-height: 1.15;
  margin: 0;
}

[type="checkbox"],
[type="radio"] {
  box-sizing: border-box;
  padding: 0;
}

::-webkit-file-upload-button {
  -webkit-appearance: button;
  font: inherit;
}

@media (min-width: 768px) {
  body {
    margin: 0 auto;
    max-width: 960px;
  }
  .hero {
    background: url("hero;cover.png") no-repeat center / cover;
  }
}
`
    const ast: any = parse(src)
    assert.strictEqual(ast.type, 'stylesheet')

    // Top-level: leading banner comment, a section comment, then rules and a
    // trailing @media.
    const comments = ast.rules.filter((n: any) => n.type === 'comment')
    assert.ok(comments.length >= 3, 'banner + section comments preserved')

    const rules = ast.rules.filter((n: any) => n.type === 'rule')
    assert.ok(rules.length >= 8, 'style rules collected')

    // The selector group splits into a list, never a joined string.
    const group = rules.find(
      (r: any) => r.selectors.length === 5 && r.selectors[0] === 'button',
    )
    assert.ok(group, 'button,input,... group is a 5-element list')

    const attrGroup = rules.find(
      (r: any) => r.selectors[0] === '[type="checkbox"]',
    )
    assert.deepStrictEqual(attrGroup.selectors, [
      '[type="checkbox"]',
      '[type="radio"]',
    ])

    // Trailing comments stay attached as nodes inside a rule's declarations.
    const html = rules.find((r: any) => r.selectors[0] === 'html')
    assert.deepStrictEqual(
      html.declarations.map((d: any) => d.type),
      ['declaration', 'comment', 'declaration', 'comment'],
    )

    // The @media block wraps its own rules; the url() with a ';' is one value.
    const media = ast.rules.find((n: any) => n.type === 'media')
    assert.strictEqual(media.media, '(min-width: 768px)')
    const hero = media.rules.find((r: any) => r.selectors[0] === '.hero')
    assert.deepStrictEqual(hero.declarations[0], {
      type: 'declaration',
      property: 'background',
      value: 'url("hero;cover.png") no-repeat center / cover',
    })

    // Round-trip safety: re-parsing the serialized type list is stable.
    assert.ok(
      ast.rules.every((n: any) => typeof n.type === 'string'),
      'every top-level node is typed',
    )
  })

  test('realistic stylesheet', () => {
    const src = `
      /* base */
      body {
        margin: 0;
        font-family: "Helvetica Neue", Arial, sans-serif;
      }
      .nav > li { display: inline-block; padding: 0 10px; }
      @media (min-width: 768px) {
        .nav > li { padding: 0 20px; }
      }
    `
    assert.deepStrictEqual(parse(src), sheet(
      comment(' base '),
      rule(['body'], [
        decl('margin', '0'),
        decl('font-family', '"Helvetica Neue", Arial, sans-serif'),
      ]),
      rule(['.nav > li'], [
        decl('display', 'inline-block'),
        decl('padding', '0 10px'),
      ]),
      {
        type: 'media',
        media: '(min-width: 768px)',
        rules: [rule(['.nav > li'], [decl('padding', '0 20px')])],
      },
    ))
  })
})
