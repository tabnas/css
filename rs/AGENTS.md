# Agents Guide — rs/

The Rust port of the canonical TypeScript in [`../ts`](../ts). Read
[`../AGENTS.md`](../AGENTS.md) first: it holds the AST contract, the
grammar-embedding workflow, the conformance bar and the cross-runtime
rules. This file covers only what is specific to this crate.

## Layout

| Path | |
|---|---|
| `src/lib.rs` | the public surface: `parse`, `parse_with`, `Css`, `VERSION`, and the `#[cfg(doctest)]` module that makes every Markdown example a doctest |
| `src/lex.rs` | the `cssToken` matcher and its scanners, the token set, `Error`, and the ECMAScript whitespace helpers |
| `src/machine.rs` | the rule machine, `Options`, and the empty result for `""` |
| `src/grammar.rs` | `GRAMMAR_TEXT` (the embedded `css-grammar.jsonic`) and the reader for the jsonic subset it is written in |
| `src/value.rs` | `Value` and `Node`, with every tree walk written as a stack machine |
| `tests/parity.rs` | the shared `../test/spec/*.tsv` fixtures |
| `tests/divergent.rs` | the `rust` column of `../test/divergent.tsv` |
| `tests/reworkcss.rs` | the pinned reworkcss/css corpus, fetched by the test itself |
| `tests/css.rs` | what a fixture cannot express, plus the crate's API |
| `tests/grammar.rs` | the embed against the file on disk, and the subset reader |
| `tests/perf.rs`, `tests/version.rs` | the instance-reuse guard and the version sites |
| `tests/support/mod.rs` | the fixture loader, a JSON reader and the canonical compare |
| `doc/*.md`, `README.md` | the four Diátaxis pages and the crate front page, all gated by the prose gate |
| `examples/parse.rs` | the binary the TS/Rust differential probe drives |

## This port has no engine under it

The TypeScript and Go ports are jsonic plugins: they install a grammar on a
shared engine and borrow its lexer and rule machine. There is no Rust build
of that engine, so this crate carries both, and it declares **no
dependencies at all** — not even for the test profile. `@tabnas/support` has
a Rust half (the `tabnas-support` crate) and this crate deliberately does not
take it, so `cargo test` runs from a checkout of this repository alone, with
no sibling path dependency and no registry access. `tests/support/mod.rs` is
the price of that, which is why it is kept small.

The same absence is visible to a caller: there is no relaxed-JSON base to
switch off, so `{a:1}` is rejected because it is not CSS rather than because
an option turned it off. `test/spec/leniency.tsv` holds the port to the same
verdicts as the other two.

## Things this port does differently, and why

Each of these is behaviour, not implementation detail. A change to any of
them shows up in a parse result.

- **Lookahead is lazy.** `machine.rs` reads a token only when the alt being
  tried needs one, under the rule trying it, and within an alt only while it
  still matches. Both halves decide the tree: a comment right after `{` is a
  node because the block wrapper's `#OB #CB` alt reads the first body token
  under the WRAPPER's name, and `b{,/*!important` fails as `unexpected`
  rather than `unterminated_comment` because `decl`'s first alt fails before
  the unterminated comment behind it is ever read. Reading a whole `s:`
  sequence up front changes the error code.
- **The node MOVES down the rule stack.** The canonical ports let a child
  rule inherit its parent's node by reference. There is no such aliasing
  here, so a pushed child takes the node, a constructor action hands it back
  before installing its own, and a popping rule either returns it or delivers
  its own node as the parent's `child`. Only the top frame ever runs, which
  is what makes that safe, and why `set_node` touches `stack[top - 1]`.
- **Nothing reachable from a parse result recurses per level.** A tree is as
  deep as its source, so a derived `Drop`, `Clone`, `PartialEq` or `Debug`
  would abort the process on untrusted input. All four, and the JSON output,
  are explicit stack machines in `value.rs`; `tests/css.rs` pins them at
  20,000 levels. `Debug` is the one that is easy to forget, because it is
  what a caller reaches for while debugging.
- **`str::trim` is not `String.prototype.trim`.** Use `es_trim` and
  `es_is_whitespace` from `lex.rs` at every site that produces AST text. The
  two sets differ by U+FEFF and U+0085, and both change the tree; see the
  divergence register.
- **Columns count UTF-16 code units**, measured with `char::len_utf16`,
  because a column here is the column the canonical port reports. A byte
  count is wrong for `#©{…}` and a `char` count is wrong for astral
  characters such as `#𝄞{…}`.
- **The grammar reader rejects a field it does not implement.** The other two
  ports hand the document to an engine that understands the whole jsonic alt
  surface; this one implements `s`, `b`, `p`, `r`, `a` and `g` on an alt,
  `open`/`close` on a rule and `rule` at the top. Anything else is an error
  rather than a field read as absent, so a grammar that grows a field stops
  this build instead of silently running a different grammar here. Teaching
  `grammar.rs` and `machine.rs` what a new field does is part of adding it.

## Gates

```bash
cargo fmt --all --check
cargo build --all-targets
cargo test --all-targets
cargo test --doc          # --all-targets does NOT include doctests
cargo clippy --all-targets --all-features -- -D warnings
cargo doc --no-deps       # broken intra-doc links are denied in lib.rs
```

`cargo test` fetches the reworkcss corpus for itself (`fetch_corpus()` in
`tests/reworkcss.rs`) and **fails** rather than skipping if it is still
absent afterwards.

The differential probe is a gate too, and is not part of `cargo test`,
because it needs the TypeScript port built:

```bash
bash ../scripts/divergence-probe-rs.sh 4000    # or `make probe-rs`
```

It runs the corpus once per option combination, all four of them, and
resolves the `parse` example through `cargo metadata` rather than assuming
`rs/target`, so a shared target directory does not break it.

**No GitHub workflow runs any of this yet.** `ci/workflows/rust.yml` is
staged and needs a maintainer to promote it (session credentials cannot
write `.github/workflows/*`), so until then the Rust gates run locally or
not at all. Run them before merging anything that touches `rs/`, the
grammar, or a fixture.

## Versions and releasing

`version` in `Cargo.toml` and `VERSION` in `src/lib.rs` must both equal
`ts/package.json` `"version"`; `tests/version.rs` reads both and fails the
build if either drifts. The release orchestrator rewrites the TypeScript and
Go sites and **does not know about these two**, so a release bumps them by
hand and `tests/version.rs` is what catches the omission.

`Cargo.lock` is gitignored: this is a library crate with no dependencies, so
the lockfile is not the dependency contract and nothing in this repository
reads it with `--locked`.

The crate is **not published by `release.yml`**, which publishes the npm
package and tags the Go module. Putting it on crates.io is a separate
decision with its own trusted-publishing setup; a local `cargo publish` is
not the release path, for the same reason a local `npm publish` is not.
