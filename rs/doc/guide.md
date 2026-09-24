# How-to guide (Rust)

Short recipes for tasks you already know you want. Each one assumes the
crate is installed and says what it returns. For the full API and every
node type, see the [reference](reference.md); for a guided first parse,
the [tutorial](tutorial.md).

Two conventions run through every recipe. `parse` is the one-call entry
point and reuses a process-wide parser, so it is cheap to call and safe
across threads. Anything with options goes through a `Css` you build and
keep, because building one reads the grammar and costs more than a parse
does.

## Parse a single stylesheet

```rust
let ast = tabnas_css::parse("a { color: red }").unwrap();
assert_eq!(Some("stylesheet"), ast.as_node().unwrap().node_type());
```

`parse` returns `Result<Value, Error>`. The `Ok` value is always a
`stylesheet` node.

## Walk the AST

`Node::get` returns `Option<&Value>`, and the `Value` accessors narrow
one to a string, a list, or a node. Nothing panics on a missing key:

```rust
use tabnas_css::Value;

fn properties(src: &str) -> Vec<String> {
    let ast = tabnas_css::parse(src).unwrap();
    let mut out = Vec::new();
    let mut stack: Vec<Value> = vec![ast];
    while let Some(value) = stack.pop() {
        match value {
            Value::List(items) => stack.extend(items),
            Value::Node(node) => {
                if Some("declaration") == node.node_type() {
                    if let Some(p) = node.get("property").and_then(Value::as_str) {
                        out.push(p.to_string());
                    }
                }
                stack.extend(node.iter().map(|(_, v)| v.clone()));
            }
            _ => {}
        }
    }
    out.sort();
    out
}

assert_eq!(
    vec!["color".to_string(), "top".to_string()],
    properties("a { color: red } @media x { b { top: 0 } }")
);
```

An explicit stack rather than recursion, because a stylesheet is as deep
as its author made it.

## Preserve declaration order and duplicates

Nothing is merged and nothing is reordered. Two declarations of
the same property are two nodes:

```rust
let ast = tabnas_css::parse("a { color: red; color: blue }").unwrap();
assert!(ast.to_json().contains(r#""value":"red"},{"type":"declaration","property":"color","value":"blue""#));
```

Which one wins is a cascade question, and the cascade is not this
parser's business.

## Keep compound and function values intact

A value is raw text up to the next top-level `;` or `}`. Commas, spaces
and parentheses inside it are part of the value:

```rust
let ast = tabnas_css::parse("p { color: rgb(1, 2, 3); border: 1px solid #fff }").unwrap();
assert!(ast.to_json().contains(r#""value":"rgb(1, 2, 3)""#));
assert!(ast.to_json().contains(r#""value":"1px solid #fff""#));
```

A `;` inside a string or inside brackets does not end the value either:

```rust
let ast = tabnas_css::parse(r#"a { background: url("a;b.png") }"#).unwrap();
assert!(ast.to_json().contains(r#"url(\"a;b.png\")"#));
```

## Keep comments as nodes

A comment at a statement or declaration position becomes a node in
source order:

```rust
let ast = tabnas_css::parse("a { /* c1 */ color: red; /* c2 */ }").unwrap();
assert!(ast.to_json().contains(r#"{"type":"comment","comment":" c1 "}"#));
assert!(ast.to_json().contains(r#"{"type":"comment","comment":" c2 "}"#));
```

A comment anywhere else is skipped. Inside a selector or a value it is
removed from the text; between a property and its colon it has no list
position and disappears.

## Read a grouped selector

`selectors` is a list, split on top-level commas only:

```rust
use tabnas_css::Value;

let ast = tabnas_css::parse(r#"[type="checkbox"], [type="radio"] { padding: 0 }"#).unwrap();
let rule = ast.as_node().unwrap()
    .get("rules").and_then(Value::as_list).unwrap()[0]
    .as_node().unwrap();
assert_eq!(
    2,
    rule.get("selectors").and_then(Value::as_list).unwrap().len()
);
```

## Parse a block at-rule

`@media`, `@supports`, `@document`, `@host` and any unknown block
at-rule produce a node with `rules`:

```rust
let ast = tabnas_css::parse("@supports (display: grid) { a { x: 1 } }").unwrap();
assert!(ast.to_json().contains(r#"{"type":"supports","supports":"(display: grid)","rules":["#));
```

An unknown keyword takes the same shape, with the keyword as both the
node type and the prelude field:

```rust
let ast = tabnas_css::parse("@layer base { a { c: 1 } }").unwrap();
assert!(ast.to_json().contains(r#"{"type":"layer","layer":"base","rules":["#));
```

## Parse font-face and page

These carry declarations rather than rules. `@page` also splits its
prelude into `selectors`:

```rust
let ast = tabnas_css::parse("@page toc, index:blank { color: green }").unwrap();
assert!(ast.to_json().contains(r#""type":"page","selectors":["toc","index:blank"]"#));
```

## Parse a statement at-rule

An at-rule with no block is a leaf, and its params ride in a field named
after the keyword:

```rust
let ast = tabnas_css::parse("@namespace svg url(http://x);").unwrap();
assert_eq!(
    ast.to_json(),
    r#"{"type":"stylesheet","rules":[{"type":"namespace","namespace":"svg url(http://x)"}]}"#
);
```

`@custom-media` is the exception: its params split into a `name` and a
`media` query, as upstream does.

```rust
let ast = tabnas_css::parse("@custom-media --narrow (max-width: 30em);").unwrap();
assert!(ast.to_json().contains(r#""name":"--narrow","media":"(max-width: 30em)""#));
```

## Parse keyframes

The body is a list of `keyframe` nodes, each with `values`:

```rust
use tabnas_css::Value;

let ast = tabnas_css::parse("@keyframes s { from { left: 0 } to { left: 1px } }").unwrap();
let kf = ast.as_node().unwrap()
    .get("rules").and_then(Value::as_list).unwrap()[0]
    .as_node().unwrap();
assert_eq!(Some("s"), kf.get("name").and_then(Value::as_str));
assert_eq!(2, kf.get("keyframes").and_then(Value::as_list).unwrap().len());
```

## Read nested rules

A nested style rule or at-rule lands in the parent rule's
`declarations`, in source order, interleaved with declarations:

```rust
use tabnas_css::Value;

let ast = tabnas_css::parse("a { color: red; & b { top: 0 } }").unwrap();
let decls = ast.as_node().unwrap()
    .get("rules").and_then(Value::as_list).unwrap()[0]
    .as_node().unwrap()
    .get("declarations").and_then(Value::as_list).unwrap();
assert_eq!(Some("declaration"), decls[0].as_node().unwrap().node_type());
assert_eq!(Some("rule"), decls[1].as_node().unwrap().node_type());
```

So a walk over `declarations` has to check `type` rather than assume
every member is a declaration.

## Get source positions

```rust
use tabnas_css::{Css, Options};

let css = Css::with_options(Options { position: true, ..Options::default() });
let ast = css.parse("a { x: 1 }").unwrap();
assert!(ast.to_json().contains(r#""position":{"start":{"line":1,"column":1}"#));
```

Lines and columns are 1-based. `end` is the position just past the last
character, and columns count UTF-16 code units so that a column here is
the column the canonical port reports for the same source.

One node can lack an `end`: a declaration whose value is empty never
reaches the step that records one, so its `position` serialises with a
`start` and nothing else.

## Lowercase property names

```rust
use tabnas_css::{Css, Options};

let css = Css::with_options(Options {
    lowercase_properties: true,
    ..Options::default()
});
let ast = css.parse("A { COLOR: Red }").unwrap();
assert!(ast.to_json().contains(r#""selectors":["A"]"#));
assert!(ast.to_json().contains(r#""property":"color","value":"Red""#));
```

Property names only. CSS property names are case-insensitive; selectors
and values are not.

## Reuse a parser for many inputs

```rust
use tabnas_css::{Css, Options};

let css = Css::with_options(Options { position: true, ..Options::default() });
for src in ["a { x: 1 }", "b { y: 2 }"] {
    assert!(css.parse(src).is_ok());
}
```

`Css` is immutable once built, so one can be shared behind a reference
across threads. The no-options `parse` already does this internally.

## Handle the empty cases

```rust
let empty = r#"{"type":"stylesheet","rules":[]}"#;
assert_eq!(empty, tabnas_css::parse("").unwrap().to_json());
assert_eq!(empty, tabnas_css::parse("  ").unwrap().to_json());
```

An empty source is an empty stylesheet rather than an error, matching
upstream.

## Handle a parse error

`Error` carries a `code`, a `message`, and the line and column where the
parse stopped:

```rust
let err = tabnas_css::parse("a { x: 1").unwrap_err();
assert_eq!("unexpected", err.code);
assert_eq!(1, err.line);
```

`code` is the part to branch on. There are two, `unexpected` and
`unterminated_comment`, and the [reference](reference.md#errors)
describes when each is raised. `Error` implements `Display` and
`std::error::Error`, so `?` works in any function returning
`Box<dyn std::error::Error>`.

## Emit the tree as JSON

```rust
let ast = tabnas_css::parse("a { x: 1 }").unwrap();
let text = ast.to_json();
assert!(text.starts_with(r#"{"type":"stylesheet""#));
```

`to_json` writes keys in insertion order, so `type` comes first. It is
iterative rather than recursive, so a deeply nested stylesheet
serialises rather than exhausting the stack.
