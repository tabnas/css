#!/usr/bin/env bash
# Fetch every third-party conformance corpus this repository measures itself
# against. Idempotent; safe to run before every test invocation.
#
# Corpora are NEVER committed (see .gitignore). This script is the only
# supported way to obtain them, and it pins an exact upstream commit SHA for
# each so a published conformance number always names one exact corpus.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
bash "$HERE/fetch-css-parsing-tests.sh" "$@"
bash "$HERE/fetch-reworkcss-tests.sh" "$@"
