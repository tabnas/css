# Agents Guide — css

## What this project is

`@tabnas/css` is a **grammar plugin** that parses
[CSS](https://developer.mozilla.org/en-US/docs/Web/CSS) into a faithful
**abstract syntax tree** — the widely-used
[`reworkcss/css`](https://github.com/reworkcss/css) model: ordered, typed
nodes that preserve declaration order, duplicate properties, rule types and
comments.

```js
c.parse('a { color: red; color: blue } /* note */')
```

→

```js
{ type: 'stylesheet', rules: [
  { type: 'rule', selectors: ['a'], declarations: [
    { type: 'declaration', property: 'color', value: 'red' },
    { type: 'declaration', property: 'color', value: 'blue' } ] },
  { type: 'comment', comment: ' note ' } ] }
```

It is a **jsonic plugin**: it layers on `@tabnas/jsonic`, reuses its fixed
punctuation tokens (`{` `}` `:`), turns off the relaxed-JSON value matchers,
and supplies its own grammar that builds the AST. Install on a jsonic engine —
`new Tabnas().use(jsonic).use(Css)` (TS) / `jsonic.Make()` then
`UseDefaults(Css, ...)` (Go).

### Node types (the output contract)

| `type` | Fields |
|---|---|
| `stylesheet` | `rules: Node[]` |
| `rule` | `selectors: string[]`, `declarations: Node[]` |
| `declaration` | `property: string`, `value: string` (raw, trimmed, comments stripped, quotes kept) |
| `comment` | `comment: string` (text between `/*` `*/`) |
| `media` / `supports` / `document` / `host` | a prelude field (`media`, `supports`, `document`; `host` has none), `rules: Node[]`. `document` also always carries `vendor` (`''` when the at-keyword is unprefixed) |
| `font-face` / `page` | `declarations: Node[]` (`page` also `selectors: string[]`, its prelude split on top-level commas) |
| `keyframes` | `name`, optional `vendor` (e.g. `-webkit-`), `keyframes: Node[]` of `keyframe` `{ values: string[], declarations: Node[] }` |
| `import` / `charset` / `namespace` (statement at-rules) | a same-named field with the raw params |
| `custom-media` | `name`, `media` (the params split at the first whitespace after the `--name`) |

Block at-rules are classified by keyword: a **rules** body (`media`,
`supports`, `document`, `host`, and unknown block at-rules → `{ type: kw,
[kw]: prelude, rules }`), a **declarations** body (`font-face`, `page`,
`viewport`, `counter-style`, `property`, …), or **keyframes**. A leading `-`
vendor prefix is split into `vendor`.

## How the parse works

CSS is context-sensitive (the same characters can begin a selector, a property
or a value), so the **lexer** owns the hard tokenisation and the **grammar**
assembles typed nodes.

The single `cssToken` matcher emits, by position:

- `#TX` — one selector (up to a top-level `,` or `{`) or a property name (up to
  `:`), chosen by a `{`-before-`;` lookahead. Selectors/values have comments
  stripped and whitespace trimmed.
- `#GC` — a top-level selector-group comma (so `h1, h2` is two `#TX` keys).
- `#VL` — a declaration value, read in the `declval` rule up to the next
  top-level `;`/`}`.
- `#CC` — a comment **node**, emitted only when the active rule is a list
  reader / block wrapper (`items`/`decls`/`kfitems`/`declbody`/`rulesbody`/
  `kfbody`). Elsewhere a comment is deferred to the builtin comment matcher and
  skipped (so mid-construct comments, e.g. between a property and its `:`, are
  dropped). **`#CC`, not `#CM`** — `#CM` is the engine's builtin comment tin
  (tin 7), which the parser ignores; a custom name is required.
- `#ATR` / `#ATD` / `#ATK` / `#ATS` — at-rules, classified by keyword and
  block-vs-statement lookahead. The keyword is the token `val`; the prelude /
  params ride in `tkn.use`. (`#ATR` = rules body, `#ATD` = declarations body,
  `#ATK` = keyframes, `#ATS` = statement at-rule.)

The grammar rules build the AST with grammar-local **actions** (named
`@cssXxx`, never `@xxx$` — `$` is reserved for engine builtins):

- node constructors `@cssSheet` / `@cssRule` / `@cssDecl` / `@cssComment` /
  `@cssKeyframe` / `@cssAtRules` / `@cssAtDecls` / `@cssKeyframes` /
  `@cssAtStmt` overwrite `r.node` with a fresh typed node;
- field setters `@cssSelector` / `@cssKfValue` / `@cssDeclVal` mutate it;
- array pushers `@cssPushRule` / `@cssPushDecl` / `@cssPushKf` append a built
  child node to the parent's array.

Rule shape: `stylesheet` → `items` (a statement-list loop) → `statement`
(one rule / at-rule / comment) → for a style rule, `sel` (selector list) +
`declbody` → `decls` → `decl` → `declval`. Block at-rules push `rulesbody`
(→ `items`), `declbody`, or `kfbody` (→ `kfitems` → `keyframe` → `kfsel`).
A node-building child rule **inherits** its parent's node; the array pushers
write the just-built child into the parent's `rules`/`declarations`/`keyframes`
array.

## Repository map

| Path | What it is |
|---|---|
| [`ts/`](ts/) | **Canonical** TypeScript implementation — the `@tabnas/css` package (version in `ts/package.json`). Plugin in `src/css.ts`. Peer-depends on `@tabnas/jsonic` and `@tabnas/parser`. No CLI. |
| [`go/`](go/) | Go port — `github.com/tabnas/css/go` (`const VERSION` in `go/css.go`). Plugin `Css` plus `MakeJsonic` / `Parse`. Depends on `github.com/tabnas/jsonic/go`. |
| [`css-grammar.jsonic`](css-grammar.jsonic) | **Single source of truth** for the grammar rules, authored in jsonic syntax. |
| [`ts/embed-grammar.js`](ts/embed-grammar.js) | Embeds `css-grammar.jsonic` into **both** `src/css.ts` and `go/css.go` (between `BEGIN/END EMBEDDED` markers). Runs first in `npm run build`. |
| [`ts/test/`](ts/test/) | TS tests (compiled to `dist-test/`): `css.test.ts` (AST parse cases), `parity.test.ts` (the shared `test/spec/*.tsv` fixtures), `reworkcss.test.ts` (the external conformance corpus), `debug-model.test.ts` (`@tabnas/debug` composition / model), `doc-examples.test.ts` (`// =>` assertions in README/doc fences), `perf.test.ts` (instance-reuse guard), `version.test.ts` (exported `VERSION` vs `package.json`). |
| [`test/spec/`](test/spec/) | Shared `.tsv` conformance fixtures, auto-discovered and run by **both** runtimes. See [`test/AGENTS.md`](test/AGENTS.md). |
| [`go/css_test.go`](go/css_test.go), [`go/perf_test.go`](go/perf_test.go), [`go/parity_test.go`](go/parity_test.go), [`go/reworkcss_test.go`](go/reworkcss_test.go), [`go/version_test.go`](go/version_test.go) | Go suite — the remaining in-language AST cases, the perf guard, the shared-fixture runner, the external conformance corpus runner, and the `VERSION` vs `ts/package.json` drift check. |
| [`ts/doc/grammar.svg`](ts/doc/grammar.svg), [`ts/doc/grammar.txt`](ts/doc/grammar.txt) | Railroad / ASCII diagram of the live grammar, generated by `@tabnas/railroad`. |
| [`ts/doc/`](ts/doc/), [`go/doc/`](go/doc/) | Per-runtime 4-quadrant Diataxis docs. |
| [`scripts/fetch-reworkcss-tests.sh`](scripts/fetch-reworkcss-tests.sh) | Fetches the pinned third-party reworkcss/css conformance corpus into `test/reworkcss-css/` (gitignored, never committed). Also `npm run install-reworkcss-tests` from `ts/`. |
| [`ts/test/reworkcss.test.ts`](ts/test/reworkcss.test.ts), [`go/reworkcss_test.go`](go/reworkcss_test.go) | The conformance runners over that corpus — see **Conformance** below. |

## Conformance

**The bar: the reworkcss/css AST model, measured against the reworkcss/css
corpus pinned at `ae6a6f9bf939cbcbc759a12d9f208afb5d4dde75` — all 46
`test/cases/*` ASTs (with AND without source positions) and all 6 non-`silent`
accept/reject assertions in its `test/parse.js`.**

That corpus is the *authoritative expected value* for a parse, not merely an
accept/reject oracle: the runners compare the whole tree. Measured status:
**45/46 cases pass** on both metrics in both runtimes; **6/6** accept/reject
assertions hold in both runtimes.

There are no known divergences. `cases/empty` used to be one and is still
asserted explicitly (never skipped) in both runners so it cannot silently
regress:

- **`cases/empty`** — a zero-length source used to yield `undefined`/`nil`.
  The cause was NOT the rule-iteration budget, as previously documented here:
  the engine short-circuits `''` and returns `lex.emptyResult` before the
  rule loop is reachable (`parser.ts` / `parser.go` `Start`). The real cause
  was that this plugin declared no `emptyResult`. It now declares
  `{ type: 'stylesheet', rules: [] }` in `ts/src/css.ts` and `go/css.go`, so
  `''` matches reworkcss, as does any non-empty source.

What this plugin does **not** claim, deliberately:

- **CSS Syntax Level 3 error recovery.** The reworkcss model *rejects* inputs
  L3 recovers from (an unclosed comment, a missing selector, an unclosed
  block), and this plugin follows reworkcss. A suite such as
  `SimonSapin/css-parsing-tests` therefore measures a different contract and
  is not the bar here.
- **`reworkcss`'s `{ silent: true }` error-recovery API** (a parse that
  collects `parsingErrors` and continues). There is no equivalent option.
- **`parent` / `source` / `position.content` fields.** Nodes are plain data;
  upstream's back-references and filename bookkeeping are not reproduced.

The behaviours the corpus exercises are additionally pinned offline, in both
runtimes, by [`test/spec/reworkcss.tsv`](test/spec/reworkcss.tsv) — those
fixtures run whether or not the corpus has been fetched, and every expected
value in them was generated from the upstream parser itself.

## Authority and alignment rules

1. **TypeScript is canonical.** When TS and Go disagree, TS wins; change Go.
2. **The grammar is single-sourced.** `css-grammar.jsonic` is authored once;
   `embed-grammar.js` copies it verbatim into the `grammarText` literal in both
   `src/css.ts` and `go/css.go`. **Never hand-edit between the
   `--- BEGIN/END EMBEDDED css-grammar.jsonic ---` markers** — edit the
   `.jsonic` and re-run `npm run embed` (or `npm run build`). The Go embed
   rejects backticks (Go raw strings), so the grammar comments use plain
   quotes, never backticks.
3. The two ports must produce the same AST for the same input. The parity
   contract is the shared grammar plus the shared `test/spec/*.tsv`
   fixtures, which both runtimes auto-discover. Add or change a parse case
   there; the in-language suites keep only what a fixture cannot express.
4. The jsonic option overrides and the `cssToken` matcher exist in **both**
   runtimes and must stay in step (they live on the grammar object so the
   plugin applies them atomically with its rule alts).
5. `Defaults` (`lowercaseProperties: false`, `position: false`) and `VERSION`
   in `go/css.go` mirror the TS `Css.defaults` and the `VERSION` exported from
   `ts/src/css.ts`. Both `VERSION` constants MUST equal `ts/package.json`
   "version"; `go/version_test.go` and `ts/test/version.test.ts` fail the
   build if either drifts. Never bump one by hand — the release orchestrator
   (`admin/publish.sh`) rewrites all three together.

## Repo-specific gotchas

- **`#CC`, not `#CM`, for comment nodes.** `#CM` resolves to the builtin
  comment tin (7), which is in the parser's IGNORE set — emitting it silently
  drops the node. Likewise the at-rule tokens use fresh names `#ATR/#ATD/#ATK/
  #ATS` and the group comma `#GC`.
- **Custom action refs may not contain `$`** (`$` is reserved for engine
  builtins). All grammar-local actions are named `@cssXxx`.
- **Go must resolve every custom token tin** via `j.Token("#CC")` etc. and pass
  them to the matcher (an external Go package can't auto-tokenise like the TS
  `lex.token('#CC', …)` does). The Go `buildGrammarAlts` also handles an
  **array** `a:` action field (e.g. `['@reset$' '@cssX']`), not just a string.
- **Comments are nodes only at list positions.** The matcher checks the active
  rule name against `COMMENT_NODE_RULES`. The block wrappers
  (`declbody`/`rulesbody`/`kfbody`) are included because their empty-block
  `#OB #CB` lookahead lexes the first body token — a comment right after `{`
  is captured there. The item *builders* (`statement`/`decl`/`keyframe`) are
  NOT in the set; they reuse the cached `#CC` the list reader produced, so a
  comment seen mid-construct (under a builder) is skipped.
- **Declaration values and selectors are raw strings** (trimmed, comments
  stripped), read by the `scanValueEnd` / `scanSelectorEnd` / `scanToBraceOrEnd`
  lookahead scanners (which skip strings, `()`/`[]`, comments, and a `\`
  escape and the character after it). Values are not parsed further; selectors
  are verbatim except a top-level group is split into the `selectors` list
  (commas inside `:not(...)` are not split). The escape skip matters: without
  it a selector such as `#f\'o\'o` or `.\3A \`\(` reads as an unterminated
  string / unbalanced paren and the whole rule fails to lex.
- **At-rule preludes and params keep their comments** (they are only trimmed),
  matching reworkcss — `@media screen /*x*/ {` has media `screen /*x*/`.
  Selectors, values and property names *do* have comments stripped.
- **Property names are not identifiers.** `isPropChar` also admits `*`, `#`,
  `/` and `\` (the `*prop` / `#prop` / `//prop` IE hacks), and `scanPropEnd`
  admits a trailing `[0-9a-z_-]+` bracket suffix (`opacity[sqrt]`) — the
  reworkcss pattern `\*?[-#\/\*\\\w]+(\[[0-9a-z_-]+\])?`. Because `/` and `*`
  are property characters, a trailing hack comment (`color/**/:`) lands inside
  the scanned name, so the name is run through `stripComments` afterwards.
- **An unclosed `/* ... */` is an error, not a comment to EOF** (`lex.bad` /
  `lex.Bad` with `unterminated_comment`), matching both the engine's builtin
  comment matcher and reworkcss.
- **Statement at-rules need a terminating `;`** (or end-of-input / `}`).
- **A zero-length source returns an empty `stylesheet`**, declared via
  `lex.emptyResult` (the engine returns that for `''` before any rule runs).
  Any non-empty source — even whitespace or a comment — also yields a
  `stylesheet` node.
- **CSS Nesting is supported** structurally in the grammar. The `decl` rule has
  alts for a nested style rule (`#TX` then `{`/`,` → `@cssRule` → `sel`) and
  nested at-rules (`#ATR/#ATD/#ATK/#ATS`), alongside the `#TX #CL` declaration
  alt. Nested nodes land in the parent rule's `declarations`, in source order.
  The disambiguation lives in the grammar (token after the key), not the lexer.
- **Source positions are opt-in** via the `position` option (default off). When
  on, `makeActions` records `node.position.start` from the constructor's open
  token and `end` from a close action (`@cssEnd` reads the close-phase token
  `r.c[0]` / Go `r.C0`; `@cssDeclVal` sets a declaration's `end`). The `advance`
  lexer helper tracks newlines so `Point.rI/cI` and emitted token `rI/cI` stay
  1-based and correct; `startPos`/`endPos` derive the {line,column} pairs. Keep
  the TS and Go position logic in lockstep. **Columns are UTF-16 code units**
  (what a JavaScript string index counts), so the Go port measures spans with
  `colWidth`, not `len()` and not `utf8.RuneCountInString` — a byte count is
  wrong for `#©{…}` and a rune count is wrong for astral characters such as
  `#𝄞{…}`. `test/spec/reworkcss.tsv` pins this in both runtimes.

## Build & test

TypeScript (from `ts/`):

```bash
npm install
npm run install-reworkcss-tests   # fetch the pinned conformance corpus (once)
npm run build          # node embed-grammar.js && tsc --build src test
npm test               # node --enable-source-maps --test "dist-test/*.test.js"
```

Without the corpus the two conformance suites *skip*, which means the
conformance claim above is unverified — a skipped conformance suite is not a
passing one. `npm run reset` fetches it as part of the cycle.

`npm run build` embeds the grammar first (into `src/css.ts` and `go/css.go`),
then compiles `src` and `test`. The diagram is regenerated with
`@tabnas/railroad` off the live config.

Go (from `go/`):

```bash
go build ./...
go test -v ./...       # AST parse cases (mirrors css.test.ts) + conformance
```

The Go conformance runner reads the same `test/reworkcss-css/` corpus, so
fetch it with `scripts/fetch-reworkcss-tests.sh` first (otherwise
`TestReworkcssCases` / `TestReworkcssAcceptReject` skip).

The repo-root [`Makefile`](Makefile) wraps both halves
(`make build|test|clean`, `make reset`, `make publish-go V=x.y.z`,
`make publish-ts`).

## Composition test (@tabnas/debug)

`ts/test/debug-model.test.ts` proves the plugin composes with
[`@tabnas/debug`](https://github.com/tabnas/debug) (a `file:` devDependency,
skipped when absent). It asserts the AST rule set is present
(`stylesheet`/`items`/`statement`/`sel`/`declbody`/`decls`/`decl`),
`config.start === 'stylesheet'`, `Css` in `plugins`, and the push/replace edges
(stylesheet→items, items→statement and self-replace, statement→sel/bodies,
decls self-replace), and that the model JSON round-trips. There is no Go
equivalent; the Go suite is self-contained.

## CI

`.github/workflows/ci.yml` calls the org-standard reusable workflow
`tabnas/.github/.github/workflows/polyglot-ci.yml` with
`deps: "parser debug json abnf railroad jsonic"` — it clones that closure as
siblings, builds each, then runs `npm test` here (the composition test runs
because `@tabnas/debug` is a devDependency) and `go build` / `go test` for the
Go module. The `pretest` npm script fetches the reworkcss corpus, so the **TS**
conformance suites really run in CI (verified on all three OSes) rather than
skipping; if the fetch fails it prints a warning and they skip (claim
unverified) rather than breaking the build.

The **go** job runs on a separate runner with no fetch step, so
`TestReworkcssCases` / `TestReworkcssAcceptReject` SKIP there — the Go
conformance number is verified locally (`make test` after a fetch), not by CI.
What CI does verify for Go is `test/spec/reworkcss.tsv`, the offline pins of
every behaviour the corpus exercises, which the shared-fixture runner executes
unconditionally. Do not add a network fetch to `go test` to close this;
fetching stays opt-in.

`.github/workflows/release.yml` publishes the npm package on a `ts/v*` tag via
OIDC trusted publishing. The workflow files cannot be edited from a session
credential — promotion goes through `tabnas/admin`.
