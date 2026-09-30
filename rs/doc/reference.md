# Reference (Rust)

The complete public surface of `tabnas-css`, every option, and every
node the parser can produce. Statements here are normative: the other
pages link back to this one rather than restating it.

## Crate

| | |
|---|---|
| Package | `tabnas-css` |
| Library | `tabnas_css` |
| Minimum Rust | 1.85 (`rust-version`), the engine's own floor |
| Dependencies | `tabnas-parser`, renamed `tabnas` in the manifest; `tabnas-jsonic`, which depends on `tabnas-json` |
| Dev-dependencies | `tabnas-debug`, for the tests only |
| Features | none |
| Build script | none |

```rust
use tabnas_css::{Css, Error, Node, Options, Value};
```

This page documents the crate this repository builds, which is taken
from the repository with the `[patch]` tables the
[README](../README.md#install) lists, because its own dependencies are
sibling path dependencies. The `tabnas-css` 0.5.9 on crates.io is a
different implementation: it has no dependencies, and none of the
plugin API below.

The engine's types appear in this API as `tabnas::…`: `Tabnas`,
`Plugin`, `PluginError`, `Value` and `TabnasError`. The crate re-exports
the engine as `tabnas_css::tabnas` and jsonic as `tabnas_css::tabnas_jsonic`;
name their types through those paths. A dependency of your own on
either can resolve to another copy of the crate, one from crates.io for
instance, whose types are not the ones this API takes and returns.

`tabnas_css::VERSION` is the crate's version as a `&'static str`. It
equals the version in `Cargo.toml`, and equals the version the
TypeScript package and the Go module carry.

## Public API

### `fn parse(src: &str) -> Result<Value, Error>`

Parse a CSS document with the default options. Returns a
`Value::Node` holding a `stylesheet` node, or an [`Error`](#struct-error).

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

A reusable parser: a jsonic engine with the plugin installed, and the
options it was built with.

| Method | Returns | What it does |
|---|---|---|
| `Css::new()` | `Css` | A parser with the default options. |
| `Css::with_options(Options)` | `Css` | A parser with the given options. |
| `css.parse(&str)` | `Result<Value, Error>` | Parse a document into this crate's [`Value`](#enum-value). |
| `css.options()` | `Options` | The options this parser was built with. |
| `css.tabnas()` | `&tabnas::Tabnas` | The engine this parser runs. Its own parse returns the [tree form](#the-two-result-forms). |

Building a `Css` installs the grammar on a new engine, which costs more
than a parse. `Css` implements `Default`, and `Debug`, which shows the
options only. It is `Send` and `Sync`, and immutable after
construction, so `&Css` can be shared across threads.

```rust
use tabnas_css::{Css, Options};

let css = Css::with_options(Options { position: true, ..Options::default() });
assert!(css.options().position);
assert!(css.parse("a { x: 1 }").is_ok());
```

### `struct Options`

Plugin options: `CssOptions` in the canonical port. Both fields default
to `false`, and `Options` implements `Default`, `Clone`, `Copy`,
`Debug`, `PartialEq` and `Eq`.

| Field | Type | Default |
|---|---|---|
| `lowercase_properties` | `bool` | `false` |
| `position` | `bool` | `false` |

| Method | Returns | What it does |
|---|---|---|
| `Options::from_value(&tabnas::Value)` | `Options` | Reads an engine option bag: the keys `lowercaseProperties` and `position`. |
| `options.to_value()` | `tabnas::Value` | An engine option bag holding both keys as `true` or `false`, for `use_plugin`. |

`from_value` reads each key for its JavaScript truthiness, as
`!!options.position` does in the canonical port: an absent key,
`undefined`, `null`, `false`, `0`, `NaN` and `""` are `false`, and any
other value is `true`. Other keys are ignored, and a bag that is not an
object gives the defaults.

```rust
use tabnas_css::Options;

let options = Options { lowercase_properties: true, position: false };
assert_eq!(options, Options::from_value(&options.to_value()));
```

### `fn plugin() -> tabnas::Plugin`

The grammar as an engine plugin, named `css`: `Css` in the canonical
port. Its option bag is the one [`Options::to_value`](#struct-options)
writes, with both keys `false` by default.

Installing it adds, to the engine it is used on:

- the 13 rules of `css-grammar.jsonic`, each alternate tagged with the
  group `css`;
- the option overrides: jsonic's own rules excluded, `stylesheet` the
  start rule, `;` as the member separator `#CA`, `[` and `]` not
  tokens, the string, number, text and value matchers off, `/* */` the
  only comment, `{"type":"stylesheet","rules":[]}` the result for `""`,
  and the rule history bounded at one link;
- the `cssToken` lex matcher, at order 100000, ahead of every builtin
  matcher;
- the actions that build the nodes, a `parse.prepare` hook that clears
  the plugin's per-parse state, a lex subscriber, and a parse guard
  named `tabnas-css/depth`, the tree form's bound.

Using it again on the same engine, or deriving an engine from one that
has it (`Tabnas::derive`), installs it again with the options then in
force: the matcher and the actions are replaced and the rules
installed afresh. The lex subscriber is added once per engine.

Use it on jsonic, as `make` does. On a bare
`tabnas::Tabnas::new()` it gives the same results.

```rust
let mut parser = tabnas_css::tabnas_jsonic::make();
parser.use_plugin(tabnas_css::plugin(), None).unwrap();
let tree = parser.parse("a { color: red }").unwrap();
assert_eq!(
    tree.to_json().to_string(),
    r#"{"type":"stylesheet","rules":[{"type":"rule","selectors":["a"],"declarations":[{"type":"declaration","property":"color","value":"red"}]}]}"#
);
```

### `fn css(parser: &mut tabnas::Tabnas, options: &Options) -> Result<(), tabnas::PluginError>`

Install the plugin on `parser` with `options`. It is
`parser.use_plugin(plugin(), Some(options.to_value()))`, so the install
is recorded and runs again on a derived engine.

### `fn make() -> tabnas::Tabnas`

A jsonic engine (`tabnas_css::tabnas_jsonic::make()`) with the plugin installed,
with the default options.

### `fn make_with(options: Options) -> tabnas::Tabnas`

The same, with `options`.

### `const TREE_RULE_DEPTH: usize`

`768`: the most rules the [tree form](#the-two-result-forms) lets be
open at once. That is 191 nested style rules, which take four rules
each (`decls`, `decl`, `sel` and `declbody`), or 256 nested `@media`
blocks, which take three (`items`, `statement` and `rulesbody`). One
level more fails with `cancel`. [`Css::parse`](#struct-css) is not
bounded.

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

`Value` implements `From<&str>`, `From<String>` and `From<Node>`.

`Undefined` is JavaScript's `undefined`: a key that exists, in insertion
order, but is absent from the serialised JSON. The canonical port writes
`end: undefined` into a `position` at construction and fills it in
later, so a node whose end is never recorded serialises with a `start`
and no `end` at all.

Dropping a `Value`, cloning one, comparing two, writing one as JSON and
formatting one for `Debug` are all iterative, so an AST as deep as its
source cannot exhaust the stack on any of them. None of the five is
derived. `Debug` writes the JSON, with `Undefined` keys shown as
`undefined` rather than dropped.

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

`Node` implements `Default`, and `Clone`, `PartialEq` and `Debug`
iteratively, as `Value` does.

### `struct Error`

A parse failure.

| Field | Type | What it is |
|---|---|---|
| `code` | `String` | the error code, and the part to branch on |
| `message` | `String` | a human-readable explanation |
| `line` | `usize` | 1-based line where the parse stopped |
| `column` | `usize` | 1-based column, in UTF-16 code units |

`Error` implements `Display`, `Debug`, `Clone`, `PartialEq`, `Eq` and
`std::error::Error`. `Display` writes
`[css/<code>]: <message> (line <line>, column <column>)`.

`Error` also implements `From<tabnas::TabnasError>`, for an error from
the engine's own parse. The code is kept, the message is the engine's
detail, the line is the engine's row, and the column is converted from
the engine's count of Unicode scalars to UTF-16 code units, the count
`Css::parse` reports:

```rust
let engine = tabnas_css::make().parse("#\u{1D11E}\u{1D11E} a{!}").unwrap_err();
assert_eq!(7, engine.col);
let error = tabnas_css::Error::from(engine);
assert_eq!(("unexpected", 1, 9), (error.code.as_str(), error.line, error.column));
```

### `mod grammar`

`grammar::grammar_text()` returns the verbatim `css-grammar.jsonic`
text the crate was built from, as a `&'static str`. Nothing in the
module is needed to parse CSS.

### `mod lex`

The token kinds and the text helpers the matcher uses.

| Item | What it is |
|---|---|
| `Tin` | the token kinds of the [token table](#tokens), each with `name()`, its grammar name, and `describe()`, a human description |
| `Error` | the parse failure, re-exported at the crate root |
| `es_trim(&str) -> &str` | trims as JavaScript's `String.prototype.trim` does |
| `es_is_whitespace(char) -> bool` | ECMAScript whitespace: Unicode `White_Space` plus U+FEFF, less U+0085 |
| `strip_comments(&str) -> String` | removes `/* … */` comments, leaving quoted strings untouched |
| `split_selectors(&str) -> Vec<String>` | splits a prelude on top-level commas, stripping comments and trimming each selector |
| `vendor_prefix(&str) -> Option<&str>` | the `-vendor-` prefix of an at-keyword, if it has one |

### `mod value`

`Value` and `Node`, re-exported at the crate root.

## The two result forms

A parse returns one of two forms, depending on the entry point.

| | `parse`, `parse_with`, `Css::parse` | the engine's parse: `make()`, `plugin()`, `Css::tabnas()` |
|---|---|---|
| Result | `Result<tabnas_css::Value, tabnas_css::Error>` | `Result<tabnas::Value, tabnas::TabnasError>` |
| JSON | `to_json()`, byte for byte the canonical port's `JSON.stringify`, key order included | the engine's writer: a whole number, such as a line, as `1.0` |
| Error column | UTF-16 code units | Unicode scalars; `Error::from` converts |
| Depth | no limit | [`TREE_RULE_DEPTH`](#const-tree_rule_depth-usize) open rules; one more fails with `cancel` |
| Recovery | none | with the engine's `parse.recover.enabled`, a partial stylesheet, not always the canonical port's |

Both forms hold the same tree, value for value: the canonical port's
plain objects. For `""` both are `{"type":"stylesheet","rules":[]}`,
with no `position` even when `position` is on. A `position.end` that
was never recorded, as for a declaration with an empty value, is an
undefined key in `Css::parse`'s tree, which its JSON leaves out, and no
key at all in the tree form.

The tree form's bound is a parse guard named `tabnas-css/depth`, a name
no other plugin uses, so installing jsonic or a grammar layered on it
after this plugin leaves it in place. It exists
because the engine's `Value` drops, clones, compares, and prints by
recursion, one stack frame per level. A tree at the bound survives all
of those on a 2 MiB thread in a debug build. The canonical port has no
such bound, so this one is a registered divergence of the Rust port.

`Css::parse` asks the plugin to keep each node as a flat record rather
than a nested engine value, and builds this crate's `Value` from the
records without recursion. Its depth is bounded by memory alone.

```rust
let deep = format!("{}b:c{}", "a{".repeat(192), "}".repeat(192));
assert_eq!("cancel", tabnas_css::make().parse(&deep).unwrap_err().code);
assert!(tabnas_css::parse(&deep).is_ok());
```

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

`Css::parse` has no error-recovery mode: a document either parses or
fails. The engine's tree form returns a partial stylesheet when the
engine's own recovery is on (see [the two result
forms](#the-two-result-forms)); the concepts page compares it with the
canonical port's.

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
or a value. That is the whole reason the engine passes the matcher the
rule that is reading.

## Errors

`Css::parse` raises two codes, both inherited from the engine rather
than declared here.

| `code` | Raised when |
|---|---|
| `unterminated_comment` | a `/*` has no closing `*/` |
| `unexpected` | no grammar alternative matches the next token |

The engine's tree form raises a third, `cancel`, for nesting past
[`TREE_RULE_DEPTH`](#const-tree_rule_depth-usize).

```rust
assert_eq!("unterminated_comment", tabnas_css::parse("/*").unwrap_err().code);
assert_eq!("unexpected", tabnas_css::parse("}").unwrap_err().code);
```

`line` and `column` say where the parse stopped. `message` is the
engine's own text and is meant for a person, so branch on `code` rather
than on text.
