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
| `media` / `supports` / `document` / `host` | a prelude field (`media`, `supports`, `document`; `host` has none), `rules: Node[]` |
| `font-face` / `page` | `declarations: Node[]` (`page` also `selectors: string[]`) |
| `keyframes` | `name`, optional `vendor` (e.g. `-webkit-`), `keyframes: Node[]` of `keyframe` `{ values: string[], declarations: Node[] }` |
| `import` / `charset` / `namespace` (statement at-rules) | a same-named field with the raw params |

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
| [`ts/`](ts/) | **Canonical** TypeScript implementation — the `@tabnas/css` package (`0.1.0`). Plugin in `src/css.ts`. Peer-depends on `@tabnas/jsonic` and `@tabnas/parser`. No CLI. |
| [`go/`](go/) | Go port — `github.com/tabnas/css/go` (`const Version` in `go/css.go`). Plugin `Css` plus `MakeJsonic` / `Parse`. Depends on `github.com/tabnas/jsonic/go`. |
| [`css-grammar.jsonic`](css-grammar.jsonic) | **Single source of truth** for the grammar rules, authored in jsonic syntax. |
| [`ts/embed-grammar.js`](ts/embed-grammar.js) | Embeds `css-grammar.jsonic` into **both** `src/css.ts` and `go/css.go` (between `BEGIN/END EMBEDDED` markers). Runs first in `npm run build`. |
| [`ts/test/`](ts/test/) | TS tests (compiled to `dist-test/`): `css.test.ts` (AST parse cases), `debug-model.test.ts` (`@tabnas/debug` composition / model), `doc-examples.test.ts` (`// =>` assertions in README/doc fences), `perf.test.ts` (instance-reuse guard). |
| [`go/css_test.go`](go/css_test.go), [`go/perf_test.go`](go/perf_test.go) | Go suite — the same AST parse cases as `css.test.ts`, hand-mirrored, plus the perf guard. |
| [`ts/doc/grammar.svg`](ts/doc/grammar.svg), [`ts/doc/grammar.txt`](ts/doc/grammar.txt) | Railroad / ASCII diagram of the live grammar, generated by `@tabnas/railroad`. |
| [`ts/doc/`](ts/doc/), [`go/doc/`](go/doc/) | Per-runtime 4-quadrant Diataxis docs. |

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
   contract is the shared grammar plus the hand-mirrored case sets in
   `ts/test/css.test.ts` and `go/css_test.go`. Add/change cases in both.
4. The jsonic option overrides and the `cssToken` matcher exist in **both**
   runtimes and must stay in step (they live on the grammar object so the
   plugin applies them atomically with its rule alts).
5. `Defaults` (`lowercaseProperties: false`) and `Version` in `go/css.go`
   mirror the TS `Css.defaults` and the `package.json` version.

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
  lookahead scanners (which skip strings, `()`/`[]`, comments). Values are not
  parsed further; selectors are verbatim except a top-level group is split into
  the `selectors` list (commas inside `:not(...)` are not split).
- **Statement at-rules need a terminating `;`** (or end-of-input / `}`).
- **A zero-length source returns `undefined`/`nil`** (the engine's max-iteration
  budget scales with source length). Any non-empty source — even whitespace or
  a comment — yields a `stylesheet` node.
- **CSS Nesting is not supported** — a style rule's block is declarations (and
  comments) only; at-rules / rules nested inside it are out of scope.

## Build & test

TypeScript (from `ts/`):

```bash
npm install
npm run build          # node embed-grammar.js && tsc --build src test
npm test               # node --enable-source-maps --test "dist-test/*.test.js"
```

`npm run build` embeds the grammar first (into `src/css.ts` and `go/css.go`),
then compiles `src` and `test`. The diagram is regenerated with
`@tabnas/railroad` off the live config.

Go (from `go/`):

```bash
go build ./...
go test -v ./...       # AST parse cases (mirrors css.test.ts)
```

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

`.github/workflows/build.yml` has a **build** job (Ubuntu/Windows/macOS,
Node 24) that clones the tabnas closure as siblings, builds each, then runs
`npm test` here (the composition test runs because `@tabnas/debug` is a
devDependency), and a **build-go** job (Ubuntu/macOS, Go 1.24) that sets up a
`go work` over the modules and runs `go build` / `go test -v`.
