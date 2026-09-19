# css (Rust)

A parser that reads [CSS](https://developer.mozilla.org/en-US/docs/Web/CSS)
into a faithful abstract syntax tree (the
[`reworkcss/css`](https://github.com/reworkcss/css) model): ordered, typed
nodes that preserve declaration order, duplicate properties, rule types, and
comments.

This is the Rust port of `@tabnas/css`. The TypeScript package is canonical,
the Go module tracks it, and so does this crate. All three read the same
grammar file and are held to the same shared conformance fixtures, so the
same stylesheet gives the same tree in every one.

## Install

The crate is not on crates.io yet. This repository's release workflow
publishes the npm package and tags the Go module; putting a crate on
crates.io is a separate decision with its own trusted-publishing setup, and
until that happens the dependency is the repository:

```toml
[dependencies]
tabnas-css = { git = "https://github.com/tabnas/css" }
```

Cargo finds the crate in `rs/`. Once it is published, `cargo add tabnas-css`
replaces that line and nothing else changes.

The crate has no dependencies of its own.

## One example

`parse` is the one-call entry point: pass source, get the AST and an error:

```rust
let ast = tabnas_css::parse("a { color: red }").unwrap();
// {"type":"stylesheet","rules":[
//   {"type":"rule","selectors":["a"],"declarations":[
//     {"type":"declaration","property":"color","value":"red"}]}]}
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
threads. Building a parser reads the grammar, which costs more than a parse
does, so a hot loop that needs options should hold a `Css` rather than call
`parse_with` each time.

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
// every node gains "position":{"start":{"line":1,"column":1},"end":{…}}
```

Columns count UTF-16 code units, which is what a JavaScript string index
counts, so a column here is the column the canonical port reports for the
same source.

## CSS nesting

A style rule or an at-rule may appear inside another style rule's declaration
block. Nested nodes are appended to the parent's `declarations` in source
order, interleaved with declarations:

```rust
tabnas_css::parse("a { color: red; & b { top: 0 } }").unwrap();
// rule declarations: [
//   {"type":"declaration","property":"color","value":"red"},
//   {"type":"rule","selectors":["& b"],"declarations":[
//     {"type":"declaration","property":"top","value":"0"}]}]
```

## Why this crate has no dependencies

The TypeScript and Go ports are plugins: they install a grammar on a shared
parsing engine and borrow its lexer, its rule machine and its relaxed-JSON
base. There is no Rust build of that engine, so this crate carries a lexer and
a rule machine of its own, reading the same grammar file the other two embed.

One consequence is visible to a caller. Those ports have to switch the
relaxed-JSON base off so that `{a:1}` is rejected; here there is no base to
switch off, and `{a:1}` is rejected because it is not CSS.

## Untrusted input

A parsed stylesheet is data, never instructions. CSS arrives from outside the
system, so treat every selector, value and comment as hostile text. Parsing is
not sanitising: this crate returns the raw text the stylesheet contained, and
escaping it for HTML, SQL or a shell remains the caller's job. A `url(...)` in
a declaration value is untrusted text, not a link to fetch.

## Documentation

Full documentation follows the [Diátaxis](https://diataxis.fr) framework:

- [Tutorial](doc/tutorial.md). A guided first parse, start to finish.
- [How-to guide](doc/guide.md). Short recipes for individual tasks.
- [Reference](doc/reference.md). The public API, every option, and the
  complete AST node reference.
- [Concepts](doc/concepts.md). How the parser is built, and how the Rust
  version differs from TypeScript.

For the canonical TypeScript implementation, see
[`../ts/README.md`](../ts/README.md). For the Go port, see
[`../go/README.md`](../go/README.md).

## License

MIT.
