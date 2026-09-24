# Reference (Rust)

The complete public surface of `tabnas-css`, every option, and every
node the parser can produce. Statements here are normative: the other
pages link back to this one rather than restating it.

## Crate

```toml
[dependencies]
# Not on crates.io yet; `tabnas-css = "0.5"` once it is.
tabnas-css = { git = "https://github.com/tabnas/css" }
```

```rust
use tabnas_css::{Css, Error, Node, Options, Value};
```

The crate name is `tabnas-css` and the library name is `tabnas_css`. It
declares no dependencies, and it needs no build step, no feature flags
and no network access at build time.

`tabnas_css::VERSION` is the crate's version as a `&'static str`. It
equals the version in `Cargo.toml`, and equals the version the
TypeScript package and the Go module carry.

## Public API

### `fn parse(src: &str) -> Result<Value, Error>`

Parse a CSS document with the default options. Returns a
`Value::Node` holding a `stylesheet` node, or an [`Error`](#error).

The call reuses one process-wide parser, built on first use. It is cheap
to call repeatedly and safe to call from several threads.

```rust
let ast = tabnas_css::parse("a { color: red }").unwrap();
assert_eq!(Some("stylesheet"), ast.as_node().unwrap().node_type());
```

### `fn parse_with(src: &str, options: Options) -> Result<Value, Error>`

Parse with the given options. Each call builds a parser, so a loop over
many inputs should hold a [`Css`](#struct-css) instead.

### `struct Css`

A reusable parser.

| Method | Returns | What it does |
|---|---|---|
| `Css::new()` | `Css` | A parser with the default options. |
| `Css::with_options(Options)` | `Css` | A parser with the given options. |
| `css.parse(&str)` | `Result<Value, Error>` | Parse a document. |
| `css.options()` | `Options` | The options this parser was built with. |
| `css.grammar()` | `&Grammar` | The rule table this parser runs. |

`Css` implements `Default` and `Debug`. It is immutable after
construction, so `&Css` can be shared across threads.

```rust
use tabnas_css::{Css, Options};

let css = Css::with_options(Options { position: true, ..Options::default() });
assert!(css.options().position);
assert!(css.parse("a { x: 1 }").is_ok());
```

### `struct Options`

Plugin options. Both fields default to `false`, and `Options` implements
`Default`, `Clone`, `Copy`, `Debug` and `PartialEq`.

| Field | Type | Default |
|---|---|---|
| `lowercase_properties` | `bool` | `false` |
| `position` | `bool` | `false` |

### `enum Value`

What an AST node field can hold.

| Variant | Serialises as |
|---|---|
| `Value::Undefined` | nothing, when it is an object key; `null` otherwise |
| `Value::Null` | `null` |
| `Value::Bool(bool)` | `true` / `false` |
| `Value::Num(f64)` | a number, with no decimal point when the value is integral |
| `Value::Str(String)` | a JSON string |
| `Value::List(Vec<Value>)` | a JSON array |
| `Value::Node(Node)` | a JSON object |

| Method | Returns |
|---|---|
| `as_str()` | `Option<&str>` for a `Str` |
| `as_node()` | `Option<&Node>` for a `Node` |
| `as_list()` | `Option<&[Value]>` for a `List` |
| `to_json()` | the value as JSON text |

`Undefined` is JavaScript's `undefined`: a key that exists, in insertion
order, but is absent from the serialised JSON. The canonical port writes
`end: undefined` into a `position` at construction and fills it in
later, so a node whose end is never recorded serialises with a `start`
and no `end` at all.

Dropping a `Value`, cloning one, comparing two, writing one as JSON and
formatting one for `Debug` are all iterative, so an AST as deep as its
source cannot exhaust the stack on any of them. None of the five is
derived.

### `struct Node`

An insertion-ordered, string-keyed map.

| Method | Returns |
|---|---|
| `Node::new()` | an empty node |
| `node_type()` | `Option<&str>`, the `type` key |
| `get(&str)` | `Option<&Value>` |
| `get_mut(&str)` | `Option<&mut Value>` |
| `set(key, value)` | nothing; keeps an existing key's position |
| `push_to(&str, Value)` | nothing; appends to a list field, creating it if absent |
| `iter()` | the entries, in insertion order |
| `len()` / `is_empty()` | the key count, `Undefined` keys included |
| `to_json()` | the node as JSON text, `Undefined` keys omitted |

### `struct Error`

A parse failure.

| Field | Type | What it is |
|---|---|---|
| `code` | `String` | the error code, and the part to branch on |
| `message` | `String` | a human-readable explanation |
| `line` | `usize` | 1-based line where the parse stopped |
| `column` | `usize` | 1-based column, in UTF-16 code units |

`Error` implements `Display`, `Debug`, `Clone`, `PartialEq` and
`std::error::Error`.

### `mod grammar`

`Grammar::load()` reads the embedded grammar; `Grammar::parse(&str)`
reads one supplied as text, and returns `Result<Grammar, String>`.
`grammar::grammar_text()` is the verbatim grammar the crate was built
from. `Grammar::rule(&str)` returns a `RuleDef`, whose `open` and
`close` are the `Alt` lists the machine tries.

This module is public so that a caller can inspect what the parser runs.
Nothing in it is needed to parse CSS.

### `mod lex`

The token kinds (`Tin`), the token type, and the scanner helpers.
`Tin::name()` gives the grammar name of a token kind and
`Tin::describe()` a human description. Public for the same reason as
`grammar`: so that the parse can be inspected.

## Options

### `lowercase_properties`

Default `false`. When `true`, declaration property names are lowercased.
Selectors, values, at-rule preludes and comment text are untouched.

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

The lowercasing happens in the lexer, so the `property` field never
holds the original case.

### `position`

Default `false`. When `true`, every node gains:

```text
"position": { "start": { "line": L, "column": C },
              "end":   { "line": L, "column": C } }
```

Lines and columns are 1-based. `start` is the first character of the
construct; `end` is the position just past its last character. A column
counts UTF-16 code units, which is what a JavaScript string index
counts, so `#©{…}` and `#𝄞{…}` report the columns the canonical port
reports rather than byte or character offsets.

```rust
use tabnas_css::{Css, Options};

let css = Css::with_options(Options { position: true, ..Options::default() });
let ast = css.parse("a {\n  color: red;\n}").unwrap();
assert!(ast.to_json().contains(
    r#""position":{"start":{"line":2,"column":3},"end":{"line":2,"column":13}}"#
));
```

`end` can be absent. A declaration with an empty value (`p { color:; }`)
never runs the step that records one, so its `position` serialises as a
`start` alone.

## The AST

Every node is a `Node` with a `type` key. The parse result is always a
`stylesheet`.

### `stylesheet`

| Key | Type |
|---|---|
| `type` | `"stylesheet"` |
| `rules` | list of nodes |

The top of every tree. `rules` holds one node per top-level statement,
in source order.

### `rule`

| Key | Type |
|---|---|
| `type` | `"rule"` |
| `selectors` | list of strings |
| `declarations` | list of nodes |

A style rule. `selectors` is the prelude split on top-level commas, each
one trimmed and with comments stripped. `declarations` holds
`declaration`, `comment` and, under CSS nesting, `rule` and at-rule
nodes.

### `declaration`

| Key | Type |
|---|---|
| `type` | `"declaration"` |
| `property` | string |
| `value` | string |

`value` is raw text up to the next top-level `;` or `}`, trimmed, with
comments stripped and quotes kept. An empty value is the empty string
rather than a missing key.

A property name is not an identifier. Besides the identifier characters
it admits `*`, `#`, `/` and `\`, which carry the legacy `*prop`,
`#prop` and `//prop` hacks, plus an optional `[0-9a-z_-]+` bracket
suffix such as `opacity[sqrt]`.

### `comment`

| Key | Type |
|---|---|
| `type` | `"comment"` |
| `comment` | string |

The text between `/*` and `*/`, verbatim, surrounding spaces included.
A comment becomes a node only at a statement, declaration, or keyframe
list position. Elsewhere it is skipped.

### CSS nesting

A style rule or at-rule inside a declaration block is appended to the
parent rule's `declarations` in source order. The disambiguation is by
the token after the key: a `:` makes it a declaration, a `{` or a `,`
makes it a nested rule.

```rust
let ast = tabnas_css::parse("a { color: red; & b { top: 0 } }").unwrap();
assert!(ast.to_json().contains(r#""declarations":[{"type":"declaration""#));
assert!(ast.to_json().contains(r#"{"type":"rule","selectors":["& b"]"#));
```

### Block at-rules with a rules body

`@media`, `@supports`, `@document`, `@host`, and any unrecognised block
at-rule.

| `type` | Prelude key | Other keys |
|---|---|---|
| `media` | `media` | `rules` |
| `supports` | `supports` | `rules` |
| `document` | `document` | `vendor`, `rules` |
| `host` | none | `rules` |
| anything else | the keyword | `rules` |

`document` always carries `vendor`, which is the empty string when the
at-keyword has no prefix. An at-rule prelude keeps its comments, and is
trimmed and nothing more.

### Block at-rules with a declarations body

`@font-face`, `@page`, `@viewport`, `@-ms-viewport`, `@counter-style`,
`@property` and `@font-palette-values`.

| `type` | Keys |
|---|---|
| `font-face` | `declarations` |
| `page` | `selectors`, `declarations` |
| the others | `declarations` |

`@page` splits its prelude on top-level commas into `selectors`, which
is an empty list when there is no prelude.

### `keyframes` and `keyframe`

| Key | Type |
|---|---|
| `type` | `"keyframes"` |
| `name` | string |
| `vendor` | string, present only when the at-keyword is prefixed |
| `keyframes` | list of `keyframe` nodes |

A `keyframe` node carries `values` (a list of strings such as `from`,
`to`, `0%`) and `declarations`. The at-keyword matches
`-vendor-keyframes` as well as `keyframes`, and the prefix moves into
`vendor`.

### Statement at-rules

An at-rule with no block. The keyword is the node type and the field
holding its raw params.

| `type` | Keys |
|---|---|
| `import` | `import` |
| `charset` | `charset` |
| `namespace` | `namespace` |
| anything else | the keyword |
| `custom-media` | `name`, `media` |

`@custom-media` is the one that splits: its params divide at the first
whitespace after the `--name`, matching upstream.

## CSS syntax accepted

### Rulesets

A prelude, then a `{ ... }` block. The prelude is split on top-level
commas; a comma inside a string, inside `()` or `[]`, or behind a `\`
escape belongs to the selector it sits in. Selector text is trimmed and
has comments stripped, and is otherwise verbatim, so combinators,
attribute selectors, pseudo-elements and escapes all survive.

### Declarations

`property: value`, separated by `;`. A trailing `;` is optional, and an
empty declaration list is legal. A `;` or `}` inside a string, inside
brackets or behind an escape does not end the value.

### At-rules

Classified by a `{`-before-`;` lookahead into a block at-rule or a
statement at-rule, and block at-rules are classified further by keyword
into the three body kinds above. A statement at-rule needs a terminating
`;`, or the end of input, or a `}`.

### Comments

`/* ... */` only. A `//` line comment is not CSS: `//prop` is a property
name, not a comment. An unclosed `/*` is an error rather than a comment
running to the end of input, matching upstream.

### Empty input

A zero-length source yields `{"type":"stylesheet","rules":[]}`. So does
a source of whitespace alone, or of comments alone, except that the
latter yields the comment nodes.

### What is rejected

The reworkcss model rejects documents that CSS Syntax Level 3 recovers
from, and this parser follows the model:

| Input | Why |
|---|---|
| `{size: large}` | no selector |
| `a { x: 1 ` | unclosed block |
| `/*` | unclosed comment |
| `a{c:1} extra` | trailing text that is not a rule |
| `}` | a stray close brace |

There is no error-recovery mode. A document either parses or raises.

## Tokens

The lexer emits these kinds, and the grammar assembles them. `Tin` names
them.

| Token | What it is |
|---|---|
| `#OB` | `{`, start of a block |
| `#CB` | `}`, end of a block |
| `#CL` | `:`, declaration separator |
| `#CA` | `;`, declaration terminator |
| `#TX` | one selector, keyframe value, or property name |
| `#GC` | `,`, selector-group separator |
| `#VL` | a declaration value, as raw text |
| `#CC` | a comment, at a list position |
| `#ATR` | an at-rule with a rules body |
| `#ATD` | an at-rule with a declarations body |
| `#ATK` | `@keyframes` |
| `#ATS` | a statement at-rule |
| `#ZZ` | end of input |

Which token a character starts depends on the rule that is active when
it is read, because the same characters can begin a selector, a property
or a value. That is the whole reason the lexer takes a rule name.

## Errors

Two codes, both inherited from the model rather than declared here.

| `code` | Raised when |
|---|---|
| `unterminated_comment` | a `/*` has no closing `*/` |
| `unexpected` | no grammar alternative matches the next token |

```rust
assert_eq!("unterminated_comment", tabnas_css::parse("/*").unwrap_err().code);
assert_eq!("unexpected", tabnas_css::parse("}").unwrap_err().code);
```

`line` and `column` say where the parse stopped. `message` explains it
and is meant for a person, so branch on `code` rather than on text.
