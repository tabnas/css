#!/usr/bin/env bash
# TS/Rust differential probe — the Rust sibling of divergence-probe.sh.
#
# Generates a DETERMINISTIC pseudo-random corpus of CSS-ish inputs from a token
# alphabet, parses every one with both runtimes, canonicalises the two results
# and reports every input the two runtimes classify or value differently.
#
# Nothing here is a third-party corpus: the inputs are generated from the seed
# below, so a run is reproducible without vendoring anything.
#
# This is a GATE. It EXITS NON-ZERO when the two runtimes disagree.
#
# TWO DIFFERENCES FROM THE GO PROBE, both deliberate:
#
#   1. It runs the corpus TWICE, once with the `position` option off and once
#      with it on. Positions are where the ports are most easily and most
#      invisibly wrong, and every divergence found while writing the Rust port
#      was position-only — so a probe that never turns them on cannot see the
#      class of bug it is most likely to catch. (The Go probe does not, which
#      is why the TS/Go position divergences recorded in AGENTS.md went
#      unreported by it.)
#
#   2. Newlines ride in the corpus as the `\n` escape the shared fixtures use,
#      rather than being dropped. Line and column tracking is only exercised
#      by multi-line input.
#
# Usage:  bash scripts/divergence-probe-rs.sh [count] [--report-only]
#
#   count          how many inputs to generate (default 4000)
#   --report-only  list divergences and exit 0. For exploring a change in
#                  progress. Never use it in CI, which is the one place the
#                  exit code is the whole point.
#
# Requires a built ts/dist (npm run build from ts/) and a cargo toolchain.
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
  "a","b",".c","#d","*","{","}",":",";",","," ","\n","\t","\r",
  "/*","*/","/*x*/","@media","@import","@keyframes","@font-face","@page",
  "@supports","@-webkit-keyframes","@-moz-document","@document","@charset",
  "@custom-media","@layer","@container","@host","@viewport","@counter-style",
  "screen","from","to","0%","50%","\"s\"","'"'"'s'"'"'","\"a;b\"",
  "url(x)","(",")","[","]","!important","--v","&",">","+","~",
  "<!--","-->","\\","\\3A ","1px","red","e:f","g:h;","opacity[sqrt]",
  "//x","#h","*p","©","\u{1d11e}","--test","url-prefix()","::before",
  // The code points where char::is_whitespace in Rust and
  // String.prototype.trim in JavaScript disagree, plus the ones they agree
  // on. A trim written against the wrong set is invisible without them, and
  // it silently changes the AST.
  "\ufeff","\u0085","\u00a0","\u2028","\u2029",
]
const out = []
for (let i = 0; i < Number(process.argv[2]); i++) {
  const n = 1 + Math.floor(rnd() * 12)
  let str = ""
  for (let j = 0; j < n; j++) str += toks[Math.floor(rnd() * toks.length)]
  // Escaped, not dropped: the runners decode these, so multi-line inputs are
  // probed rather than skipped.
  out.push(str.replace(/\\/g, "\\\\").replace(/\n/g, "\\n")
              .replace(/\t/g, "\\t").replace(/\r/g, "\\r"))
}
fs.writeFileSync(process.argv[1], out.join("\n") + "\n")
console.error("probe: generated " + out.length + " inputs")
' "$WORK/in.txt" "$COUNT"

cargo build --quiet --manifest-path "$HERE/rs/Cargo.toml" --example parse

# Where cargo PUT that binary is not always `rs/target`: CARGO_TARGET_DIR, or
# a `build.target-dir` in any cargo config the machine carries, moves it. Ask
# cargo rather than assuming, so the probe runs on a machine with a shared
# target directory instead of dying with "No such file or directory".
TARGET_DIR="$(cargo metadata --manifest-path "$HERE/rs/Cargo.toml" \
  --format-version 1 --no-deps |
  node -e 'let s = ""
process.stdin.on("data", (d) => (s += d))
process.stdin.on("end", () => console.log(JSON.parse(s).target_directory))')"
PARSE="$TARGET_DIR/debug/examples/parse"
if [ ! -x "$PARSE" ]; then
  echo "probe: no parse example at $PARSE" >&2
  exit 2
fi

STATUS=0
for MODE in "" "--position"; do
  LABEL="${MODE:-default options}"

  node "$HERE/scripts/probe-lines.cjs" $MODE < "$WORK/in.txt" > "$WORK/ts.out"
  "$PARSE" --lines $MODE < "$WORK/in.txt" > "$WORK/rs.out"

  node -e '
const fs = require("fs")
const canon = (v) => Array.isArray(v) ? v.map(canon)
  : (v && typeof v === "object")
    ? Object.fromEntries(Object.keys(v).sort().map((k) => [k, canon(v[k])]))
    : v
const load = (p) => fs.readFileSync(p, "utf8").split("\n").map((line) =>
  line.startsWith("ERROR") || "" === line ? line : JSON.stringify(canon(JSON.parse(line))))
const src = fs.readFileSync(process.argv[1], "utf8").split("\n")
const a = load(process.argv[2]), b = load(process.argv[3])
let n = 0
for (let i = 0; i < src.length; i++) {
  if ("" === src[i] || a[i] === b[i]) continue
  n++
  if (n <= 20) {
    console.log("DIVERGE " + JSON.stringify(src[i]))
    console.log("   TS: " + String(a[i]).slice(0, 200))
    console.log("   RS: " + String(b[i]).slice(0, 200))
  }
}
const label = process.argv[5]
console.log(0 === n
  ? "NO DIVERGENCE [" + label + "] (" + src.filter(Boolean).length + " inputs)"
  : n + " divergences [" + label + "] of " + src.filter(Boolean).length + " inputs")

// A divergence FAILS unless the caller explicitly asked for a report. The
// argument is passed rather than read from the environment so that a CI job
// cannot acquire the opt-out by inheriting a stray variable.
if (0 < n && "1" !== process.argv[4]) {
  process.exitCode = 1
}
' "$WORK/in.txt" "$WORK/ts.out" "$WORK/rs.out" "$REPORT_ONLY" "$LABEL" || STATUS=1
done

exit "$STATUS"
