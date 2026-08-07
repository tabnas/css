/* Copyright (c) 2026 Richard Rodger and other contributors, MIT License */

// Conformance against the THIRD-PARTY reworkcss/css corpus.
//
// README.md and AGENTS.md both state that this plugin's output "AST shape
// follows the widely-used reworkcss/css model", so reworkcss's own
// `test/cases/*/ast.json` files are the authoritative expected VALUES for
// this plugin -- not merely accept/reject oracles. This runner therefore
// compares the WHOLE tree, not "it didn't throw".
//
// The corpus is NOT vendored. `scripts/fetch-reworkcss-tests.sh` clones
// reworkcss/css at a pinned commit into `test/reworkcss-css/`, which is
// gitignored. The `pretest` npm script runs it, so the corpus is always
// present in a normal `npm test`. If it is absent this file FAILS LOUDLY --
// it never skips. A conformance test that quietly does not run is worse than
// no test at all, because the green tick is a lie.
//
// `go/reworkcss_test.go` runs the SAME corpus with the SAME derivation, so
// the two runtimes cannot drift without one of them going red.
//
// There is no SKIP list.

import { describe, test } from 'node:test'
import assert from 'node:assert'
import { existsSync, readFileSync, readdirSync } from 'node:fs'
import { join } from 'node:path'

import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '../dist/css'

// Pinned upstream. Keep in step with scripts/fetch-reworkcss-tests.sh.
const UPSTREAM = 'https://github.com/reworkcss/css'
const SHA = 'ae6a6f9bf939cbcbc759a12d9f208afb5d4dde75'

// Census of the pinned commit. A corpus that has silently shrunk must not
// quietly improve the conformance number, so these are hard assertions.
const CASE_COUNT = 46
const THROW_COUNT = 4
const NOTHROW_COUNT = 2
const OPTIONED_COUNT = 1 // `parse(src, {silent:true})` -- no tabnas equivalent

// At runtime this file is loaded from `dist-test/`, so hop up to the repo root.
const corpusDir = join(__dirname, '..', '..', 'test', 'reworkcss-css')
const casesDir = join(corpusDir, 'test', 'cases')

function requireCorpus(): void {
  if (existsSync(join(corpusDir, 'test', 'parse.js'))) return
  throw new Error(
    'MISSING CONFORMANCE CORPUS: ' +
      corpusDir +
      '\nThe reworkcss/css corpus is third-party and is never committed.' +
      '\nFetch it (pinned to ' +
      SHA +
      ') with:\n\n    bash scripts/fetch-reworkcss-tests.sh\n\n' +
      'This test does NOT skip when the corpus is absent: a conformance ' +
      'suite that quietly does not run is worse than no suite at all.',
  )
}

function parseCss(src: string, opts: object = {}): any {
  return new Tabnas().use(jsonic).use(Css, opts).parse(src)
}

// Byte-identical to upstream test/cases.js `readFile`, including its
// single-occurrence CRLF replace, so the parser sees exactly the source that
// produced the expected ASTs (line/column numbers depend on it).
function readCase(file: string): string {
  let src = readFileSync(file, 'utf8')
  src = src.replace(/\r\n/, '\n')
  src = src.replace(/\n$/, '')
  return src
}

// Upstream wraps the tree as {type:'stylesheet', stylesheet:{rules, ...}};
// this plugin emits {type:'stylesheet', rules}. Unwrap to compare.
function upstreamRules(ast: any): any[] {
  assert.ok(ast && ast.stylesheet, 'upstream ast.json has no .stylesheet')
  return ast.stylesheet.rules
}

// Strip a key recursively (used for `position`, and for `source` inside a
// position -- this plugin has no notion of a source filename).
function without(value: any, keys: string[]): any {
  if (Array.isArray(value)) return value.map((v) => without(v, keys))
  if (value && 'object' === typeof value) {
    const out: any = {}
    for (const k of Object.keys(value)) {
      if (keys.includes(k)) continue
      out[k] = without(value[k], keys)
    }
    return out
  }
  return value
}

const json = (v: any) => JSON.parse(JSON.stringify(v ?? null))

// ---------------------------------------------------------------------------
// Half 1: valid documents must parse AND produce the correct VALUE.
// ---------------------------------------------------------------------------

describe(`reworkcss/css cases (${SHA.slice(0, 7)})`, () => {
  test('corpus is present', () => requireCorpus())

  requireCorpus()
  const names = readdirSync(casesDir).sort()

  test('corpus census', () => {
    assert.equal(
      names.length,
      CASE_COUNT,
      `expected ${CASE_COUNT} cases at ${UPSTREAM}@${SHA}; the pinned corpus ` +
        'changed. Do not silently accept a different suite.',
    )
  })

  for (const name of names) {
    const input = readCase(join(casesDir, name, 'input.css'))
    const expected = JSON.parse(readFileSync(join(casesDir, name, 'ast.json'), 'utf8'))

    // Primary metric: the structural AST, positions removed (the plugin's
    // `position` option is off by default).
    test(`cases/${name}: AST`, () => {
      const got = json(parseCss(input))
      assert.notStrictEqual(got, null, `cases/${name}: parse produced no value`)
      assert.deepStrictEqual(
        without(got.rules, ['position']),
        without(upstreamRules(expected), ['position']),
        `cases/${name}`,
      )
    })

    // Secondary metric: the same tree WITH source positions, which upstream
    // records for every node. `source` is upstream-only (a filename).
    test(`cases/${name}: AST + position`, () => {
      const got = json(parseCss(input, { position: true }))
      assert.notStrictEqual(got, null, `cases/${name}: parse produced no value`)
      assert.deepStrictEqual(
        got.rules,
        without(upstreamRules(expected), ['source']),
        `cases/${name} (position)`,
      )
    })
  }
})

// ---------------------------------------------------------------------------
// Half 2: the must-fail / must-accept inputs. Upstream keeps these in code
// (test/parse.js), not in a data file, so they are extracted mechanically
// from the fetched source and the extraction count is pinned -- a change in
// upstream's file shape fails loudly instead of yielding an empty corpus.
// ---------------------------------------------------------------------------

type ErrCase = { kind: 'throws' | 'doesNotThrow'; src: string; optioned: boolean }

// Decode a JavaScript single-quoted string literal.
function decodeJsString(lit: string): string {
  let out = ''
  for (let i = 0; i < lit.length; i++) {
    const c = lit[i]
    if ('\\' !== c) {
      out += c
      continue
    }
    const n = lit[++i]
    if ('n' === n) out += '\n'
    else if ('r' === n) out += '\r'
    else if ('t' === n) out += '\t'
    else if ('f' === n) out += '\f'
    else if ('b' === n) out += '\b'
    else if ('v' === n) out += '\v'
    else if ('0' === n) out += '\0'
    else if ('u' === n) {
      out += String.fromCodePoint(parseInt(lit.slice(i + 1, i + 5), 16))
      i += 4
    } else if ('x' === n) {
      out += String.fromCodePoint(parseInt(lit.slice(i + 1, i + 3), 16))
      i += 2
    } else out += n
  }
  return out
}

const ASSERT_RE =
  /assert\.(throws|doesNotThrow)\(\s*function\s*\(\)\s*\{\s*parse\('((?:[^'\\]|\\.)*)'\s*(,[^)]*)?\)/g

function extractErrorCases(): ErrCase[] {
  const body = readFileSync(join(corpusDir, 'test', 'parse.js'), 'utf8')
  const out: ErrCase[] = []
  for (const m of body.matchAll(ASSERT_RE)) {
    out.push({
      kind: m[1] as ErrCase['kind'],
      src: decodeJsString(m[2]),
      optioned: undefined !== m[3],
    })
  }
  return out
}

function label(s: string): string {
  const one = JSON.stringify(s)
  return 60 < one.length ? one.slice(0, 57) + '..."' : one
}

describe(`reworkcss/css must-fail inputs (${SHA.slice(0, 7)})`, () => {
  requireCorpus()
  const cases = extractErrorCases()

  test('extraction census', () => {
    const t = cases.filter((c) => 'throws' === c.kind && !c.optioned).length
    const n = cases.filter((c) => 'doesNotThrow' === c.kind && !c.optioned).length
    const o = cases.filter((c) => c.optioned).length
    assert.deepStrictEqual(
      { throws: t, doesNotThrow: n, optioned: o },
      { throws: THROW_COUNT, doesNotThrow: NOTHROW_COUNT, optioned: OPTIONED_COUNT },
      'upstream test/parse.js changed shape; the mechanical extraction is ' +
        'no longer reading the corpus it was written for. Fix the extractor ' +
        'rather than accepting a smaller must-fail set.',
    )
  })

  for (const c of cases) {
    // `parse(src, {silent:true})` asserts reworkcss's error-RECOVERY mode,
    // which this plugin does not implement and does not claim to. Not a skip
    // of a case this plugin is judged on: it is a case about a different API.
    if (c.optioned) {
      test(`[n/a: reworkcss silent option] ${label(c.src)}`, () => {
        assert.equal(c.kind, 'doesNotThrow')
      })
      continue
    }

    if ('throws' === c.kind) {
      test(`[reject] ${label(c.src)}`, () => {
        assert.throws(
          () => parseCss(c.src),
          `upstream reworkcss/css rejects this input; it must not parse`,
        )
      })
    } else {
      test(`[accept] ${label(c.src)}`, () => {
        const got = parseCss(c.src)
        assert.notStrictEqual(got, undefined, 'parse produced no value')
      })
    }
  }
})
