# Tutorial: your first CSS parse (Rust)

By the end of this page you will have parsed a stylesheet with
`tabnas-css`, read the tree it produced, and turned on both of its
options. Every step shows what the parser actually returns.

You need Rust and Cargo, and a crate to work in. Nothing else: this
crate has no dependencies.

If you have already parsed CSS with the TypeScript or Go version, the
tree is the same tree and only the Rust types are new. Skip to
[the reference](reference.md).

## 1. Install

The crate is not on crates.io yet, so the dependency is the repository.
Add it to `Cargo.toml`:

```toml
[dependencies]
tabnas-css = { git = "https://github.com/tabnas/css" }
```

Cargo finds the crate in `rs/`. That is the whole install: there is no
engine to bring along, no build script, and no optional features. When the
crate is published, this line becomes `tabnas-css = "0.5"` and nothing
else about the rest of this page changes.

## 2. Parse a rule

`parse` takes source and returns the AST:

```rust
let ast = tabnas_css::parse("a { color: red }").unwrap();
assert_eq!(
    ast.to_json(),
    r#"{"type":"stylesheet","rules":[{"type":"rule","selectors":["a"],"declarations":[{"type":"declaration","property":"color","value":"red"}]}]}"#
);
```

The top of the tree is always a `stylesheet` node, and its `rules` hold
one node per statement. `to_json` is there for when the tree is going
somewhere else; the next step reads it in place.

## 3. Read the AST

A node is a `Node`: an insertion-ordered map with a `type` key. `get`
returns a `Value`, and the `Value` accessors turn one into something
concrete:

```rust
use tabnas_css::Value;

let ast = tabnas_css::parse("a { color: red }").unwrap();
let sheet = ast.as_node().unwrap();
assert_eq!(Some("stylesheet"), sheet.node_type());

let rules = sheet.get("rules").and_then(Value::as_list).unwrap();
let rule = rules[0].as_node().unwrap();
assert_eq!(Some("rule"), rule.node_type());

let decl = rule.get("declarations").and_then(Value::as_list).unwrap()[0]
    .as_node().unwrap();
assert_eq!(Some("color"), decl.get("property").and_then(Value::as_str));
assert_eq!(Some("red"), decl.get("value").and_then(Value::as_str));
```

A declaration value is raw text. The parser trims it and strips comments
out of it, and leaves everything else alone, quotes included.

## 4. Group selectors

A comma at the top level of a prelude separates selectors, so one rule
can carry several:

```rust
use tabnas_css::Value;

let ast = tabnas_css::parse("h1, h2 { margin: 0 }").unwrap();
let rule = ast.as_node().unwrap()
    .get("rules").and_then(Value::as_list).unwrap()[0]
    .as_node().unwrap();
let selectors: Vec<&str> = rule
    .get("selectors").and_then(Value::as_list).unwrap()
    .iter().filter_map(Value::as_str).collect();
assert_eq!(vec!["h1", "h2"], selectors);
```

Only a top-level comma splits. A comma inside `:not(...)`, inside
brackets or inside a string belongs to the selector it sits in:

```rust
use tabnas_css::Value;

let ast = tabnas_css::parse("a:not(.x, .y), b { top: 0 }").unwrap();
let rule = ast.as_node().unwrap()
    .get("rules").and_then(Value::as_list).unwrap()[0]
    .as_node().unwrap();
let selectors: Vec<&str> = rule
    .get("selectors").and_then(Value::as_list).unwrap()
    .iter().filter_map(Value::as_str).collect();
assert_eq!(vec!["a:not(.x, .y)", "b"], selectors);
```

## 5. Keep comments

A comment between statements, or between declarations, is a node of its
own and keeps its place in source order:

```rust
let ast = tabnas_css::parse("/* head */ a { color: red; /* why */ }").unwrap();
assert!(ast.to_json().contains(r#"{"type":"comment","comment":" head "}"#));
assert!(ast.to_json().contains(r#"{"type":"comment","comment":" why "}"#));
```

`comment` is the text between the delimiters, with its surrounding
spaces intact. A comment in the middle of a construct, between a
property and its colon, has no list position to sit in, so it is
dropped:

```rust
let ast = tabnas_css::parse("a { color /* hm */ : red }").unwrap();
assert!(!ast.to_json().contains("comment"));
```

## 6. Nest a block at-rule

An at-rule whose body is a list of rules becomes a node with its own
`rules`, and the prelude lands in a field named after the keyword:

```rust
let ast = tabnas_css::parse("@media screen { a { color: blue } }").unwrap();
assert_eq!(
    ast.to_json(),
    r#"{"type":"stylesheet","rules":[{"type":"media","media":"screen","rules":[{"type":"rule","selectors":["a"],"declarations":[{"type":"declaration","property":"color","value":"blue"}]}]}]}"#
);
```

An at-rule prelude keeps its comments. Selectors, values, and property
names do not, and that difference matches upstream rather than being a
choice made here.

## 7. Parse a statement at-rule

An at-rule with no block is a leaf node, and its params ride in a field
named after the keyword:

```rust
let ast = tabnas_css::parse(r#"@import "base.css";"#).unwrap();
assert_eq!(
    ast.to_json(),
    r#"{"type":"stylesheet","rules":[{"type":"import","import":"\"base.css\""}]}"#
);
```

The quotes stay. Params are raw text, exactly as with a declaration
value.

## 8. Parse a keyframes block

`@keyframes` gets a shape of its own, because its body is neither rules
nor declarations:

```rust
let ast = tabnas_css::parse(
    "@keyframes slide { from { left: 0 } 50%, 100% { left: 10px } }",
).unwrap();
assert_eq!(
    ast.to_json(),
    r#"{"type":"stylesheet","rules":[{"type":"keyframes","name":"slide","keyframes":[{"type":"keyframe","values":["from"],"declarations":[{"type":"declaration","property":"left","value":"0"}]},{"type":"keyframe","values":["50%","100%"],"declarations":[{"type":"declaration","property":"left","value":"10px"}]}]}]}"#
);
```

Each keyframe carries `values` rather than `selectors`, and a vendor
prefix on the at-keyword is split out into a `vendor` field:

```rust
let ast = tabnas_css::parse("@-webkit-keyframes x { to { opacity: 1 } }").unwrap();
assert!(ast.to_json().contains(r#""name":"x","vendor":"-webkit-""#));
```

## 9. Turn on an option

Both options are off by default. Build a `Css` with the ones you want
and keep it:

```rust
use tabnas_css::{Css, Options};

let css = Css::with_options(Options {
    lowercase_properties: true,
    ..Options::default()
});
let ast = css.parse("A { COLOR: Red }").unwrap();
assert!(ast.to_json().contains(r#""property":"color","value":"Red""#));
```

Only the property name changed. The selector and the value are left as
the author wrote them, because only property names are case-insensitive
in CSS.

Positions work the same way:

```rust
use tabnas_css::{Css, Options};

let css = Css::with_options(Options { position: true, ..Options::default() });
let ast = css.parse("a {\n  color: red;\n}").unwrap();
assert!(ast.to_json().contains(
    r#""position":{"start":{"line":2,"column":3},"end":{"line":2,"column":13}}"#
));
```

Lines and columns are 1-based, and `end` is the position just past the
last character. Hold the `Css` rather than rebuilding one: building
reads the grammar, which costs more than a parse.

## 10. The empty cases

An empty source is an empty stylesheet, and so is a source that holds
only whitespace:

```rust
let empty = r#"{"type":"stylesheet","rules":[]}"#;
assert_eq!(empty, tabnas_css::parse("").unwrap().to_json());
assert_eq!(empty, tabnas_css::parse("   \n  ").unwrap().to_json());
```

A document that is not CSS is an error rather than an empty tree:

```rust
let err = tabnas_css::parse("{size: large}").unwrap_err();
assert_eq!("unexpected", err.code);
```

That one has no selector, and this parser follows upstream in rejecting
it. There are two error codes in total, and the
[reference](reference.md#errors) lists both.

## Where to go next

- [How-to guide](guide.md) for recipes: walking the tree, reading nested
  rules, handling errors.
- [Reference](reference.md) for the full API and every node type.
- [Concepts](concepts.md) for how the parse works and where this port
  differs from the canonical one.
