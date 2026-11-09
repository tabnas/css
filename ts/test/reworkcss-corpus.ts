/* Copyright (c) 2025 Richard Rodger and other contributors, MIT License */

// The reworkcss/css conformance corpus, as the tests read it: where it is,
// its cases' inputs, and the accept/reject inputs of its `test/parse.js`.
// `reworkcss.test.ts` holds the parse to the corpus, and `window.test.ts`
// holds a parse from a reader to the parse of the same text; both read it
// through here, so the two cannot read it differently. See
// `reworkcss.test.ts` for what the corpus is and how it is fetched.

import { existsSync, readFileSync } from 'node:fs'
import { join } from 'node:path'

// At runtime this file is loaded from `dist-test/`, so hop up to the repo root.
const corpusDir = join(__dirname, '..', '..', 'test', 'reworkcss-css')
const casesDir = join(corpusDir, 'test', 'cases')

// Whether the corpus has been fetched.
const present = existsSync(join(corpusDir, 'test', 'parse.js'))

// Byte-identical to upstream test/cases.js `readFile`, including its
// single-occurrence CRLF replace, so the parser sees exactly the source that
// produced the expected ASTs (line/column numbers depend on it).
function readCase(file: string): string {
  let src = readFileSync(file, 'utf8')
  src = src.replace(/\r\n/, '\n')
  src = src.replace(/\n$/, '')
  return src
}

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

type ErrorCase = { kind: string; src: string; optioned: boolean }

// The accept/reject oracle, extracted mechanically from upstream's own
// test/parse.js so it cannot drift from what reworkcss actually asserts.
function extractErrorCases(): ErrorCase[] {
  const body = readFileSync(join(corpusDir, 'test', 'parse.js'), 'utf8')
  const out: ErrorCase[] = []
  for (const m of body.matchAll(ASSERT_RE)) {
    out.push({
      kind: m[1],
      src: decodeJsString(m[2]),
      optioned: undefined !== m[3],
    })
  }
  return out
}

export { corpusDir, casesDir, present, readCase, extractErrorCases }
export type { ErrorCase }
