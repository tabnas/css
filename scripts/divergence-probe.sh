#!/usr/bin/env bash
# TS/Go differential probe.
#
# Generates a DETERMINISTIC pseudo-random corpus of CSS-ish inputs from a
# token alphabet, parses every one with both runtimes, canonicalises the two
# results (Go marshals map keys in sorted order and HTML-escapes < > &) and
# reports every input the two runtimes classify or value differently.
#
# Nothing here is a third-party corpus: the inputs are generated from the seed
# below, so the run is reproducible without vendoring anything.
#
# The divergences this found are pinned as fixtures in
# test/spec/divergence.tsv, which BOTH runtimes run unconditionally.
#
# Usage:  bash scripts/divergence-probe.sh [count]     (default 4000)
#
# Requires a built ts/dist (npm run build) and a working go toolchain.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
COUNT="${1:-4000}"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

node -e '
const fs = require("fs")
let s = 123456789
const rnd = () => { s = (s * 1103515245 + 12345) & 0x7fffffff; return s / 0x7fffffff }
const toks = [
  "a","b",".c","#d","*","{","}",":",";",","," ","\n","\t",
  "/*","*/","/*x*/","@media","@import","@keyframes","@font-face","@page",
  "@supports","@-webkit-keyframes","screen","from","0%","\"s\"","'"'"'s'"'"'",
  "url(x)","(",")","[","]","!important","--v","&",">","+","~",
  "<!--","-->","\\","1px","red","e:f","g:h;","\r",
]
const out = []
for (let i = 0; i < Number(process.argv[2]); i++) {
  const n = 1 + Math.floor(rnd() * 9)
  let str = ""
  for (let j = 0; j < n; j++) str += toks[Math.floor(rnd() * toks.length)]
  if (str.includes("\n")) continue
  out.push(str)
}
fs.writeFileSync(process.argv[1], out.join("\n") + "\n")
console.error("probe: generated " + out.length + " inputs")
' "$WORK/in.txt" "$COUNT"

(cd "$HERE/ts" && node -e '
const fs = require("fs")
const { Tabnas } = require("@tabnas/parser")
const { jsonic } = require("@tabnas/jsonic")
const { Css } = require("./dist/css")
const out = []
for (const src of fs.readFileSync(process.argv[1], "utf8").split("\n")) {
  try {
    const v = new Tabnas().use(jsonic).use(Css).parse(src)
    out.push(JSON.stringify(src) + "\tOK " + JSON.stringify(v === undefined ? null : v))
  } catch { out.push(JSON.stringify(src) + "\tERR") }
}
fs.writeFileSync(process.argv[2], out.join("\n") + "\n")
' "$WORK/in.txt" "$WORK/ts.out")

(cd "$HERE/go" && CSS_DUMP_IN="$WORK/in.txt" CSS_DUMP_OUT="$WORK/go.out" \
  go test -run TestDivergenceProbeDump -timeout 900s . >/dev/null)

node -e '
const fs = require("fs")
const canon = (v) => Array.isArray(v) ? v.map(canon)
  : (v && typeof v === "object")
    ? Object.fromEntries(Object.keys(v).sort().map((k) => [k, canon(v[k])]))
    : v
const load = (p) => {
  const m = new Map()
  for (const line of fs.readFileSync(p, "utf8").split("\n")) {
    if (!line) continue
    const i = line.indexOf("\t")
    const src = JSON.parse(line.slice(0, i))
    const rest = line.slice(i + 1)
    m.set(src, rest === "ERR" ? "ERR" : "OK " + JSON.stringify(canon(JSON.parse(rest.slice(3)))))
  }
  return m
}
const a = load(process.argv[1]), b = load(process.argv[2])
let n = 0
for (const [src, av] of a) {
  const bv = b.get(src)
  if (av === bv) continue
  n++
  console.log("DIVERGE " + JSON.stringify(src))
  console.log("   TS: " + String(av).slice(0, 200))
  console.log("   GO: " + String(bv).slice(0, 200))
}
console.log(n === 0
  ? "NO DIVERGENCE (" + a.size + " distinct inputs)"
  : n + " divergences of " + a.size + " distinct inputs")
process.exitCode = 0
' "$WORK/ts.out" "$WORK/go.out"
