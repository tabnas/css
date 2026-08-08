#!/usr/bin/env bash
# Fetch the reworkcss/css conformance corpus into test/reworkcss-css/.
#
# README.md and AGENTS.md state that this plugin's AST "follows the widely-used
# reworkcss/css model", so reworkcss's own `test/cases/*/{input.css,ast.json}`
# pairs are the authoritative expected VALUES for the parse, and the
# `assert.throws` / `assert.doesNotThrow` inputs in its `test/parse.js` are the
# authoritative accept/reject oracle.
#
# The corpus is third-party (MIT, Copyright TJ Holowaychuk and contributors)
# and is NOT redistributed in this repository — running this script is an
# explicit opt-in to clone it. It is pinned to a commit so the conformance
# number cannot drift under us.
#
# Usage:
#   scripts/fetch-reworkcss-tests.sh            # default location
#   scripts/fetch-reworkcss-tests.sh /some/dir  # custom destination
#
# After fetching, both conformance runners pick the corpus up automatically:
#   cd ts && npm test     # ts/test/reworkcss.test.ts
#   cd go && go test ./... # go/reworkcss_test.go
set -euo pipefail

URL="https://github.com/reworkcss/css.git"
# Keep in step with SHA in ts/test/reworkcss.test.ts and go/reworkcss_test.go.
SHA="ae6a6f9bf939cbcbc759a12d9f208afb5d4dde75"

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="${1:-$REPO_ROOT/test/reworkcss-css}"

# Already at the pinned commit: nothing to do, and no network needed. This
# is the fast path that lets `npm test` call this script unconditionally.
if [ "$(git -C "$DEST" rev-parse HEAD 2>/dev/null || true)" = "$SHA" ] &&
   [ -d "$DEST/test/cases" ]; then
  echo "Corpus already at $SHA."
  exit 0
fi

if [ -d "$DEST/.git" ]; then
  echo "Corpus present at $DEST — checking out $SHA ..."
  git -C "$DEST" fetch --quiet origin "$SHA" 2>/dev/null || git -C "$DEST" fetch --quiet origin
else
  echo "Cloning $URL into $DEST ..."
  git clone --quiet "$URL" "$DEST"
fi

git -C "$DEST" checkout --quiet "$SHA"

cases=$(find "$DEST/test/cases" -mindepth 1 -maxdepth 1 -type d | wc -l)
echo "Done. $cases conformance cases at $SHA."
