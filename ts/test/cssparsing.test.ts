/* Copyright (c) 2026 Richard Rodger and other contributors, MIT License */

// Conformance against the THIRD-PARTY CSS Syntax Level 3 corpus
// (SimonSapin/css-parsing-tests -- the implementation-independent suite used
// by rust-cssparser/Servo, tinycss2 and Crass).
//
// The corpus is NOT vendored. `scripts/fetch-css-parsing-tests.sh` clones it
// at a pinned commit into `test/css-parsing-tests/`, which is gitignored. The
// `pretest` npm script runs it, so a normal `npm test` always has it. If it is
// absent this file FAILS LOUDLY -- it never skips.
//
// `go/cssparsing_test.go` runs the SAME files with the SAME derivation.
//
// ---------------------------------------------------------------------------
// WHAT IS COMPARED, AND WHY
// ---------------------------------------------------------------------------
// Upstream's expected values are component-value trees: a token-level model
// (idents, delims, whitespace, nested {} [] () blocks) that this plugin does
// not produce and does not claim to. It produces a reworkcss-style rule AST.
// So the runner compares a *kind sequence* computed MECHANICALLY from
// upstream's own expected output:
//
//   ["at-rule", name, ...]      -> "@" + name
//   ["qualified rule", ...]     -> "rule"
//   ["declaration", name, ...]  -> "decl:" + name
//   ["error", ...]              -> "error"
//
// and the same sequence read off this plugin's AST (comment nodes dropped --
// CSS Syntax discards comments, so upstream's model has no item to match).
//
// If upstream's expected sequence contains an "error" item, the input
// contains a construct that CSS Syntax reports as a parse error. This plugin
// has no error-recovery mode (unlike reworkcss's `silent` option), so the only
// conformant response available to it is to REJECT the whole input. Those
// cases are therefore asserted as must-reject.
//
// FILES DELIBERATELY NOT WIRED, and why -- these are NOT a skip list, they
// test algorithms this plugin has no entry point for, so running them would
// manufacture failures that say nothing about CSS conformance:
//   one_rule.json / one_declaration.json  "parse a rule"/"parse a declaration"
//       parse exactly ONE item and report "extra-input" for anything after it;
//       a stylesheet parser legitimately accepts several. Their `;` semantics
//       also differ from declaration-list semantics.
//   component_value_list.json / one_component_value.json
//       token-level, no rule/declaration structure to compare.
//   stylesheet_bytes.json   byte-stream encoding detection; this plugin takes
//       a decoded string.
//   color_*.json / An+B.json
//       value-level micro-syntaxes; this plugin keeps declaration values as
//       raw text by design (documented in README.md).
//
// There is no SKIP list among the wired files: every case in every wired file
// runs and is asserted.

import { describe, test } from 'node:test'
import assert from 'node:assert'
import { existsSync, readFileSync } from 'node:fs'
import { join } from 'node:path'

import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '../dist/css'

// Pinned upstream. Keep in step with scripts/fetch-css-parsing-tests.sh.
const UPSTREAM = 'https://github.com/SimonSapin/css-parsing-tests'
const SHA = '203ce36bffd617db7f118c551e32794561fb273d'

const suiteDir = join(__dirname, '..', '..', 'test', 'css-parsing-tests')

function requireCorpus(): void {
  if (existsSync(join(suiteDir, 'stylesheet.json'))) return
  throw new Error(
    'MISSING CONFORMANCE CORPUS: ' +
      suiteDir +
      '\nThe css-parsing-tests corpus is third-party and is never committed.' +
      '\nFetch it (pinned to ' +
      SHA +
      ') with:\n\n    bash scripts/fetch-css-parsing-tests.sh\n\n' +
      'This test does NOT skip when the corpus is absent: a conformance ' +
      'suite that quietly does not run is worse than no suite at all.',
  )
}

function parseCss(src: string): any {
  return new Tabnas().use(jsonic).use(Css).parse(src)
}

// Each upstream file is a flat array of [input, expected, input, expected...].
function loadPairs(file: string): { input: string; expected: any[] }[] {
  const flat = JSON.parse(readFileSync(join(suiteDir, file), 'utf8'))
  const out: { input: string; expected: any[] }[] = []
  for (let i = 0; i + 1 < flat.length; i += 2) {
    out.push({ input: flat[i], expected: flat[i + 1] })
  }
  return out
}

function upstreamKind(item: any): string {
  if (!Array.isArray(item)) return 'other:' + JSON.stringify(item)
  switch (item[0]) {
    case 'at-rule':
      return '@' + item[1]
    case 'qualified rule':
      return 'rule'
    case 'declaration':
      return 'decl:' + item[1]
    case 'error':
      return 'error'
    default:
      return 'other:' + JSON.stringify(item[0])
  }
}

function nodeKind(node: any): string | null {
  const t = node && node.type
  if ('comment' === t) return null
  if ('rule' === t) return 'rule'
  if ('declaration' === t) return 'decl:' + node.property
  // Every other node type in this AST is an at-rule; `vendor` is split off
  // the keyword by the grammar, so put it back to recover the at-keyword.
  return '@' + (node.vendor ?? '') + t
}

function nodeKinds(nodes: any[]): string[] {
  return (nodes ?? []).map(nodeKind).filter((k): k is string => null !== k)
}

function label(s: string): string {
  const one = JSON.stringify(s)
  return 60 < one.length ? one.slice(0, 57) + '..."' : one
}

// `wrap` turns a declaration-list/block-contents input into a whole
// stylesheet; `pick` pulls the matching node array back out of the result.
function runFile(
  file: string,
  count: number,
  wrap: (s: string) => string,
  pick: (ast: any) => any[],
) {
  requireCorpus()
  const pairs = loadPairs(file)

  describe(file, () => {
    test('corpus census', () => {
      assert.equal(
        pairs.length,
        count,
        `expected ${count} cases in ${file} at ${UPSTREAM}@${SHA}; the ` +
          'pinned corpus changed. Do not silently accept a different suite.',
      )
    })

    for (const p of pairs) {
      const wantKinds = p.expected.map(upstreamKind)
      const mustReject = wantKinds.includes('error')

      test(`${mustReject ? '[reject]' : '[accept]'} ${label(p.input)}`, () => {
        const src = wrap(p.input)

        if (mustReject) {
          assert.throws(
            () => parseCss(src),
            'CSS Syntax L3 reports a parse error for this input ' +
              `(upstream expects ${JSON.stringify(wantKinds)}); this plugin ` +
              'has no error-recovery mode, so it must reject.',
          )
          return
        }

        const raw = parseCss(src)
        // An empty stylesheet parses to undefined by design (documented in
        // AGENTS.md), and upstream expects [] for it -- that is a match.
        const got = undefined === raw ? { rules: [] } : JSON.parse(JSON.stringify(raw))
        assert.deepStrictEqual(nodeKinds(pick(got)), wantKinds, label(p.input))
      })
    }
  })
}

const identity = (s: string) => s
const topRules = (ast: any) => ast.rules ?? []
// A declaration list / block contents has no standalone entry point here, so
// wrap it in a style rule and read that rule's `declarations` back out. CSS
// Nesting means nested rules and at-rules also land in `declarations`, which
// is exactly what blocks_contents.json expects.
const wrapBlock = (s: string) => 'a{\n' + s + '\n}'
const blockDecls = (ast: any) => ast.rules?.[0]?.declarations ?? []

describe(`css-parsing-tests (CSS Syntax L3, ${SHA.slice(0, 7)})`, () => {
  test('corpus is present', () => requireCorpus())

  runFile('stylesheet.json', 16, identity, topRules)
  runFile('rule_list.json', 15, identity, topRules)
  runFile('declaration_list.json', 10, wrapBlock, blockDecls)
  runFile('blocks_contents.json', 13, wrapBlock, blockDecls)
})
