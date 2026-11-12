/* Copyright (c) 2026 Richard Rodger and other contributors, MIT License */

// A parse whose tree is emptied behind it (tabnas/css#79).
//
// A host that streams css through its transducer part (tabnas/transduce's
// ParserEventSource, pruning) empties the tree behind the stream: once it has
// seen a close pass of a rule whose node is an array, the array is emptied,
// and, for a grammar whose maps it empties too, a map's members are deleted.
// css appends a block's rules, declarations and keyframes in loops that close
// once per item on the block's node, so after the first item the list it
// appends to may be gone: each append makes the list again, as the Go port's
// appendField does, and the parse goes on. These tests empty the tree as
// transduce does, over every document of the reworkcss corpus and a long
// stylesheet, and hold each parse to end as it does with nothing emptied:
// accepted, or refused with the same code.

import { test, describe } from 'node:test'
import assert from 'node:assert'
import { readdirSync } from 'node:fs'
import { join } from 'node:path'

import { make } from '../dist/css'

import { casesDir, present, readCase } from './reworkcss-corpus'

// A plain map of the tree, as the grammar builds them.
function isMap(value: unknown): value is Record<string, unknown> {
  if (null === value || 'object' !== typeof value || Array.isArray(value)) return false
  const proto = Object.getPrototypeOf(value)
  return null === proto || Object.prototype === proto
}

// A parser whose tree is emptied as transduce's pruning source empties it,
// maps included: at every close pass, the closed rule's node.
function emptying(): any {
  const tn: any = make()
  tn.sub({
    ruleDone: (rule: any, _ctx: any, done: any) => {
      if ('c' !== done.state || true === done.probe) return
      const node = rule.node
      if (Array.isArray(node)) node.length = 0
      else if (isMap(node)) for (const key of Object.keys(node)) delete node[key]
    },
  })
  return tn
}

// How a parse ends: accepted, or refused with its code.
function ending(parse: () => unknown): string {
  try {
    parse()
    return 'accepted'
  } catch (error: any) {
    return 'refused ' + (error?.code ?? String(error?.message ?? error))
  }
}

describe('a parse whose tree is emptied behind it', () => {
  test('a long stylesheet parses with its maps emptied at every close', () => {
    const items: string[] = []
    for (let i = 0; i < 200; i++) {
      items.push(`.c${i} { color: red; margin: 0 ${i}px; padding: 1px }`)
      if (0 === i % 50) {
        items.push(`@media screen { .m${i} { color: blue } .n${i} { color: green } }`)
        items.push(`@keyframes k${i} { from { opacity: 0 } 50%, 75% { opacity: 0.5 } to { opacity: 1 } }`)
      }
    }
    const text = items.join('\n')
    assert.strictEqual(ending(() => make().parse(text)), 'accepted')
    assert.strictEqual(ending(() => emptying().parse(text)), 'accepted')
  })

  test('every reworkcss document ends as it does with nothing emptied', { skip: !present }, () => {
    const names = readdirSync(casesDir).sort()
    assert.ok(0 < names.length)
    let accepted = 0
    for (const name of names) {
      const input = readCase(join(casesDir, name, 'input.css'))
      const plain = ending(() => make().parse(input))
      assert.strictEqual(ending(() => emptying().parse(input)), plain, name)
      if ('accepted' === plain) accepted++
    }
    assert.ok(0 < accepted)
  })
})
