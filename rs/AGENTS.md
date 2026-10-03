# Agents Guide — rs/

The Rust port of the canonical TypeScript in [`../ts`](../ts). Read
[`../AGENTS.md`](../AGENTS.md) first: it holds the AST contract, the
grammar-embedding workflow, the conformance bar and the cross-runtime
rules. This file covers only what is specific to this crate.

## Layout

| Path | |
|---|---|
| `src/lib.rs` | the public surface: `plugin`, `css`, `make`, `make_with`, `Options` (with `from_value` and `to_value`), `Css`, `parse`, `parse_with`, `VERSION`, the re-exports of `tabnas` and `tabnas_jsonic`, and the `#[cfg(doctest)]` module that makes every Markdown example a doctest |
| `src/plugin.rs` | the install, the actions that build the nodes, the two node stores (tree and arena), UTF-16 positions (`col16`), the arena meta, the `tabnas-css/depth` guard and `TREE_RULE_DEPTH` |
| `src/lex.rs` | the `cssToken` matcher and its scanners (with `BraceScans`, a group scanned once), the lex subscriber (end-of-input overshoot, bad-token lookahead), `Tin`, `Error` and `From<TabnasError>`, and the ECMAScript whitespace helpers |
| `src/grammar.rs` | `GRAMMAR_TEXT` (the embedded `css-grammar.jsonic`), `OPTIONS_DOC` (the canonical option overrides, plus `rule.history: 1` and the `@css-prepare` hook), `RULES`, and `specs()`, which reads the text with jsonic once per process |
| `src/value.rs` | `Value` and `Node`, with every tree walk written as a stack machine, and `from_arena`, which builds the tree from the arena without recursion |
| `tests/parity.rs` | every shared `../test/spec/*.tsv` row three ways: `Css::parse`, the plugin on jsonic, and the plugin on a bare engine |
| `tests/divergent.rs` | the `rust` column of `../test/divergent.tsv`, read through the engine's API, and `Css::parse` against the `ts` cell on every row |
| `tests/css.rs` | what a fixture cannot express, plus the crate's API: key order, an absent `position.end`, error positions taken from the TypeScript suite, 20,000-level nesting on a 2 MiB thread |
| `tests/grammar.rs` | the embed against the file on disk, the rules the plugin installs, and every alternate in the `css` group |
| `tests/plugin.rs` | the engine API: the tree form against `Css::parse`, a bare engine, option truthiness, a second install and `derive`, `merge`, the depth bound and a later `depth` guard, recovery and relexing, a caller's meta, a seeded context, error columns |
| `tests/repeat.rs` | every repetition a replace loop: rule depth over 10,000 items of each, the grammar's pushes and replaces, linear parse time for rules and for selector and keyframe groups |
| `tests/debug_model.rs` | the grammar composed with `tabnas-debug`, the Rust half of `ts/test/debug-model.test.ts` |
| `tests/memory.rs` | a counting allocator holding a parse's peak memory, flat and nested, under ceilings |
| `tests/reworkcss.rs` | the pinned reworkcss/css corpus, fetched by the test itself |
| `tests/perf.rs`, `tests/version.rs` | the instance-reuse guard and the version sites |
| `tests/support/mod.rs` | the crate's own fixture loader, JSON reader and canonical compare; `tabnas-support` is not a dependency |
| `doc/*.md`, `README.md` | the four Diátaxis pages and the crate front page, all gated by the prose gate |
| `examples/parse.rs` | the binary the TS/Rust differential probe drives |

## A plugin on the engine

The crate is a tabnas plugin, as the TypeScript and Go ports are: it runs
on the engine `tabnas-parser` (imported as `tabnas`), layered on
`tabnas-jsonic`, which reads `css-grammar.jsonic` and supplies the fixed
tokens. `install` in `src/plugin.rs` puts the canonical option overrides
(`OPTIONS_DOC`), the 13 rules (each alternate tagged `css`), the
`cssToken` matcher at order 1e5, the actions, a `parse.prepare` hook, a
lex subscriber and the `tabnas-css/depth` guard on an engine. The engine implements
the whole alternate surface, so a field the grammar grows needs no
teaching here; a new ACTION does, in `register_actions`.

The dependencies are sibling checkouts, the standard tabnas development
model: clone `parser`, `json`, `jsonic` and `debug` from
`https://github.com/tabnas/` next to this repository. `rs/Cargo.toml`
takes the engine and jsonic by path (jsonic takes json by path in turn),
and `tabnas-debug` as a dev-dependency the same way. Dependencies change
only on explicit instruction from the maintainer (`../CLAUDE.md`); the
engine switch was one.

## Rules this port keeps, and why

Each is behaviour. A change to any of them shows up in a parse result, a
memory profile or a stack overflow.

- **The installed rules are the canonical port's, rule for rule.** The
  13 rules and every alternate (order, token slots, `b`, `p`, `r`,
  actions, groups) are what `css-grammar.jsonic` gives the TypeScript and
  Go ports, and no rule carries a lifecycle action (`bo`, `ao`, `bc`,
  `ac`). Work only Rust needs runs inside an alternate's action instead:
  the arena hand-back is in `@cssEnd`, on the stylesheet's close.
  `tests/grammar.rs` pins the absence of lifecycle actions and the `css`
  group on every alternate; `tests/repeat.rs` pins the pushes and
  replaces.
- **A node constructor installs a FRESH cell.** The engine shares one
  node cell between a rule and the rules it pushes or replaces into, so
  `@cssSheet`, `@cssRule`, `@cssDecl` and the other constructors set
  `rule.node = Rc::new(RefCell::new(..))` (`install_node`). Writing the
  new node into the cell a rule was handed overwrites its parent's node.
  Setters and pushers write through the shared cell (`with_node`), as the
  canonical ones mutate the shared object.
- **`Css::parse` builds in the arena, and nothing recursive is reachable
  from its result.** It passes `arena_meta()`, one object the plugin
  recognises by its address, so a caller's own meta cannot select the
  arena or lift the tree form's bound; every node is a flat
  record in a per-parse list in `ctx.u`, so the engine never holds a
  nested value, and `Value::from_arena` builds the tree walking the
  records from last to first, without recursion. `Value` and `Node` drop,
  clone, compare, print and write JSON with explicit stacks;
  `tests/css.rs` pins all of it at 20,000 levels, on the `Value` and on
  the `Node`, on a 2 MiB thread too.
  `Value::from_engine` recurses and is only for the grammar document:
  never route a parse result through it. `Debug` is the walk that is
  easy to forget, because it is what a caller reaches for while
  debugging.
- **The tree form is bounded by the `tabnas-css/depth` guard.** A parse
  through the engine's own API (`make`, `plugin`, `Css::tabnas`) builds
  engine objects, which drop, clone and print by recursion. The guard has
  a name of its own, since jsonic and the grammars on it each install
  theirs as `depth`, replacing the last; it refuses more than
  `TREE_RULE_DEPTH` (768) open rules with `cancel` unless the parse is an
  arena parse. The install removes jsonic's own `depth` guard, which
  counts `map` and `list` rules the css options exclude, so it never
  refused a css parse and cost about 1% of every one. Row 5 of `../test/divergent.tsv` registers the bound;
  moving it means moving the constant, `tests/plugin.rs` (191 nested
  rules and 256 nested `@media` at the bound) and that row together. The
  repair is upstream: when the engine's value is iterative, the bound
  and the row go.
- **UTF-16 columns and the end-of-input overshoot are the plugin's job.**
  The engine counts columns in Unicode scalars. Positions go through
  `col16`, a binary search over a sorted list of the source's astral
  characters built on the first position; errors go through
  `From<TabnasError> for Error`. A byte count is wrong for `#©{…}` and a
  scalar count for `#𝄞{…}`. The scanners may step one past the end
  (`@host\`): `css_token` records the overshoot under `OVERSHOOT` in
  `ctx.u` and the lex subscriber adds it to the end-of-source token's
  column, so the `host` node ends at 7 and the stylesheet at 8.
- **`str::trim` is not `String.prototype.trim`.** Use `es_trim` and
  `es_is_whitespace` from `lex.rs` at every site that produces AST text.
  The two sets differ by U+FEFF and U+0085, and both change the tree; see
  the divergence register.
- **A bad token fetched behind a good one is the engine's to report.**
  The canonical engine throws a bad token when it is fetched, and records
  and skips it under recovery; since tabnas/parser#274 the Rust engine
  does the same. Before that it buffered the token, an error with no
  alternative took its code from the first lookahead token only, and the
  lex subscriber cleared the unconsumed lookahead when a bad token
  arrived behind a good one (with recovery and relexing off) so that
  `a{b\"x;"/*` failed as `unterminated_comment`. That clearing is gone;
  do not bring it back. `../test/spec/comments.tsv` pins the fail-fast
  rows, and `tests/plugin.rs` pins recovery (`unterminated_comment` at
  1:5, the canonical port's first error) and relexing (the good token is
  reported, as the canonical engine reports it).
- **The matcher emits the grammar's own tokens by name.** `#CC`, `#GC`
  and the at-rule tokens carry `lex::BY_NAME` (-1), which the engine
  resolves as it lexes. Never capture a token number at install:
  `Tabnas::merge` renumbers custom tokens without running the plugin, and
  a captured number names another token there. `tests/plugin.rs` merges
  both ways.
- **A child that built no node is not pushed.** `push_child` skips a
  child whose `child_node` is undefined: it failed before its
  constructor ran (only recovery gets past that) and still shares the
  parent's cell, where `child_value()` answers with the parent's own node.
  The canonical port pushes the parent into itself there, a cycle.
- **A selector group is scanned once.** `BraceScans` keeps the first
  item's `{`-before-`;` answer with a cursor that moves forward, and
  gives it to a later item's start the cursor reaches at bracket depth 0;
  any other start scans afresh. The scan's whole state is its position
  and its depth, so the answer is exact; without the cache a group's
  items each scanned to its `{`, and 200 KB of selectors took 9 s. An
  unclosed comment's answer is kept as well: the engine's recovery asks
  again at the start that failed, up to its skip budget. The state is
  five numbers under one `ctx.u` key, updated in place, since the matcher
  asks on every `#TX`: a fresh key and list per token cost a flat
  stylesheet 5 to 10% of its parse. `tests/repeat.rs` pins linear time
  for both kinds of group, and the recovery rows with a stray `(` or `[`
  in `tests/plugin.rs` pin the depth-0 check.
- **Lookahead is lazy, and comment nodes are keyed on the rule name.**
  The engine reads a token only when the alternate being tried needs one,
  under the rule trying it; `COMMENT_NODE_RULES` in `lex.rs` names the
  rules at which the matcher makes a comment a node. A comment right
  after `{` is a node because the block wrapper's `#OB #CB` alternate
  reads the first body token under the wrapper's name, and
  `b{,/*!important` fails as `unexpected` because `decl`'s alternates
  fail on the first token before the comment behind it is read.
- **Per-parse state lives in `ctx.u` and `@css-prepare` clears it.** The
  arena, the astral list, the overshoot and the group scan sit under
  `tabnas-css/…` keys, and the prepare hook removes them before every
  parse, so a caller's seeded context cannot reach them
  (`tests/plugin.rs` seeds each; the arena is read only by `Css::parse`,
  which takes no seed, so its clear is a guard the test cannot observe).
  A new key goes in the hook too.
- **A second install applies its options.** `use_plugin` again, or
  `derive`, reruns `install`: the matcher and the actions (which capture
  the options) are re-registered by name, and the rules in `RULES` are
  removed and installed afresh, since installing over an existing rule
  puts the new alternates in front of the old (`tests/plugin.rs` compares
  every rule's alternate counts with a fresh instance's). The lex subscriber is
  added by the first install only, because subscribers are not named. A
  rule added to the grammar goes into `RULES`.
- **`rule.history` is 1.** Rust only; the canonical options do not set
  it. Unbounded, every item of a list stays reachable until the list
  closes: 100,000 flat rules peaked at 927 MiB, and bounded at 210 MiB.
  No alternate reads `prev`; one that must would reopen this, and
  `tests/memory.rs` holds the ceilings.
- **Every repetition is a replace loop, never a push chain.** The loop
  is `r:`, the item may be `p:`; push is for structure (a block's body, a
  list's first item, an item's parts). The five loops are `items`,
  `decls` and `kfitems`, which replace themselves in their close, and
  `sel` and `kfsel`, in their open. `tests/repeat.rs` is the proof: over
  10,000 items each repetition reaches the depth one item needs, nesting
  still grows it (four per style rule, three per `@media`), the installed
  grammar's pushes and replaces are exactly the listed ones, and ten
  times the rules parse in about ten times the time. The grammar is
  shared with TypeScript and Go; write any new repetition in
  `css-grammar.jsonic` the same way, and add a row for it to
  `repetitions()`.
- **The engine's debug self-check stays off.**
  `[profile.dev.package.tabnas-parser] debug-assertions = false` in
  `Cargo.toml`: the check compares the whole rule stack with a shadow
  copy on every step, quadratic in depth (1,000 nested rules 8.3 s, 4,000
  134 s in a debug build), and `cargo test` parses 20,000 levels. A
  profile applies only to the root package, which is why the README's
  "Untrusted input" tells consumers to set it too.

## Gates

```bash
ci/rust/run.sh     # the full gate, from the repository root; `make gate-rs`
make test-rs       # the fast loop: tests, doctests and clippy
```

`ci/rust/run.sh` checks the sibling checkouts, runs through the MSRV
toolchain (1.85, from `rust-version`; it warns when that is not
installed), checks the lock's entry for this crate against the
manifest, then runs `cargo fmt --check` (not `--all`, which reaches into
the siblings), `build --all-targets`, `test --all-targets`, `test --doc`
(`--all-targets` does not include doctests), clippy with `-D warnings`
and `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`. Last, it diffs the
whole lock against the committed one with the sibling versions masked,
and puts the lock back. It does NOT pass `--locked`, deliberately: the
siblings are resolved from their main branches, and a sibling's version
bump would fail every pull request here.

`cargo test` fetches the reworkcss corpus for itself (`fetch_corpus()` in
`tests/reworkcss.rs`) and **fails** rather than skipping if it is still
absent afterwards.

The differential probe is a gate too, and is not part of `cargo test`,
because it needs the TypeScript port built:

```bash
bash ../scripts/divergence-probe-rs.sh 4000    # or `make probe-rs`
```

It runs the corpus once per option combination, all four of them, and asks
cargo for the `parse` example it just built rather than deriving a path, so
a shared target directory or a configured target triple does not break it.
It also refuses a combination whose output matches the default mode: an
option the corpus never reaches is a run that reports coverage it does not
have. It restores `rs/Cargo.lock` after its build.

**`.github/workflows/rust.yml` runs all of this on every push to `main`
and every pull request.** Two jobs: `rust` checks the repository out into `css/`,
clones the four siblings beside it, installs Rust 1.85 with `rustup` and
runs `css/ci/rust/run.sh`; `probe` builds the TypeScript port and runs
`scripts/divergence-probe-rs.sh 4000` against the same siblings. Run the
gate locally before pushing anything that touches `rs/`, the grammar, or
a fixture: the workflow is the gate, not the first place to find out.

## Versions and releasing

`version` in `Cargo.toml` and `VERSION` in `src/lib.rs` must both equal
`ts/package.json` `"version"`; `tests/version.rs` reads both and fails the
build if either drifts. The release orchestrator rewrites the TypeScript and
Go sites and **does not know about these two**. `make version-rs V=x.y.z`
bumps both and this crate's own entry in `Cargo.lock`, which the gate
checks before anything else runs.

`Cargo.lock` is committed. The gate holds it to the resolution cargo
makes, sibling versions aside, so a change to `Cargo.toml` commits the
updated lock with it.

crates.io has `tabnas-css` 0.5.9: the earlier crate, with its own lexer
and rule machine and no dependencies. The published pages (README and
`doc/`) call it a different implementation and carry no history, since
they ship in the package. `.github/workflows/crates-release.yml`
publishes `rs/` from the release tag on every release, rewriting each sibling path dependency into a requirement on that
crate's newest version on crates.io, where `tabnas-parser`,
`tabnas-jsonic` and `tabnas-json` are published. The engine-based crate
removes `mod machine`, `Css::grammar`, `grammar::{Grammar, Alt,
RuleDef}` and `lex::{Token, Lex, Point, start_pos, end_pos}`, which is
breaking for a Rust caller of 0.5.9; the version moves in lockstep with
`ts/package.json`, and when to publish is the maintainer's decision. A
local `cargo publish` is not the release path.

Until then a consumer takes the crate from git, with a `[patch]` table
per repository whose manifest names siblings by path (the README's
Install section); `aless`'s `Cargo.toml` uses the same pattern for every
tabnas crate it takes from git.
