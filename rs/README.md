# css (Rust)

A parser that reads [CSS](https://developer.mozilla.org/en-US/docs/Web/CSS)
into a faithful abstract syntax tree (the
[`reworkcss/css`](https://github.com/reworkcss/css) model): ordered, typed
nodes that preserve declaration order, duplicate properties, rule types, and
comments.

This is the Rust port of `@tabnas/css`. The TypeScript package is canonical,
the Go module tracks it, and so does this crate. All three are plugins on
the tabnas engine, read the same grammar file and are held to the same
shared conformance fixtures, so the same stylesheet gives the same tree in
every one.

Where a port does differ, the difference is a row of
[`../test/divergent.tsv`](../test/divergent.tsv) with a cell per runtime, and
each suite asserts its own cell. There are five rows. Four are Go against
TypeScript, and this crate follows TypeScript on every one, including the
column arithmetic it would be tidier to round off. The fifth is this crate's
own: the engine's tree form (see [the engine's own API](#the-engines-own-api))
refuses nesting deeper than `TREE_RULE_DEPTH` open rules, where TypeScript
and Go have no limit. `parse` and `Css::parse` have no limit either.

## Install

The crate is taken from the repository. The `tabnas-css` 0.5.9 on
crates.io is a different implementation: it has no dependencies, and none
of the plugin API this page describes.

Its own dependencies are named by relative path to sibling checkouts
(`path = "../../parser/rs"`). Inside a git source, cargo resolves such a
path within that same repository, where it does not exist, so a crate
taken from git needs a `[patch]` table for each repository that carries
one, pointing it at its own repository:

```toml
[dependencies]
tabnas-css = { git = "https://github.com/tabnas/css" }

[patch."https://github.com/tabnas/css"]
tabnas = { package = "tabnas-parser", git = "https://github.com/tabnas/parser" }
tabnas-jsonic = { git = "https://github.com/tabnas/jsonic" }

[patch."https://github.com/tabnas/jsonic"]
tabnas = { package = "tabnas-parser", git = "https://github.com/tabnas/parser" }
tabnas-json = { git = "https://github.com/tabnas/json" }

[patch."https://github.com/tabnas/json"]
tabnas = { package = "tabnas-parser", git = "https://github.com/tabnas/parser" }
```

Cargo finds the crate in `rs/`. Once the engine-based crate is published,
a version requirement replaces all of this.

The crate needs Rust 1.85 or later, the engine's own floor.

## One example

`parse` is the one-call entry point: pass source, get the AST and an error:

```rust
let ast = tabnas_css::parse("a { color: red }").unwrap();
assert_eq!(
    ast.to_json(),
    r#"{"type":"stylesheet","rules":[{"type":"rule","selectors":["a"],"declarations":[{"type":"declaration","property":"color","value":"red"}]}]}"#
);
```

Every node is a `Node`, an insertion-ordered map with a `type` key. Fields
such as `rules`, `declarations` and `selectors` are `Value::List`. Read them
with `Node::get` and the `Value` accessors, or call `Value::to_json` and hand
the text to whatever consumes it:

```rust
use tabnas_css::Value;

let ast = tabnas_css::parse("h1, h2 { margin: 0 }").unwrap();
let rule = ast.as_node().unwrap()
    .get("rules").and_then(Value::as_list).unwrap()[0]
    .as_node().unwrap();
assert_eq!(Some("rule"), rule.node_type());
```

`parse` reuses one process-wide parser and is safe to call from several
threads. Building a parser installs the grammar on a new engine, which costs
more than a parse does, so a hot loop that needs options should hold a `Css`
rather than call `parse_with` each time.

## Options

`Options` has two fields, both `false` by default:

- `lowercase_properties`. Normalise property names to lower case. Selectors
  and values are left untouched.
- `position`. Attach a `position` (1-based `start` and `end`, each a line and
  a column) to every node.

```rust
use tabnas_css::{Css, Options};

let css = Css::with_options(Options { position: true, ..Options::default() });
let ast = css.parse("a { color: red }").unwrap();
// Every node gains a position; the stylesheet's spans the source.
assert!(ast.to_json().ends_with(
    r#""position":{"start":{"line":1,"column":1},"end":{"line":1,"column":17}}}"#
));
```

Columns count UTF-16 code units, which is what a JavaScript string index
counts, so a column here is the column the canonical port reports for the
same source.

## CSS nesting

A style rule or an at-rule may appear inside another style rule's declaration
block. Nested nodes are appended to the parent's `declarations` in source
order, interleaved with declarations:

```rust
let ast = tabnas_css::parse("a { color: red; & b { top: 0 } }").unwrap();
// The rule's declarations: a declaration, then the nested rule.
assert_eq!(
    ast.to_json(),
    r#"{"type":"stylesheet","rules":[{"type":"rule","selectors":["a"],"declarations":[{"type":"declaration","property":"color","value":"red"},{"type":"rule","selectors":["& b"],"declarations":[{"type":"declaration","property":"top","value":"0"}]}]}]}"#
);
```

## A plugin on the tabnas engine

The TypeScript and Go ports install the grammar on the tabnas engine, layered
on the relaxed-JSON grammar jsonic, and this crate does the same in Rust: it
runs on `tabnas-parser` (imported as `tabnas`) and `tabnas-jsonic`, and
re-exports both, as `tabnas_css::tabnas` and `tabnas_css::tabnas_jsonic`, so
that a caller names their types without a second copy of either. jsonic
reads `css-grammar.jsonic`, and the plugin installs its rules with the
canonical option overrides and the `cssToken` lex matcher. Those overrides
switch jsonic's own rules and value matchers off, so `{a:1}` is refused here
as it is in the other two ports.

### The engine's own API

`plugin()` is the grammar as an engine plugin, `make()` builds a jsonic
engine with it installed, and `Css::tabnas()` is the engine a `Css` runs.
Their parse returns the engine's own `tabnas::Value`, the same tree as the
canonical port's plain objects, with these differences from `Css::parse`:

- the engine writes a whole number, such as a line in a `position`, as `1.0`
  in JSON;
- an engine error's column counts Unicode scalars, where this crate's
  `Error` counts UTF-16 code units (`tabnas_css::Error::from` converts one);
- the tree is bounded at `TREE_RULE_DEPTH` (768) open rules, and one level
  more fails with `cancel`;
- with the engine's recovery on, the partial stylesheet is not always the
  canonical port's (the [concepts page](doc/concepts.md#recovery) says
  where they part).

The [how-to guide](doc/guide.md#use-the-plugin-on-your-own-engine) has the
recipe, and the [reference](doc/reference.md#the-two-result-forms) the
details.

### Building from a checkout

The engine, jsonic, the strict-JSON grammar jsonic takes, and the debug
plugin the tests use are sibling checkouts, the standard tabnas development
model. Clone [`parser`](https://github.com/tabnas/parser),
[`json`](https://github.com/tabnas/json),
[`jsonic`](https://github.com/tabnas/jsonic) and
[`debug`](https://github.com/tabnas/debug) next to this repository, and
`cargo test` in `rs/` builds against them.

## Untrusted input

A parsed stylesheet is data, never instructions. CSS arrives from outside the
system, so treat every selector, value, and comment as hostile text. Parsing is
not sanitising: this crate returns the raw text the stylesheet contained, and
escaping it for HTML, SQL or a shell remains the caller's job. A `url(...)` in
a declaration value is untrusted text, not a link to fetch.

A parse holds memory, and takes time, in proportion to its input, and
nesting costs the most. Measured in a release build, 100,000 flat rules
(1.7 MB of CSS) peaked at 210 MiB, or 580 MiB with positions on, and
100,000 nested rules (0.6 MB) at 678 MiB, about 7 KiB per open level. The
densest nesting, `a{` repeated, holds about 2.8 KiB and takes about 6 µs
per byte of input. A host that parses untrusted CSS should cap the input's
size.

The engine's `parse.recover` and `lex.relex` modes are the exception to
that proportion: in the engine this crate builds on, both take time
quadratic in the input, and a valid 16 KB stylesheet takes about 16 s with
recovery on. `parse` and `Css::parse` use neither.

`parse` and `Css::parse` have no depth limit: 20,000 nested rules parse, and
the result drops, clones, compares and prints without recursion, on a 2 MiB
thread. The engine's tree form stops at `TREE_RULE_DEPTH` open rules, as
described above.

When debug assertions are on, the engine checks its whole rule stack against
a shadow copy on every step, which makes a debug build's parse quadratic in
nesting depth. Measured in a debug build, 1,000 nested rules took 8.3 s,
2,000 took 32.9 s and 4,000 took 134 s. This crate's manifest turns the
check off for its own builds, but a profile setting applies only to the
root package, so a crate that parses untrusted, possibly deep CSS in its own
debug builds sets the same key in its own manifest:

```toml
[profile.dev.package.tabnas-parser]
debug-assertions = false
```

## Documentation

Full documentation follows the [Diátaxis](https://diataxis.fr) framework:

- [Tutorial](doc/tutorial.md). A guided first parse, start to finish.
- [How-to guide](doc/guide.md). Short recipes for individual tasks.
- [Reference](doc/reference.md). The public API, every option, and the
  complete AST node reference.
- [Concepts](doc/concepts.md). How the parser is built, and how the Rust
  port differs from TypeScript.

For the canonical TypeScript implementation, see
[`../ts/README.md`](../ts/README.md). For the Go port, see
[`../go/README.md`](../go/README.md).

## License

MIT.
