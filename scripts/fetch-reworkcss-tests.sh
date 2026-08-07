#!/usr/bin/env bash
# Fetch the reworkcss/css test corpus into test/reworkcss-css/ so both the
# TypeScript and Go conformance runners can exercise the parser against the
# third-party suite for the AST model this plugin explicitly claims.
#
# UPSTREAM (pinned):
#   https://github.com/reworkcss/css
#   commit ae6a6f9bf939cbcbc759a12d9f208afb5d4dde75  (2026-06-04)
#
# README.md and AGENTS.md both state that the output "AST shape follows the
# widely-used reworkcss/css model", so reworkcss's own test/cases/*/ast.json
# files are the authoritative expected VALUES for this plugin, not merely
# accept/reject oracles. test/parse.js additionally carries the must-throw /
# must-not-throw inputs, which the runners extract mechanically.
#
# A commit SHA is pinned, not a branch: the conformance numbers in this
# repository always refer to one exact corpus.
#
# The corpus is owned by its authors (MIT, see its LICENSE) and is NOT
# redistributed as part of this repository -- test/reworkcss-css/ is
# gitignored and must never be committed.
#
# The script is idempotent: if the pinned commit is already checked out it
# exits 0 without touching the network. Pass --force to re-fetch.
#
# The conformance tests run this automatically:
#   ts/  -> the `pretest` npm script
#   go/  -> TestMain in go/reworkcss_test.go
# If the corpus is missing the tests FAIL LOUDLY. They never skip.
set -euo pipefail

REPO="https://github.com/reworkcss/css.git"
SHA="ae6a6f9bf939cbcbc759a12d9f208afb5d4dde75"

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="${1:-$HERE/test/reworkcss-css}"
FORCE=0
if [ "${1:-}" = "--force" ]; then FORCE=1; DEST="$HERE/test/reworkcss-css"; fi
if [ "${2:-}" = "--force" ]; then FORCE=1; fi

if [ "$FORCE" = "0" ] && [ -d "$DEST/.git" ]; then
  have="$(git -C "$DEST" rev-parse HEAD 2>/dev/null || true)"
  if [ "$have" = "$SHA" ]; then
    echo "reworkcss/css: already at $SHA ($DEST)"
    exit 0
  fi
fi

rm -rf "$DEST"
mkdir -p "$DEST"
git -C "$DEST" init -q
git -C "$DEST" remote add origin "$REPO"
git -C "$DEST" fetch -q --depth 1 origin "$SHA"
git -C "$DEST" checkout -q FETCH_HEAD

have="$(git -C "$DEST" rev-parse HEAD)"
if [ "$have" != "$SHA" ]; then
  echo "ERROR: reworkcss/css checked out $have, expected $SHA" >&2
  exit 1
fi

if [ ! -d "$DEST/test/cases" ] || [ ! -f "$DEST/test/parse.js" ]; then
  echo "ERROR: reworkcss/css corpus is missing test/cases or test/parse.js" >&2
  exit 1
fi

echo "reworkcss/css: fetched $SHA into $DEST"
