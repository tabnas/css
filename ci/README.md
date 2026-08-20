# ci/

Staging area for GitHub Actions workflow changes.

This directory exists because session credentials cannot write
`.github/workflows/*` — see admin `DECISIONS.md` ADR-8. To change CI:

1. Put the intended workflow file in `workflows/`.
2. A maintainer promotes it with the admin `rollout/apply-ci-folders.sh`
   script.

## Pending

- **`workflows/divergence.yml`** — runs `scripts/divergence-probe.sh` as a
  gate on every push and pull request.

### Why this is worth a CI job

The probe generates a deterministic pseudo-random corpus of CSS-ish inputs,
parses each with both runtimes, and reports every input they classify or
value differently. It is the instrument that would have caught audit item
C1 — a `slice bounds out of range` panic in the Go scanner, live and
unrecorded, which no shared fixture could see because no fixture happened
to contain the shape that triggered it.

Running it by hand catches that class only when someone remembers to run it.

### It could not have been armed as it was

Until now the script ended with:

```js
process.exitCode = 0
```

and a header saying "This is an INSTRUMENT, not a test. Nothing in CI runs
it." Wiring **that** version into a workflow would have produced a job that
printed every divergence it found and passed anyway — a gate that cannot
fail, which is worse than no gate, because a green tick is read as evidence
that something was checked.

The script now exits non-zero on divergence. `--report-only` keeps the old
behaviour for exploring a change in progress, and is passed as an argument
rather than read from the environment, so a CI job cannot acquire the
opt-out by inheriting a stray variable.

Verified by reverting the TypeScript half of the C1 fix on this branch: the
probe reported 11 divergences of 668 distinct inputs and exited **1**. With
the fix restored it reports NO DIVERGENCE and exits 0.

### What it does not replace

A fixture. When the probe finds something, pin it in `test/spec/` once both
runtimes agree on the answer: a fixture names the case forever and is read
by a human, while the probe only says that one seed found it once.
