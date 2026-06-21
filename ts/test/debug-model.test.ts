/* Copyright (c) 2025 Richard Rodger and other contributors, MIT License */

// Composition test: the CSS grammar plugin layered with the official
// @tabnas/debug plugin. @tabnas/debug is a devDependency, but this still
// resolves it dynamically and SKIPS when it is absent so the suite stays
// runnable outside the package; TABNAS_DEBUG_PATH can point at a sibling
// checkout's built plugin.

import { describe, test } from 'node:test'
import assert from 'node:assert'

import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Css } from '../dist/css'

function loadDebug(): any {
  const candidates = [process.env.TABNAS_DEBUG_PATH, '@tabnas/debug'].filter(
    Boolean,
  ) as string[]
  for (const c of candidates) {
    try {
      return require(c).Debug
    } catch {
      /* try next */
    }
  }
  return null
}

const Debug = loadDebug()
const skip = Debug
  ? false
  : '@tabnas/debug not available (set TABNAS_DEBUG_PATH)'

function build(): any {
  const tn = new Tabnas().use(jsonic).use(Css, {})
  tn.use(Debug, { print: false, trace: false })
  return tn
}

describe('compose: css + @tabnas/debug', () => {
  test('parses normally with the debug plugin installed', { skip }, () => {
    const tn = build()
    assert.deepStrictEqual(
      JSON.parse(JSON.stringify(tn.parse('a { x: 1 } /* c */'))),
      {
        type: 'stylesheet',
        rules: [
          {
            type: 'rule',
            selectors: ['a'],
            declarations: [{ type: 'declaration', property: 'x', value: '1' }],
          },
          { type: 'comment', comment: ' c ' },
        ],
      },
    )
  })

  test('debug.model() returns the structured css grammar', { skip }, () => {
    const tn = build()
    const m = tn.debug.model()

    // The AST-building rules are present, and the entry rule is the
    // implicit top-level stylesheet.
    const ruleNames = m.rules.map((r: any) => r.name)
    for (const name of [
      'stylesheet',
      'items',
      'statement',
      'sel',
      'declbody',
      'decls',
      'decl',
    ]) {
      assert.ok(ruleNames.includes(name), `rules should include ${name}`)
    }
    assert.equal(m.config.start, 'stylesheet')
    assert.ok(
      m.plugins.some((p: any) => p.name === 'Css'),
      'plugins should list Css',
    )

    // The rule-reference graph captures the recursive AST structure: the
    // stylesheet pushes an items list; each items pushes a statement and
    // close-replaces itself to iterate; a statement opens one of the bodies
    // (rules / declarations / keyframes) or a selector list; declarations
    // iterate via decls -> decl.
    const edge = (name: string) => m.graph.find((e: any) => e.name === name)
    assert.deepStrictEqual(edge('stylesheet').openPush, ['items'])
    assert.deepStrictEqual(edge('items').openPush, ['statement'])
    assert.deepStrictEqual(edge('items').closeReplace, ['items'])
    assert.ok(
      edge('statement').openPush.includes('sel'),
      'statement should push sel (a style rule)',
    )
    assert.deepStrictEqual(edge('decls').closeReplace, ['decls'])

    // The grammar portion is JSON-serialisable and round-trips.
    const grammar = {
      tokens: m.tokens,
      rules: m.rules,
      graph: m.graph,
      config: m.config,
      abnf: m.abnf,
    }
    assert.deepStrictEqual(JSON.parse(JSON.stringify(grammar)).rules, m.rules)
  })
})
