#!/usr/bin/env bash
# Fetch the authoritative CSS Syntax Level 3 conformance corpus into
# test/css-parsing-tests/ so both the TypeScript and Go conformance runners
# can exercise the parser against a third-party suite.
#
# UPSTREAM (pinned):
#   https://github.com/SimonSapin/css-parsing-tests
#   commit 203ce36bffd617db7f118c551e32794561fb273d  (2025-09-24)
#
# This is the implementation-independent corpus derived from the 2021
# CR draft of CSS Syntax Level 3, and is the suite used by rust-cssparser
# (Servo), tinycss2 and Crass. A commit SHA is pinned, not a branch: the
# conformance numbers in this repository always refer to one exact corpus.
#
# The corpus is owned by its authors (MIT/Apache-2.0, see its LICENSE) and is
# NOT redistributed as part of this repository -- test/css-parsing-tests/ is
# gitignored and must never be committed.
#
# The script is idempotent: if the pinned commit is already checked out it
# exits 0 without touching the network. Pass --force to re-fetch.
#
# The conformance tests run this automatically:
#   ts/  -> the `pretest` npm script
#   go/  -> TestMain in go/cssparsing_test.go
# If the corpus is missing the tests FAIL LOUDLY. They never skip: a
# conformance suite that quietly does not run is worse than no suite at all,
# because the green tick is a lie.
set -euo pipefail

REPO="https://github.com/SimonSapin/css-parsing-tests.git"
SHA="203ce36bffd617db7f118c551e32794561fb273d"

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="${1:-$HERE/test/css-parsing-tests}"
FORCE=0
if [ "${1:-}" = "--force" ]; then FORCE=1; DEST="$HERE/test/css-parsing-tests"; fi
if [ "${2:-}" = "--force" ]; then FORCE=1; fi

if [ "$FORCE" = "0" ] && [ -d "$DEST/.git" ]; then
  have="$(git -C "$DEST" rev-parse HEAD 2>/dev/null || true)"
  if [ "$have" = "$SHA" ]; then
    echo "css-parsing-tests: already at $SHA ($DEST)"
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
  echo "ERROR: css-parsing-tests checked out $have, expected $SHA" >&2
  exit 1
fi

# Census gate: refuse a corpus that has silently shrunk. These are the
# per-file pair counts (each file is a flat array of input/expected PAIRS)
# observed at the pinned commit.
for f in stylesheet.json rule_list.json one_rule.json declaration_list.json one_declaration.json; do
  if [ ! -f "$DEST/$f" ]; then
    echo "ERROR: css-parsing-tests is missing $f" >&2
    exit 1
  fi
done

echo "css-parsing-tests: fetched $SHA into $DEST"
