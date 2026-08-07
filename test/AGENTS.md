# Agents Guide — shared test corpora

Three corpora live under `test/`, all shared by both runtimes:

| Path | What | Runners |
|---|---|---|
| [`spec/`](spec/) | hand-written `.tsv` fixtures (below) | `ts/test/parity.test.ts`, `go/parity_test.go` |
| `reworkcss-css/` | **third-party, NOT committed** — `reworkcss/css` at pinned commit `ae6a6f9`. 46 documents with exact expected ASTs (the model README.md claims) plus must-throw / must-not-throw inputs in `test/parse.js`. Fetched by `scripts/fetch-reworkcss-tests.sh`. | `ts/test/reworkcss.test.ts`, `go/reworkcss_test.go` |
| `css-parsing-tests/` | **third-party, NOT committed** — CSS Syntax Level 3 corpus (`SimonSapin/css-parsing-tests`) at pinned commit `203ce36`. Fetched by `scripts/fetch-css-parsing-tests.sh`. | `ts/test/cssparsing.test.ts`, `go/cssparsing_test.go` |

## The corpora are never committed

Both third-party corpora are **gitignored** and fetched on demand at a pinned
commit SHA. Vendoring a corpus is forbidden: it cannot be verified against
upstream, and it is not this repository's to redistribute.

They also **must never silently skip**. `ts/` fetches them in the `pretest`
npm script; `go/` fetches them in `TestMain` (`go/conformance_test.go`); and
`.github/workflows/ci.yml` has an independent `conformance-corpora` job that
re-fetches, checks the census, and fails if either directory is ever tracked
by git. If a corpus is missing at test time the runners **fail loudly** with a
message naming the fetch script. A conformance test that quietly does not run
is worse than no test at all, because the green tick is a lie.

## What the third-party runners assert

Both directions, always:

- valid documents must parse **and produce the correct value** — the reworkcss
  runner compares the WHOLE tree (twice: positions stripped, and again with
  `position: true` against upstream's recorded line/columns); the
  css-parsing-tests runner compares a rule/declaration **kind sequence**
  derived mechanically from upstream's component-value trees, because this
  plugin does not emit component values and does not claim to;
- must-fail documents must be **rejected with an error**.

Neither runner has a skip list. `ts/test/cssparsing.test.ts` documents, at the
top, which upstream files are deliberately not wired and **why** — they test
entry points this plugin has no equivalent for (`one_rule`, `one_declaration`)
or token/value micro-syntaxes it explicitly keeps as raw text (`color_*`,
`An+B`, `component_value_list`). Those are reasoned exclusions, not a skip
list, and every case in every wired file runs.

As of the `conformance-2026-08` baseline both runners go loudly **RED**. Those
failures are real parser gaps. They must not be papered over by editing the
corpora, the derivation, the census constants or the assertions.

## Where the two authorities disagree

`reworkcss/css` and CSS Syntax L3 genuinely disagree about brace-first input:
Syntax L3 accepts `{ color: #aaa }` as a qualified rule with an empty prelude,
reworkcss throws "selector missing". This plugin follows reworkcss, so
`spec/leniency.tsv` pins ERROR for those, while `cssparsing.test.ts` records
the Syntax-L3 side as a (currently red) gap. Both are honest; neither is a
weakened test.

## `spec/*.tsv`

`spec/*.tsv` holds the cross-runtime conformance fixtures. Both runtimes
auto-discover and run **every** file in this directory, so a change here
affects TypeScript and Go together — edit with that in mind.

Two of them are baseline instruments rather than ordinary fixtures:

- `leniency.tsv` — relaxed-JSON documents that must be rejected, guarding
  against jsonic base leniency leaking through the plugin.
  `ts/test/leniency.test.ts` adds the stronger equivalence check (documented
  stack vs `new Tabnas().use(Css)` alone).
- `divergence.tsv` — minimal reproducers for inputs where TS and Go disagree,
  pinned to the TS behaviour per the "TypeScript is canonical" rule. **These
  rows are EXPECTED to fail in Go** until Go is brought into line; the red is
  the point. Reproduce the survey with `scripts/divergence-probe.sh`.

## Format

Tab-separated, one case per line, with a header row naming the columns.
Blank lines are skipped, and so are comment lines — a line starting with
`#` that contains no tab. (A data row always has at least one tab, so a
`#`-leading source such as a C preprocessor directive still works.)

| Column | Meaning |
|---|---|
| `input` | CSS source. Escapes `\n` `\r` `\t` `\\` are decoded. |
| `expected` | A JSON value (the parse result), or `ERROR` / `ERROR:<substring>` for inputs that must fail. |
| `opts` | Optional JSON object of plugin options (empty means defaults). |

`expected` and `opts` are **not** escape-decoded — they are raw JSON, so
JSON's own escape rules apply (`"a\nb"` is a string containing a newline).
To put a literal backslash in `input`, write `\\`.

Results are compared after a JSON round-trip, so key order and the
`OrderedMap` / null-prototype-object representations do not affect the
comparison.

## Who runs what

- TypeScript: `ts/test/parity.test.ts` — reads `../../test/spec` at runtime
  from `dist-test/`, one `describe` per file.
- Go: `go/parity_test.go` — `TestSpec` globs `../test/spec/*.tsv`.

Both discover files by directory listing: adding a `.tsv` here runs it in
both runtimes without touching either runner.

## Rules

- Prefer adding a fixture here over a one-off in-language assertion when a
  case is expressible as input → output. That is what keeps the two
  runtimes honest against each other.
- TypeScript is canonical. If the two runtimes disagree, the TS behaviour is
  the expected value — unless Go has exposed a genuine TS defect, in which
  case fix TS first and pin the corrected behaviour here.
- A new fixture must pass in BOTH runtimes: run `go test ./...` (from `go/`)
  and `npm test` (from `ts/`) before considering it done.
