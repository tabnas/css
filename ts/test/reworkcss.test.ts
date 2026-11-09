/* Copyright (c) 2025 Richard Rodger and other contributors, MIT License */

// LANGUAGE CONFORMANCE against the third-party reworkcss/css corpus.
//
// README.md and AGENTS.md both state that this plugin's output "AST shape
// follows the widely-used reworkcss/css model", so reworkcss's own
// `test/cases/*/ast.json` files are the authoritative expected VALUES for the
// parse — not merely accept/reject oracles. This runner therefore compares the
// WHOLE tree, both without and with source positions.
//
// The corpus is third-party and is NOT vendored. Fetch it (pinned) with:
//
//     npm run install-reworkcss-tests     # or scripts/fetch-reworkcss-tests.sh
//
// The `pretest` npm script runs that fetch before every `npm test`, so the
// corpus is normally present. If it is still absent the suite FAILS — it never
// skips, because a conformance suite that quietly does not run reports green
// while measuring nothing.
//
// `go/reworkcss_test.go` and `rs/tests/reworkcss.rs` run the SAME corpus with
// the SAME derivation, so the three runtimes cannot drift without one of
// them going red.
//
// There is no per-case skip list. The one case this plugin does not match,
// `empty`, is asserted explicitly below as a documented divergence.

import { describe, test } from 'node:test'
import assert from 'node:assert'
import { readFileSync, readdirSync } from 'node:fs'
import { join } from 'node:path'

import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '../dist/css'

import { casesDir, present, readCase, extractErrorCases } from './reworkcss-corpus'

// Pinned upstream. Keep in step with scripts/fetch-reworkcss-tests.sh and
// go/reworkcss_test.go.
const UPSTREAM = 'https://github.com/reworkcss/css'
const SHA = 'ae6a6f9bf939cbcbc759a12d9f208afb5d4dde75'

// Census of the pinned commit. A corpus that silently shrank must not quietly
// improve the conformance number, so these are hard assertions.
const CASE_COUNT = 46
const THROW_COUNT = 4
const NOTHROW_COUNT = 2
const OPTIONED_COUNT = 1 // `parse(src, {silent:true})` — no tabnas equivalent

// ABSENT is a FAILURE message, not a skip message. `pretest` fetches the
// corpus before every run, so reaching this means the fetch itself failed.
const ABSENT =
  'MISSING CONFORMANCE CORPUS: reworkcss/css is not installed, so the CSS' +
  ' conformance claim is UNVERIFIED. Fetch it (pinned) with `npm run' +
  ' install-reworkcss-tests`. This test does NOT skip when the corpus is' +
  ' absent.'

// Registered first in each conformance suite below. It is the guard that
// makes an unfetched corpus loud: without it the suites would simply contain
// no cases and the run would be green while measuring nothing.
function corpusPresent(): void {
  assert.ok(present, ABSENT)
}

function parseCss(src: string, opts: any = {}): any {
  return new Tabnas().use(jsonic).use(Css, opts).parse(src)
}

// Upstream wraps the tree as {type:'stylesheet', stylesheet:{rules, ...}};
// this plugin emits {type:'stylesheet', rules}. Unwrap to compare.
function upstreamRules(ast: any): any[] {
  assert.ok(ast && ast.stylesheet, 'upstream ast.json has no .stylesheet')
  return ast.stylesheet.rules
}

// Strip a key recursively (used for `position`, and for `source` inside a
// position — this plugin has no notion of a source filename).
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
// Documented divergences: cases where this plugin knowingly differs from
// upstream. Each is asserted explicitly (never skipped), so the divergence
// cannot widen unnoticed and cannot quietly disappear either.
// ---------------------------------------------------------------------------
const DIVERGENT: Record<string, (t: any) => void> = {
  // NO LONGER DIVERGENT — kept here so the case stays explicitly asserted.
  // A zero-length source used to yield undefined. The cause was never the
  // rule iteration budget (the engine returns cfg.lex.emptyResult for ''
  // before the rule loop is reachable); it was that css declared no
  // emptyResult. It now declares one in src/css.ts, so '' matches upstream.
  empty: () => {
    assert.deepStrictEqual(json(parseCss('')), { type: 'stylesheet', rules: [] })
    assert.deepStrictEqual(json(parseCss(' ')), { type: 'stylesheet', rules: [] })
  },
}

// ---------------------------------------------------------------------------
// Half 1: valid documents must parse AND produce the correct VALUE.
// ---------------------------------------------------------------------------
describe(`reworkcss/css cases (${SHA.slice(0, 7)})`, () => {
  const names = present ? readdirSync(casesDir).sort() : []

  test('corpus present', corpusPresent)

  test('corpus census', () => {
    if (!present) return corpusPresent()
    assert.equal(
      names.length,
      CASE_COUNT,
      `expected ${CASE_COUNT} cases at ${UPSTREAM}@${SHA}; the pinned corpus ` +
        'changed. Do not silently accept a different suite.',
    )
  })

  for (const name of names) {
    const divergent = DIVERGENT[name]
    if (divergent) {
      test(`cases/${name}: documented divergence`, divergent)
      continue
    }

    const input = readCase(join(casesDir, name, 'input.css'))
    const expected = JSON.parse(
      readFileSync(join(casesDir, name, 'ast.json'), 'utf8'),
    )

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
// Half 2: the accept/reject oracle, extracted mechanically from upstream's
// own test/parse.js so it cannot drift from what reworkcss actually asserts.
// ---------------------------------------------------------------------------

function label(s: string): string {
  const one = JSON.stringify(s)
  return 60 < one.length ? one.slice(0, 57) + '..."' : one
}

describe(
  `reworkcss/css accept-reject oracle (${SHA.slice(0, 7)})`,
  () => {
    const cases = present ? extractErrorCases() : []

    test('corpus present', corpusPresent)

    test('extraction census', () => {
      if (!present) return corpusPresent()
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
            'upstream reworkcss/css rejects this input; it must not parse',
          )
        })
      } else {
        test(`[accept] ${label(c.src)}`, () => {
          const got = parseCss(c.src)
          assert.notStrictEqual(got, undefined, 'parse produced no value')
        })
      }
    }
  },
)
