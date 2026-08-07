/* Copyright (c) 2026 Richard Rodger and other contributors, MIT License */

// Leniency-leak guard.
//
// The documented setup for this plugin layers it on the jsonic engine:
//
//     new Tabnas().use(jsonic).use(Css)
//
// jsonic is a deliberately RELAXED JSON dialect. Across the tabnas grammar
// plugins the single largest source of over-acceptance has been jsonic's base
// leniency leaking through a plugin that only *adds* rules -- @tabnas/json5,
// for example, rejects '{a:1' with the plugin alone but ACCEPTS it through
// the documented stack.
//
// @tabnas/css is not supposed to be susceptible: AGENTS.md says it "turns off
// the relaxed-JSON value matchers" and supplies fixed CSS tokenisation, and
// it applies those option overrides atomically with its rule alts, so the
// plugin behaves the same with or without `use(jsonic)`.
//
// This file PINS that property. It asserts an EQUIVALENCE -- the documented
// stack and the plugin alone must classify every probe identically -- rather
// than pinning particular verdicts, so it stays honest even where the current
// verdict is wrong. If a future change lets a jsonic construct through only
// on the documented stack, this goes red.

import { describe, test } from 'node:test'
import assert from 'node:assert'

import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '../dist/css'

const stack = () => new Tabnas().use(jsonic).use(Css)
const alone = () => new Tabnas().use(Css)

type Verdict = { ok: boolean; value?: string }

function classify(make: () => any, src: string): Verdict {
  try {
    return { ok: true, value: JSON.stringify(make().parse(src) ?? null) }
  } catch {
    return { ok: false }
  }
}

// Probes: jsonic-flavoured constructs that are NOT CSS, plus a handful of
// real CSS for contrast. Any of these accepted only through the stack would
// be a leak.
const PROBES = [
  // --- bare jsonic / relaxed-JSON documents (not CSS at all) -------------
  '{a:1}',
  '{a:1',
  '[1,2,3]',
  '[1,2,3',
  'a:1',
  'a:1,b:2',
  'true',
  'null',
  '42',
  '"str"',
  "'str'",
  '`str`',
  'a,b,c',
  '{"a":1}',
  'a:{b:1}',
  '{}',
  '[]',
  // --- jsonic comment styles that CSS does not have ----------------------
  'a{c:1} # hash comment',
  'a{c:1} // line comment',
  'a{c:1} /* css comment */',
  // --- truncation / trailing junk ----------------------------------------
  'a{c:1',
  'a{c:1}}',
  '}',
  'a{c:1} extra',
  'a{c:1};;',
  '@media screen{',
  // --- real CSS (must behave identically both ways) ----------------------
  'a{c:1}',
  '#a{b:c}',
  'a{c:1;}',
  'a{--x:1}',
  '@media screen{a{b:c}}',
  '@import "x";',
  '/*x*/',
  'a{c:1; d e{f:g}}',
]

describe('leniency: plugin alone vs documented jsonic stack', () => {
  for (const src of PROBES) {
    test(`same verdict: ${JSON.stringify(src)}`, () => {
      const s = classify(stack, src)
      const a = classify(alone, src)
      assert.equal(
        s.ok,
        a.ok,
        `LENIENCY LEAK: new Tabnas().use(jsonic).use(Css) ${s.ok ? 'ACCEPTS' : 'rejects'}` +
          ` ${JSON.stringify(src)} but new Tabnas().use(Css) ${a.ok ? 'ACCEPTS' : 'rejects'} it.` +
          ' The plugin must not inherit jsonic base leniency.',
      )
      if (s.ok) {
        assert.equal(
          s.value,
          a.value,
          `VALUE DIVERGENCE for ${JSON.stringify(src)} between the documented` +
            ' stack and the plugin alone.',
        )
      }
    })
  }

  // The probe list must actually contain jsonic documents, otherwise this
  // file could pass by testing nothing interesting.
  test('probe set covers relaxed-JSON documents', () => {
    assert.ok(PROBES.includes('{a:1}'), 'probe set lost its jsonic documents')
    assert.ok(30 <= PROBES.length, `only ${PROBES.length} probes`)
  })
})
