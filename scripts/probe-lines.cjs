// The TypeScript half of scripts/divergence-probe-rs.sh: the same contract as
// rs/examples/parse.rs with --lines, so the probe compares like with like.
//
// Each line of stdin is one CSS document with `\n` `\r` `\t` `\\` decoded —
// the escape codec the shared test/spec fixtures use — and one result is
// printed per line: the AST as JSON, or ERROR:<code>.
//
// Usage: node scripts/probe-lines.cjs [--position] [--lowercase-properties]
'use strict'

const path = require('path')
const fs = require('fs')

const TS = path.join(__dirname, '..', 'ts')
const { Tabnas } = require(path.join(TS, 'node_modules', '@tabnas', 'parser'))
const { jsonic } = require(path.join(TS, 'node_modules', '@tabnas', 'jsonic'))
const { Css } = require(path.join(TS, 'dist', 'css.js'))

const opts = {}
for (const arg of process.argv.slice(2)) {
  if ('--position' === arg) opts.position = true
  else if ('--lowercase-properties' === arg) opts.lowercaseProperties = true
  else {
    console.error('probe-lines: unknown argument ' + arg)
    process.exit(2)
  }
}

// The engine builds null-prototype objects; JSON.stringify handles them, but
// a plain copy keeps the output comparable field for field.
function plain(v) {
  if (null === v || 'object' !== typeof v) return v
  if (Array.isArray(v)) return v.map(plain)
  const out = {}
  for (const k of Object.keys(v)) out[k] = plain(v[k])
  return out
}

function decode(s) {
  let out = ''
  for (let i = 0; i < s.length; i++) {
    if ('\\' === s[i] && i + 1 < s.length) {
      const c = s[i + 1]
      if ('n' === c) { out += '\n'; i++; continue }
      if ('r' === c) { out += '\r'; i++; continue }
      if ('t' === c) { out += '\t'; i++; continue }
      if ('\\' === c) { out += '\\'; i++; continue }
    }
    out += s[i]
  }
  return out
}

const tn = new Tabnas().use(jsonic).use(Css, opts)
function report(src) {
  try {
    return JSON.stringify(plain(tn.parse(src)))
  } catch (err) {
    return 'ERROR:' + (err && err.code)
  }
}

const rows = fs.readFileSync(0, 'utf8').split('\n')
if ('' === rows[rows.length - 1]) rows.pop()
process.stdout.write(rows.map((row) => report(decode(row))).join('\n') + '\n')
