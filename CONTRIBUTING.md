# Contributing to css

Thanks for your interest in contributing! The organization-wide conventions
in [tabnas/.github](https://github.com/tabnas/.github/blob/main/CONTRIBUTING.md) are
canonical and apply here. This file adds what is specific to
**tabnas/css**.

Start with [`AGENTS.md`](AGENTS.md) — it is the working guide to this
repository for humans and agents alike.

## Build & test

This repository is *polyglot*: `ts/`, `go/` and `rs/` hold three parallel
implementations of the same package. **`ts/` is canonical; `go/` and `rs/`
track it** — a behaviour change normally lands in all three, with tests in
all three.

```bash
make build     # builds ts/, go/ and rs/
make test      # tests ts/, go/ and rs/
make gate-rs   # the full Rust gate, ci/rust/run.sh, as CI runs it

# or per stack:
cd ts && npm install && npm run build && npm test
cd go && go build ./... && go test ./...
cd rs && cargo build && cargo test
```

The Rust crate is a plugin on the engine and takes it by path from sibling
checkouts: clone `tabnas/parser`, `tabnas/json`, `tabnas/jsonic` and
`tabnas/debug` next to this repository, and use Rust 1.85 or newer
(`rust-version` in `rs/Cargo.toml`). The shared workflow has no Rust step,
so this repository's own `.github/workflows/rust.yml` clones those siblings
and runs `ci/rust/run.sh`; run it locally too (`make gate-rs`) before
opening a PR that touches `rs/`, the grammar, or a shared fixture.
`rs/Cargo.lock` is committed: commit a change to it only when you meant
one, and the gate fails a lock that no longer describes the manifest.

The TypeScript and Go sides install published packages, `@tabnas/*` from
the npm registry and `github.com/tabnas/*/go` from the module proxy, so they
need no other checkout. Sibling checkouts are optional there: to work
against unreleased siblings, clone them into the same parent directory and
run admin's `scripts/link.sh`, which links them over
`ts/node_modules/@tabnas/*` and writes a `go.work` one level up. Never commit
that wiring. CI builds the siblings named in `.github/workflows/ci.yml`'s
`deps` from source. The Rust checkouts above are needed because
`rs/Cargo.toml` stays path-only: the crates are on crates.io, and the release
workflow swaps the paths for crates.io versions only when it publishes
`tabnas-css`.

## Commit messages

[Conventional Commits](https://www.conventionalcommits.org/) are required,
for commit messages and PR titles alike. PRs are squash-merged, so a PR's
title is its commit message, and the GitHub Release that each release creates
lists those titles in its generated notes. They do not set the version: a
release is its own version-bump pull request, then a `release.yml` dispatch
(see [`AGENTS.md`](AGENTS.md), "Releasing"). For example:

```
feat: add lax mode for trailing commas
fix: handle CRLF inside block scalars
docs: clarify plugin ordering
```

Use `feat!:` / `fix!:` (or a `BREAKING CHANGE:` footer) for breaking changes.

## Pull requests

1. Open an issue first for anything larger than a small fix.
2. Branch from `main`; keep the PR focused on one change.
3. `make test` must pass for **all three** implementations.
4. PR titles follow Conventional Commits — PRs are squash-merged, so the
   title becomes the commit message.
5. CI must be green before merge.

## Security issues

Never open a public issue for a vulnerability — see [SECURITY.md](SECURITY.md).

## Code of conduct

Participation is covered by the org
[Code of Conduct](https://github.com/tabnas/.github/blob/main/CODE_OF_CONDUCT.md).
