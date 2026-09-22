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
#   1. It runs the corpus once per OPTION COMBINATION — all four of them,
#      since the option surface is two booleans. Positions are where the ports
#      are most easily and most invisibly wrong, and every divergence found
#      while writing the Rust port was position-only — so a probe that never
#      turns them on cannot see the class of bug it is most likely to catch.
#      (The Go probe does not, which is why the TS/Go position divergences
#      recorded in AGENTS.md went unreported by it.) `lowercaseProperties`
#      is in for the same reason, one path lower: it rewrites the property
#      name, and nothing else the probe runs exercises that rewrite.
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
  // UPPERCASE property names, without which the two --lowercase-properties
  // modes below double the runtime of this probe and exercise nothing:
  // every other token that can become a property name here is already
  // lowercase, so the option has no work to do, and a port that
  // implemented it differently would go unseen.
  "E:F;","COLOR:red;","Opacity:1","--V:2;","BACKGROUND-COLOR",
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
// SEEDED, not only random. The tokens above can appear in any order, so a
// well-formed `selector{PROPERTY:value}` is vanishingly rare in 4000 draws
// -- and without one, the two --lowercase-properties modes below run the
// whole corpus and rewrite nothing. These few make the option do work; the
// self-check after the loop is what keeps that true.
out.push(
  "a{COLOR:red;}", "a{E:F;}", ".c{Opacity:1}", "a{--V:2;}",
  "a{BACKGROUND-COLOR:red;COLOR:blue}", "@media screen{a{Opacity:1}}",
)
fs.writeFileSync(process.argv[1], out.join("\n") + "\n")
console.error("probe: generated " + out.length + " inputs")
' "$WORK/in.txt" "$COUNT"

# Where cargo PUTS that binary is not always `rs/target/debug/examples`.
# CARGO_TARGET_DIR or a `build.target-dir` moves the root, and a
# `build.target` or CARGO_BUILD_TARGET adds a TRIPLE directory under it --
# which `cargo metadata` does not report, since it answers
# `target_directory` and nothing about the profile or the triple. Deriving
# the path from it was right about the root and wrong about the rest, so on
# a machine with a configured triple (even the host's own) the probe died
# with "no parse example at ..." before comparing anything.
#
# So the path is not derived at all: cargo is asked for the artifact it
# just wrote. `--message-format=json` emits a `compiler-artifact` line per
# built target, and the `executable` field of the one whose target is our
# example is the answer under every profile, target-dir and triple.
# BUILT FOR THE HOST, explicitly. Finding the artifact is not the same as
# being able to run it: a `build.target` or CARGO_BUILD_TARGET naming a
# foreign triple produces a binary this machine cannot execute, and the
# probe would die on exec having compared nothing. `--target` on the
# command line overrides both, so the example is always host-runnable
# whatever the checkout is configured to cross-compile.
HOST="$(rustc -vV | sed -n 's/^host: //p')"
[ -n "$HOST" ] || { echo "probe: rustc did not report a host triple" >&2; exit 2; }

PARSE="$(cargo build --quiet --manifest-path "$HERE/rs/Cargo.toml" --example parse \
  --target "$HOST" --message-format=json-render-diagnostics |
  node -e 'let s = ""
process.stdin.on("data", (d) => (s += d))
process.stdin.on("end", () => {
  let found = ""
  for (const line of s.split("\n")) {
    if ("" === line.trim()) continue
    let m
    try { m = JSON.parse(line) } catch (e) { continue }
    if ("compiler-artifact" !== m.reason || !m.executable) continue
    if ("parse" !== m.target.name || !m.target.kind.includes("example")) continue
    found = m.executable
  }
  console.log(found)
})')"
if [ -z "$PARSE" ] || [ ! -x "$PARSE" ]; then
  echo "probe: cargo reported no parse example executable (got '${PARSE:-}')" >&2
  exit 2
fi

STATUS=0
# EVERY option combination, not just the two the header used to describe.
# `lowercaseProperties` rewrites the property name, which is a text path of
# its own, and a probe that never turns an option on cannot see a port that
# implements it differently. Four combinations of two booleans is the whole
# option surface (AGENTS.md, "Defaults").
for MODE in "" "--position" "--lowercase-properties" "--position --lowercase-properties"; do
  LABEL="${MODE:-default options}"

  node "$HERE/scripts/probe-lines.cjs" $MODE < "$WORK/in.txt" > "$WORK/ts.out"
  "$PARSE" --lines $MODE < "$WORK/in.txt" > "$WORK/rs.out"

  # The default mode is the baseline every other mode is measured against.
  [ -z "$MODE" ] && cp "$WORK/ts.out" "$WORK/ts-base.out"

  # AN OPTION THAT CHANGES NOTHING IS AN OPTION THIS PROBE DOES NOT COVER.
  # Four combinations of two booleans read as four times the coverage, and
  # for a long time --lowercase-properties was three of those runs doing
  # exactly what the default run did: every token that could become a
  # property name was already lowercase. The corpus is seeded now, and this
  # asserts the seeds are still doing their job rather than trusting them.
  if [ -n "$MODE" ]; then
    CHANGED="$(cmp -s "$WORK/ts-base.out" "$WORK/ts.out" && echo 0 || echo 1)"
    if [ "$CHANGED" = "0" ]; then
      echo "probe: [$LABEL] produced output identical to the default mode," >&2
      echo "       so this combination exercised nothing. The corpus no longer" >&2
      echo "       reaches the option's path -- fix the corpus, not this check." >&2
      STATUS=1
    fi
  fi

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
