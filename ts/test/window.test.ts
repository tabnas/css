/* Copyright (c) 2026 Richard Rodger and other contributors, MIT License */

// A parse from a reader (`tn.parseReader`, admin ADR-30 and ADR-33).
//
// `make()` builds the parser whole, as `new Tabnas().use(jsonic).use(Css,
// options)` builds it, and declares `lex.window` on it, so that a reader is
// read a window at a time. The cssToken matcher reads the source only
// through the window: a scan that reaches the window's end fills it and the
// token is cut again. These tests hold it to that: `make()` declares the
// window and parses as that composition does; every row of the shared
// fixtures, under its `opts` and again with positions on, and every document
// of the reworkcss corpus, with and without positions, parses the same from
// a reader, in pieces of 1, 3 and 64 bytes, under windows of a few UTF-16
// units and the engine's own, as from its text; and tokens of every kind
// longer than the window, and a long document, parse the same too.

import { test, describe } from 'node:test'
import assert from 'node:assert'
import { readdirSync } from 'node:fs'
import { join } from 'node:path'
import { inspect } from 'node:util'

import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { findSpecDir, loadSpec } from '@tabnas/support'

import { Css, make } from '../dist/css'
import type { CssOptions } from '../dist/css'

import { casesDir, present, readCase, extractErrorCases } from './reworkcss-corpus'

const SPEC = findSpecDir(__dirname)

type Reader = { read(buf: Uint8Array): number }
type Sizes = { margin: number; block: number; chunk: number }
type Seen = { value?: unknown; error?: string }

// A reader that hands its bytes over at most `size` at a time.
function pieces(bytes: Uint8Array, size: number): Reader {
  let at = 0
  return {
    read(buf: Uint8Array): number {
      const n = Math.min(size, buf.length, bytes.length - at)
      buf.set(bytes.subarray(at, at + n))
      at += n
      return n
    },
  }
}

// What a parse shows: its value, or its error's code, position, row,
// column and token text. An error's excerpt is left out: a parse from a
// reader shows a long line only as far back as the window reaches
// (tabnas/parser DIVERGENCE.md), the one difference allowed.
function seen(parse: () => unknown): Seen {
  try {
    return { value: parse() }
  } catch (e: any) {
    if (null == e || 'function' !== typeof e.toJSON) {
      return { error: 'throw ' + (e && e.name) + ' ' + (e && e.message) }
    }
    const d = e.toJSON()
    return {
      error:
        d.code + ' at ' + d.pos + ' ' + d.row + ':' + d.col + ' ' +
        JSON.stringify(d.token.src),
    }
  }
}

const show = (o: Seen): string =>
  undefined !== o.error ? 'error ' + o.error : 'value ' + inspect(o.value, { depth: 6 })

function same(a: Seen, b: Seen): boolean {
  if (a.error !== b.error) return false
  try {
    assert.deepStrictEqual(a.value, b.value)
    return true
  } catch (e) {
    return false
  }
}

// The pieces a reader hands its bytes over in, and the windows (margin and
// block) it is read under besides the engine's own: a few units, so that
// every scan of the matcher meets the window's edge.
const PIECES = [1, 3, 64]
const WINDOWS = [[1, 1], [3, 2], [64, 64]]

// `text` parsed by `tn` whole, and from a reader in every pieces size,
// under the engine's window and each of WINDOWS: how many reader parses
// were compared. A difference is pushed onto `failures`.
function sameFromReader(tn: Tabnas, at: string, text: string, failures: string[]): number {
  const whole = seen(() => tn.parse(text))
  const bytes = Buffer.from(text, 'utf8')
  const anyTn = tn as any
  let compared = 0
  for (const size of PIECES) {
    const reads: [string, () => unknown][] = [['', () => anyTn.parseReader(pieces(bytes, size))]]
    for (const [margin, block] of WINDOWS) {
      const sizes: Sizes = { margin, block, chunk: size }
      reads.push([
        ', margin ' + margin + ', block ' + block,
        () => anyTn.parseReaderWithSizes(pieces(bytes, size), sizes),
      ])
    }
    for (const [how, parse] of reads) {
      const read = seen(parse)
      compared++
      if (!same(read, whole)) {
        failures.push(
          at + ': ' + JSON.stringify(text.slice(0, 80)) + ' from a reader in pieces of ' +
          size + how + '\n  whole:  ' + show(whole) + '\n  reader: ' + show(read),
        )
      }
    }
  }
  return compared
}

const optionsOf = (cell: string): Partial<CssOptions> =>
  '' === cell.trim() ? {} : JSON.parse(cell)

// A parser per option set, built once: building one costs more than a
// fixture's parse, and a parser is reused across parses.
function cached(build: (options: Partial<CssOptions>) => Tabnas) {
  const built = new Map<string, Tabnas>()
  return (options: Partial<CssOptions>): Tabnas => {
    const key = JSON.stringify(options)
    let tn = built.get(key)
    if (undefined === tn) {
      tn = build(options)
      built.set(key, tn)
    }
    return tn
  }
}
const madeFor = cached((options) => make(options))
const composedFor = cached((options) => new Tabnas().use(jsonic).use(Css, options))

// Every row of the shared fixtures: where it is, its input and its options.
function fixtureRows(): { at: string; input: string; options: Partial<CssOptions> }[] {
  const rows = []
  for (const file of readdirSync(SPEC).filter((f) => f.endsWith('.tsv')).sort()) {
    for (const row of loadSpec(join(SPEC, file)).rows) {
      rows.push({
        at: file + ':' + row.line,
        input: row.unescNamed('input'),
        options: optionsOf(row.named('opts')),
      })
    }
  }
  return rows
}

describe('window', () => {
  test('make-declares-the-window', () => {
    const tn = make() as any
    assert.equal(tn.internal().config.lex.window, true)
    // The plugin alone declares nothing: a host may install it beneath
    // matchers of its own.
    const composed = new Tabnas().use(jsonic).use(Css, {}) as any
    assert.ok(!composed.internal().config.lex.window)
  })

  test('make-parses-as-the-composition-does', () => {
    // Every fixture row, whole, through `make(opts)` and through
    // `new Tabnas().use(jsonic).use(Css, opts)`, which dmog composed.
    let compared = 0
    const failures: string[] = []
    for (const { at, input, options } of fixtureRows()) {
      const made = seen(() => madeFor(options).parse(input))
      const composed = seen(() => composedFor(options).parse(input))
      compared++
      if (!same(made, composed)) {
        failures.push(at + '\n  make:     ' + show(made) + '\n  composed: ' + show(composed))
      }
    }
    assert.ok(150 < compared, '' + compared)
    assert.deepEqual(failures, [])
  })

  test('every-row-parses-the-same-from-a-reader', () => {
    // Under the row's options, and again with positions on, whose columns
    // count from the start of a line the window may have dropped.
    const rows = fixtureRows()
    let compared = 0
    const failures: string[] = []
    rows.forEach(({ at, input, options }, index) => {
      compared += sameFromReader(madeFor(options), at, input, failures)
      if (!options.position) {
        compared += sameFromReader(
          madeFor({ ...options, position: true }), at + ' (position)', input, failures)
      }
      if (0 === (index + 1) % 100 || index + 1 === rows.length) {
        console.log('window fixtures: ' + (index + 1) + ' of ' + rows.length + ' rows (' +
          Math.floor(((index + 1) * 100) / rows.length) + '%), ' + compared + ' reader parses')
      }
    })
    assert.ok(4000 < compared, '' + compared)
    assert.deepEqual(failures, [])
  })

  test('every-scan-meets-the-window-edge', () => {
    // Each scan of the matcher with what it skips (a comment, a string, an
    // escape, brackets) where a window of a few units ends inside it, and
    // each of its errors and the end-of-input overshoot.
    const docs = [
      'a { b: c /* x */ d }',
      'a { b: /* x; } */ c; d: e }',
      'a { b: "x;}" c }',
      'a { b: c\\;d; e: f }',
      'a { b: f(x;y) [z}] }',
      'a /* x */ , b { c: d }',
      'a[x="{;}"], b:not(.c, .d) { e: f }',
      'a\\{b { c: d }',
      '@media /* x { */ screen { a { b: c } }',
      '@import "a;b" /* c; */;',
      '@import url(a;b);',
      '@font-face { src: url(a) }',
      '@-webkit-keyframes k { 0%, /* x */ 50% { a: b } to { a: c } }',
      'a { b/**/: c }',
      'a { *zoom: 1; _x: 2; opacity[sqrt]: 3; //y: 4 }',
      '/* a */ /* b */ x { y: z }',
      '@host\\',
      'a { b: c\\',
      '.a\\',
      'a { b: c } /* open',
      'a { b: c /* open',
      '@media x /* open',
      'a /* open',
      'a { b: c } /',
      'é\u{1d11e} { b: \u{1d11e}é } /* \u{1d11e} */ c { d: e }',
    ]
    let compared = 0
    const failures: string[] = []
    for (const options of [{}, { position: true }]) {
      const tn = make(options)
      for (const doc of docs) {
        compared += sameFromReader(tn, JSON.stringify(options), doc, failures)
      }
    }
    assert.ok(500 < compared, '' + compared)
    assert.deepEqual(failures, [])
  })

  test('the-corpus-parses-the-same-from-a-reader', () => {
    // Every case of the reworkcss corpus, with and without positions, and
    // every input of its accept/reject oracle.
    assert.ok(present, 'the reworkcss corpus is not fetched: npm run install-reworkcss-tests')
    const names = readdirSync(casesDir).sort()
    const plain = make()
    const positioned = make({ position: true })
    let compared = 0
    const failures: string[] = []
    for (const name of names) {
      const input = readCase(join(casesDir, name, 'input.css'))
      compared += sameFromReader(plain, 'cases/' + name, input, failures)
      compared += sameFromReader(positioned, 'cases/' + name + ' (position)', input, failures)
    }
    const oracle = extractErrorCases()
    for (const c of oracle) {
      compared += sameFromReader(plain, 'parse.js ' + c.kind, c.src, failures)
    }
    console.log('window corpus: ' + names.length + ' cases and ' + oracle.length +
      ' oracle inputs (100%), ' + compared + ' reader parses')
    assert.equal(names.length, 46)
    assert.ok(1000 < compared, '' + compared)
    assert.deepEqual(failures, [])
  })

  test('tokens-longer-than-the-window-parse-the-same-from-a-reader', () => {
    // The engine's window reaches 64 KiB past the cursor before each token:
    // a token longer than that, or a lookahead that reads further, meets
    // its edge under the engine's own sizes.
    const long = (64 << 10) + 2048
    const third = Math.floor(long / 3)
    const docs = [
      // A selector, and a group whose `{` lies past the window, past a
      // comment too.
      '.' + 's'.repeat(long) + ' { b: c }',
      'h1 ' + 'x'.repeat(third) + ', h2 ' + 'y'.repeat(third) + ', h3 ' + 'z'.repeat(third) + ' { b: c }',
      'a /* ' + '{;}'.repeat(third) + ' */ , b { c: d }',
      // A property name, and a value, with a string holding `;` and `}`.
      'a { ' + 'p'.repeat(long) + ': c }',
      'a { b: ' + 'v'.repeat(long) + ' }',
      'a { b: "' + ';}'.repeat(long / 2) + '"; c: d }',
      'a { b: c /* ' + ';}'.repeat(long / 2) + ' */ d; e: f }',
      // A comment node, and a comment the declaration skips.
      '/* ' + 'c'.repeat(long) + ' */ a { b: c }',
      'a { b /* ' + 'c'.repeat(long) + ' */ : c }',
      // An at-rule's prelude, with a comment, a statement's params, and its
      // keyword.
      '@media ' + 'm'.repeat(long) + ' { a { b: c } }',
      '@media x /* ' + '{;}'.repeat(third) + ' */ { a { b: c } }',
      '@import "' + 'u'.repeat(long) + '"; a { b: c }',
      '@' + 'k'.repeat(long) + ' x; a { b: c }',
      // Escapes, brackets and parentheses the scans skip.
      '.' + '\\:'.repeat(long / 2) + ' { b: c }',
      'a[x="' + 'q'.repeat(long) + '"] { b: c }',
      'a { b: url(' + 'w'.repeat(long) + ') }',
      // Space and lines between tokens.
      'a {' + ' '.repeat(long) + 'b: c }',
      'a {' + '\n'.repeat(long) + 'b: c }',
      // Characters past ASCII, astral ones among them, before a node on its line.
      '\u{1d11e}'.repeat(long / 2) + '{a:b} c { d: é }',
      // Errors: an unclosed comment, at the end and in a value, and a
      // selector with no block.
      'a { b: c } /* ' + 'u'.repeat(long),
      'a { b: c /* ' + 'u'.repeat(long),
      's'.repeat(long),
    ]
    const failures: string[] = []
    for (const options of [{}, { position: true }]) {
      const tn = make(options) as any
      docs.forEach((doc, n) => {
        const whole = seen(() => tn.parse(doc))
        for (const size of [4096, (64 << 10) + 3]) {
          const read = seen(() => tn.parseReader(pieces(Buffer.from(doc, 'utf8'), size)))
          if (!same(read, whole)) {
            failures.push(JSON.stringify(options) + ' document ' + n + ' in pieces of ' + size +
              ': whole ' + show(whole).slice(0, 300) + ', reader ' + show(read).slice(0, 300))
          }
        }
      })
      console.log('window long tokens: ' + JSON.stringify(options) + ', ' + docs.length + ' documents')
    }
    assert.deepEqual(failures, [])
  })

  test('a-long-document-parses-the-same-from-a-reader', () => {
    // From a reader in pieces of 4 KiB, under the engine's window and
    // under one of 256 units, which moves on hundreds of times; with an
    // error at the end too, which the reader places where the whole text
    // does; and with positions on.
    const parts: string[] = ['/* sheet */\n@charset "utf-8";\n']
    for (let i = 0; i < 2000; i++) {
      parts.push(
        '.c' + i + ', h' + (i % 6) + ' > a[href^="x;' + i + '"]:not(.d, .e) {\n' +
        '  color: #' + (i % 999) + '; /* note ' + i + ' */\n' +
        '  background: url("i' + i + '.png") no-repeat; content: "é\u{1d11e}' + i + '";\n' +
        '}\n',
      )
      if (0 === i % 100) {
        parts.push('@media (min-width: ' + i + 'px) { a { b: ' + i + ' } }\n')
        parts.push('@keyframes k' + i + ' { from { o: 0 } 50%, to { o: 1 } }\n')
      }
    }
    const body = parts.join('')
    const text = body + 'z { last: 1 }\n'
    assert.ok(256 << 10 < text.length, '' + text.length)
    const broken = body + 'z { last: 1 } /* open\n'
    for (const options of [{}, { position: true }]) {
      const tn = make(options) as any
      ;[text, broken].forEach((doc, n) => {
        const whole = seen(() => tn.parse(doc))
        assert.equal(undefined === whole.error, 0 === n, show(whole).slice(0, 200))
        for (const sizes of [undefined, { margin: 256, block: 256, chunk: 4096 }]) {
          const bytes = Buffer.from(doc, 'utf8')
          const read = seen(() =>
            undefined === sizes
              ? tn.parseReader(pieces(bytes, 4096))
              : tn.parseReaderWithSizes(pieces(bytes, 4096), sizes),
          )
          assert.ok(same(read, whole), show(read).slice(0, 300))
        }
      })
      console.log('window long document: ' + JSON.stringify(options) + ', ' +
        text.length + ' characters, whole and broken')
    }
  })
})
