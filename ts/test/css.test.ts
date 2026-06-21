/* Copyright (c) 2025 Richard Rodger and other contributors, MIT License */

import { describe, test } from 'node:test'
import assert from 'node:assert'

import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '../dist/css'

// Jsonic builds maps with Object.create(null); normalise to plain objects so
// assert.deepStrictEqual can compare against JSON-style literals.
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

describe('css', () => {
  test('empty stylesheet', () => {
    // A zero-length source runs no rules (an engine convention) and yields
    // undefined; any non-empty source (even whitespace or comments) yields
    // an empty stylesheet object.
    assert.strictEqual(parse(''), undefined)
    assert.deepStrictEqual(parse('   \n  '), {})
    assert.deepStrictEqual(parse('/* only a comment */'), {})
  })

  test('single rule single declaration', () => {
    assert.deepStrictEqual(parse('a { color: red; }'), {
      a: { color: 'red' },
    })
  })

  test('declaration without trailing semicolon', () => {
    assert.deepStrictEqual(parse('a { color: red }'), { a: { color: 'red' } })
  })

  test('multiple declarations', () => {
    assert.deepStrictEqual(parse('a { color: red; font-size: 12px; }'), {
      a: { color: 'red', 'font-size': '12px' },
    })
  })

  test('empty rule block', () => {
    assert.deepStrictEqual(parse('a {}'), { a: {} })
  })

  test('multiple rules', () => {
    assert.deepStrictEqual(parse('a { color: red } b { color: blue }'), {
      a: { color: 'red' },
      b: { color: 'blue' },
    })
  })

  test('compound value', () => {
    assert.deepStrictEqual(parse('p { border: 1px solid #fff; }'), {
      p: { border: '1px solid #fff' },
    })
  })

  test('selector grouping expands to one key per selector', () => {
    assert.deepStrictEqual(parse('h1, h2 { margin: 0 }'), {
      h1: { margin: '0' },
      h2: { margin: '0' },
    })
  })

  test('grouped selectors get independent value copies', () => {
    const r: any = parse('h1, h2 { margin: 0 }')
    r.h1.margin = 'changed'
    assert.deepStrictEqual(r.h2, { margin: '0' })
  })

  test('commas inside :not() do not split the selector', () => {
    assert.deepStrictEqual(parse('a:not(.x, .y), b { top: 0 }'), {
      'a:not(.x, .y)': { top: '0' },
      b: { top: '0' },
    })
  })

  test('at-rule prelude comma list is not split', () => {
    assert.deepStrictEqual(parse('@media screen, print { a { color: red } }'), {
      '@media screen, print': { a: { color: 'red' } },
    })
  })

  test('combinator and class selectors', () => {
    assert.deepStrictEqual(parse('.foo > .bar { top: 0 }'), {
      '.foo > .bar': { top: '0' },
    })
  })

  test('pseudo-class selector not confused with declaration', () => {
    assert.deepStrictEqual(parse('a:hover { color: red }'), {
      'a:hover': { color: 'red' },
    })
  })

  test('pseudo-element selector', () => {
    assert.deepStrictEqual(parse('a::before { content: "x" }'), {
      'a::before': { content: '"x"' },
    })
  })

  test('attribute selector', () => {
    assert.deepStrictEqual(parse('input[type=text] { border: 0 }'), {
      'input[type=text]': { border: '0' },
    })
  })

  test('value containing a colon (url)', () => {
    assert.deepStrictEqual(parse('a { background: url(http://x/y.png) }'), {
      a: { background: 'url(http://x/y.png)' },
    })
  })

  test('function value with internal semicolon-free commas', () => {
    assert.deepStrictEqual(parse('a { color: rgb(1, 2, 3); top: 0 }'), {
      a: { color: 'rgb(1, 2, 3)', top: '0' },
    })
  })

  test('block comments are ignored', () => {
    const src = `/* header */ a {
      color: red; /* the colour */
      /* a gap */
      top: 0;
    }`
    assert.deepStrictEqual(parse(src), { a: { color: 'red', top: '0' } })
  })

  test('nested at-rule', () => {
    assert.deepStrictEqual(parse('@media screen { a { color: blue } }'), {
      '@media screen': { a: { color: 'blue' } },
    })
  })

  test('at-rule prelude with parens', () => {
    assert.deepStrictEqual(
      parse('@media (max-width: 600px) { a { color: red } }'),
      { '@media (max-width: 600px)': { a: { color: 'red' } } },
    )
  })

  test('statement at-rule', () => {
    assert.deepStrictEqual(parse('@import "base.css";'), {
      '@import': '"base.css"',
    })
  })

  test('statement at-rule then rule', () => {
    assert.deepStrictEqual(parse('@charset "utf-8"; a { color: red }'), {
      '@charset': '"utf-8"',
      a: { color: 'red' },
    })
  })

  test('important is part of the value', () => {
    assert.deepStrictEqual(parse('a { color: red !important }'), {
      a: { color: 'red !important' },
    })
  })

  test('realistic stylesheet', () => {
    const src = `
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
    `
    assert.deepStrictEqual(parse(src), {
      body: {
        margin: '0',
        'font-family': '"Helvetica Neue", Arial, sans-serif',
      },
      '.nav > li': {
        display: 'inline-block',
        padding: '0 10px',
      },
      '@media (min-width: 768px)': {
        '.nav > li': { padding: '0 20px' },
      },
    })
  })

  test('lowercaseProperties option', () => {
    assert.deepStrictEqual(parse('A { COLOR: Red }', { lowercaseProperties: true }), {
      A: { color: 'Red' },
    })
  })

  test('lowercaseValues option', () => {
    assert.deepStrictEqual(parse('a { color: RED }', { lowercaseValues: true }), {
      a: { color: 'red' },
    })
  })
})
