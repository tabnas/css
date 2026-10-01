# Build, test and publish the TypeScript (ts/), Go (go/) and Rust (rs/)
# implementations. ts/ is canonical; go/ and rs/ track it.
#
# Local build/test resolve the unpublished @tabnas siblings via the
# repo-set go.work + node_modules symlinks (admin/scripts/link.sh).

.PHONY: all build test clean build-ts build-go build-rs \
        test-ts test-go test-rs gate-rs version-rs clean-ts clean-go clean-rs \
        publish-ts publish-go tags-go reset \
        prose prose-counts probe probe-rs

all: build test

build: build-ts build-go build-rs

test: test-ts test-go test-rs

clean: clean-ts clean-go clean-rs

# --- TypeScript (package in ts/) ---
build-ts:
	cd ts && npm run build

test-ts:
	cd ts && npm test

clean-ts:
	rm -rf ts/dist ts/dist-test

# Publish the TypeScript package at its current package.json version.
publish-ts: test-ts
	cd ts && npm publish --access public

# --- Go (module in go/) ---
build-go:
	cd go && go build ./...

test-go:
	cd go && go test -v ./...

clean-go:
	cd go && go clean

# Publish the Go module: make publish-go V=x.y.z
# Injects V into the Go `VERSION` const, commits, tags go/vX.Y.Z, and
# (when gh is available) creates a GitHub release.
publish-go: test-go
	@test -n "$(V)" || (echo "Usage: make publish-go V=x.y.z" && exit 1)
	sed -i.bak 's/^const VERSION = ".*"/const VERSION = "$(V)"/' go/css.go
	rm -f go/css.go.bak
	git add go/css.go
	git commit -m "go: v$(V)"
	git tag go/v$(V)
	git push origin main go/v$(V)
	@command -v gh >/dev/null 2>&1 && gh release create go/v$(V) --title "go/v$(V)" --notes "Go module release v$(V)" || true

# List published Go module tags, newest first.
tags-go:
	git tag -l 'go/v*' --sort=-version:refname

# --- Rust (crate in rs/) ---
#
# There is no publish target. The crate is released by the same dispatch
# that publishes the other two; a local `cargo publish` is not the
# release path, for the same reason a local `npm publish` is not.
#
# The crate takes the engine, jsonic and (for its tests) the debug plugin
# by path from sibling checkouts: clone tabnas/parser, tabnas/json,
# tabnas/jsonic and tabnas/debug next to this repo. These targets are the
# fast inner loop; `gate-rs` is the full gate CI runs. rs/Cargo.lock is
# committed and none of these is `--locked` (the siblings are checkouts of
# main, so their versions move), so a run can rewrite it: commit a lock
# change only when you meant one, and `gate-rs` checks that you did.
build-rs:
	cd rs && cargo build --all-targets

test-rs:
	cd rs && cargo test --all-targets && cargo test --doc
	cd rs && cargo clippy --all-targets --all-features -- -D warnings

gate-rs:
	ci/rust/run.sh

clean-rs:
	cd rs && cargo clean

# Set the Rust crate version: make version-rs V=x.y.z
#
# Bumps BOTH Rust version sites, rs/Cargo.toml and VERSION in rs/src/lib.rs,
# plus the crate's own entry in rs/Cargo.lock, which ci/rust/run.sh checks
# against the manifest before anything else runs. The release orchestrator
# rewrites the TypeScript and Go sites and does not know about these;
# rs/tests/version.rs fails a release that forgets them.
#
# The lock entry is edited in place rather than by running cargo: any cargo
# command resolves the whole graph against the sibling checkouts, and would
# re-pin whatever they have moved, which is a dependency change a version
# bump must not make.
version-rs:
	@test -n "$(V)" || (echo "Usage: make version-rs V=x.y.z" && exit 1)
	sed -i.bak 's/^version = ".*"/version = "$(V)"/' rs/Cargo.toml
	sed -i.bak 's/^pub const VERSION: &str = ".*";/pub const VERSION: \&str = "$(V)";/' rs/src/lib.rs
	rm -f rs/Cargo.toml.bak rs/src/lib.rs.bak
	awk -v v="$(V)" '$$0 == "name = \"tabnas-css\"" { f = 1; print; next } \
	  f && /^version = / { print "version = \"" v "\""; f = 0; next } { print }' \
	  rs/Cargo.lock > rs/Cargo.lock.tmp
	mv rs/Cargo.lock.tmp rs/Cargo.lock

# The differential probes: both are GATES and exit non-zero on a
# divergence. Not part of `test` because each needs the other runtime
# built; run them when changing the grammar or porting a fix.
probe:
	bash scripts/divergence-probe.sh 4000

probe-rs:
	bash scripts/divergence-probe-rs.sh 4000

reset:
	cd ts && npm run reset
	cd go && go clean -cache && go build ./... && go test -v ./...
	cd rs && cargo clean && cargo test

# The prose gate (see docs/STYLE-GUIDE.md). Vale over the reader-facing
# pages, at the levels set in .vale.ini, on the same file list
# ts/test/docs.test.js reads. Requires `vale` on PATH and one
# `vale sync`. Warnings are advisory, errors fail.
prose:
	vale --minAlertLevel=error $$(node ts/scripts/gated-docs.cjs)
	node ts/scripts/vale-counts.cjs

# Re-measure what .vale.ini and the style guide record, after
# a change to the pages or to the rules moves the numbers.
prose-counts:
	node ts/scripts/vale-counts.cjs --write
