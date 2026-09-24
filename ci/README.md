# ci/

Notes on this repository's own CI workflows. The scripts they run live
in `scripts/` (`divergence-probe.sh` and `divergence-probe-rs.sh`), so
this directory holds only this file.

The workflows themselves live in `.github/workflows/`. To change CI, edit
them there in a reviewed pull request: session credentials can push
workflow changes (admin `DECISIONS.md` ADR-8, as amended on 2026-09-24),
so staging a workflow here for a maintainer to promote is optional.
Sessions still cannot push tags. Releases therefore go through
`workflow_dispatch`, and a workflow that runs only on a tag push needs a
maintainer to push that tag.

Five of this repository's workflows also have a template in admin
`rollout/workflows/`: `ci.yml`, `crates-release.yml`, `notify-status.yml`,
`release.yml` and `scorecard.yml`. ADR-8 as amended says a workflow
changed here is mirrored in its template, so change the template too, in
admin. Otherwise admin `scripts/verify.sh` reports the drift, and the
next `rollout/apply-workflows.sh --apply` pushes the old text back.
`clib.yml` and `clib-release.yml` are stamped (each carries a
`tabnas-clib-template` marker): they change only through admin
`tasks/clib-template/` and a re-stamp with `tasks/adopt-clib.sh`, never
by hand. The others have no template and change here alone.

## Promoted

Both of these were staged here and now run from `.github/workflows/`:

- **`divergence.yml`** — runs `scripts/divergence-probe.sh` as a gate on
  every push to `main` and every pull request against it.
- **`rust.yml`** — builds and tests the Rust port (`rs/`), runs clippy and
  rustfmt, and runs `scripts/divergence-probe-rs.sh` as a gate.


### Why the divergence probe is worth a CI job

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

### Why the Rust job is worth a workflow

The shared `polyglot-ci.yml` knows about a repository's TypeScript and Go
halves and has no Rust step, so **this workflow is the only place CI runs
`cargo test`**. What it measures is not a small thing: the Rust suite runs the
shared `test/spec/*.tsv` fixtures and the whole pinned reworkcss corpus, which
is the parity contract and the conformance bar for a third runtime. A runtime
nothing runs is a runtime nobody is measuring, and the README claims the tree
is the same in all three.

The probe half is a separate job because it needs the TypeScript port built,
which the build-and-test job does not. It runs the generated corpus twice,
with `position` off and on: the two TS/Go divergences recorded in the root
`AGENTS.md` are position-only, and a probe that never turns positions on
cannot see the class of bug that it is most likely to catch.
