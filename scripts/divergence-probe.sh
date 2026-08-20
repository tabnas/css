#!/usr/bin/env bash
# TS/Go differential probe.
#
# Generates a DETERMINISTIC pseudo-random corpus of CSS-ish inputs from a token
# alphabet, parses every one with both runtimes, canonicalises the two results
# (Go marshals map keys in sorted order) and reports every input the two
# runtimes classify or value differently.
#
# Nothing here is a third-party corpus: the inputs are generated from the seed
# below, so a run is reproducible without vendoring anything.
#
# This is a GATE. It EXITS NON-ZERO when the two runtimes disagree, and CI
# runs it (ci/divergence.yml, staged for maintainer promotion per ADR-8).
#
# It used to end with `process.exitCode = 0` and a header saying nothing in CI
# ran it. Wiring that version into CI would have produced a job that reported
# every divergence and passed anyway — a gate that cannot fail, which is worse
# than no gate because the green tick is read as evidence.
#
# Still run it by hand when changing the grammar or porting a fix, and pin
# whatever it finds as a fixture in test/spec/ once both runtimes agree: a
# fixture names the case forever, while the probe only says a seed found it.
#
# Usage:  bash scripts/divergence-probe.sh [count] [--report-only]
#
#   count          how many inputs to generate (default 4000)
#   --report-only  list divergences and exit 0. For exploring a change in
#                  progress. Never use it in CI, which is the one place the
#                  exit code is the whole point.
#
# Requires a built ts/dist (npm run build) and a working go toolchain.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

COUNT=4000
REPORT_ONLY=0
for arg in "$@"; do
  case "$arg" in
    --report-only) REPORT_ONLY=1 ;;
    ''|*[!0-9]*) echo "unknown argument: $arg" >&2; exit 2 ;;
    *) COUNT="$arg" ;;
  esac
done
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
  // Bare, UNPAIRED quotes. The pool held only complete pairs -- "s" and
  // '"'"'s'"'"' -- so concatenating tokens could never put a lone quote in
  // text position, and this generator could not emit that class at all.
  //
  // It had a live divergence in it. On `a"b` TypeScript raised
  // jsonic/unexpected and Go raised jsonic/unterminated_string, because Go
  // could not represent the "no quote characters" the plugin asked for.
  // The fleet probe in admin found it; this one could not have, however
  // many seeds it was given, and for TWO independent reasons. It could not
  // build the input, and both halves recorded a reject as a bare "ERR", so
  // even given the input the two runtimes would have diffed as agreeing.
  // Both are fixed here. A generator that cannot emit a class cannot find a
  // bug in it -- and neither can a comparison that cannot tell two
  // rejections apart.
  "\"","'"'"'","'"'"'x","\"x",
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
  } catch (e) {
    const m = /\[[a-z0-9-]+\/([a-z0-9_]+)\]/.exec(String(e && e.message))
    out.push(JSON.stringify(src) + "\tERR " + (m ? m[1] : "?"))
  }
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
    m.set(src, rest.startsWith("ERR")
      ? rest
      : "OK " + JSON.stringify(canon(JSON.parse(rest.slice(3)))))
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

// A divergence FAILS unless the caller explicitly asked for a report. The
// argument is passed rather than read from the environment so that a CI job
// cannot acquire the opt-out by inheriting a stray variable.
if (0 < n && "1" !== process.argv[3]) {
  process.exitCode = 1
}
' "$WORK/ts.out" "$WORK/go.out" "$REPORT_ONLY"
