# Concepts (Rust)

How this parser is built, why it is shaped the way it is, and where it
departs from the canonical TypeScript version. For what the API does,
read the [reference](reference.md); for a first parse, the
[tutorial](tutorial.md).

## A grammar without a shared engine

`@tabnas/css` is a grammar plugin. In TypeScript and in Go it installs a
rule table on the tabnas parsing engine, reuses that engine's fixed
punctuation tokens, switches the engine's relaxed-JSON value matchers
off, and supplies a lexer of its own for everything else. The engine
supplies the machinery; the plugin supplies the CSS.

There is no Rust build of that engine. This crate therefore carries the
machinery as well as the CSS: a lexer, a rule machine, and a small
reader for the grammar file. What it does not carry is a second opinion
about the grammar. The rules themselves come from the same
`css-grammar.jsonic` the other two ports embed, copied in verbatim, and
a test compares the copy against the file so that an embed nobody re-ran
cannot ship.

That has one consequence a caller can see, and it is a pleasant one.
The other ports sit on a relaxed-JSON dialect and have to switch its
leniency off, then prove it stayed off, so that `{a:1}` is rejected.
Here there is no dialect underneath: `{a:1}` is rejected because it is
not CSS, which is the same verdict for a simpler reason.

## The output model: a reworkcss-style AST

The tree follows the [`reworkcss/css`](https://github.com/reworkcss/css)
model, which is the shape most CSS tooling already understands: ordered,
typed nodes that preserve declaration order, duplicate properties, rule
types and comments.

```rust
let ast = tabnas_css::parse("a { color: red; color: blue } /* note */").unwrap();
assert_eq!(
    ast.to_json(),
    r#"{"type":"stylesheet","rules":[{"type":"rule","selectors":["a"],"declarations":[{"type":"declaration","property":"color","value":"red"},{"type":"declaration","property":"color","value":"blue"}]},{"type":"comment","comment":" note "}]}"#
);
```

Three properties of that model matter more than the others:

- **Order is meaning.** The cascade depends on declaration order, so
  nothing is reordered and nothing is merged. Two `color` declarations
  are two nodes.
- **Values are text.** A declaration value is the raw run up to the next
  top-level `;` or `}`, trimmed, with comments removed. It is not parsed
  further, because CSS values have no single grammar and a caller who
  needs one has a better idea of which.
- **Comments are nodes.** The comments in a stylesheet carry licence
  headers and section markers, and a tool that rewrites CSS has to keep
  them.

What the model deliberately leaves out is as important. There is no
`parent` back-reference, no source filename, and no error-recovery mode
that collects failures and carries on. Nodes are plain data.

## CSS structure is supplied, not inherited

CSS is context-sensitive. The characters `a:hover` are a selector before
a `{` and a property with a value before a `;`, and nothing local
distinguishes them. A conventional token stream cannot decide, so the
decision is pushed into the lexer, which is given the rule that is
asking.

The lexer reads ahead to the next top-level `{`, `;` or `}` and answers
one question: does a `{` come first? If it does, the run is a selector
or an at-rule prelude. If a `;` or `}` comes first, the run is a
property name or a statement at-rule's params. Strings, `()`, `[]`,
comments and `\` escapes are skipped during that scan, so a `{` inside
a string never decides anything.

The escape skip is not a nicety. Without it a selector such as `#f\'o\'o`
reads as an unterminated string, and `.\3A \`\(` as an unbalanced
parenthesis, and the whole rule fails to lex.

## The mechanisms

A parse has two halves.

**The lexer** owns all non-fixed text. One matcher emits, by position:
one selector or one property name (`#TX`), a top-level selector-group
comma (`#GC`), a declaration value (`#VL`), a comment at a list position
(`#CC`), and the four at-rule kinds (`#ATR`, `#ATD`, `#ATK`, `#ATS`),
which carry the keyword and the prelude together. What it declines falls
through: `{`, `}`, `:` and `;` lex as fixed punctuation, and a comment
away from a list position is skipped as whitespace.

**The rule machine** runs the rule table. A rule has an `open` phase and
a `close` phase, each with an ordered list of alternatives. An
alternative matches when the next tokens are its token sequence, and
then its actions run, some of the matched tokens are pushed back for
whatever reads next, and the rule either pushes a child rule, replaces
itself, or closes.

The actions are the whole of the AST construction. Constructors
(`@cssRule`, `@cssDecl`, `@cssComment` and the at-rule ones) install a
fresh typed node; setters (`@cssSelector`, `@cssDeclVal`) fill its
fields; pushers (`@cssPushRule`, `@cssPushDecl`, `@cssPushKf`) append a
finished child to a parent's array. The rule shape reads like the
language: `stylesheet` to `items` to `statement`, and for a style rule
`sel` then `declbody` to `decls` to `decl` to `declval`.

## Lazy lookahead is behaviour

Tokens are read only when an alternative being tried needs one, under
the rule trying it, and within an alternative only while it still
matches. That sounds like an optimisation. It decides the tree.

A comment right after `{` becomes a node because the block wrapper's
empty-block lookahead reads the first body token under the wrapper's own
name, and the wrapper is one of the rules at which a comment is a node.
A comment between a property and its colon is read under the rule that
builds a declaration, which is not, so it is skipped. Same characters,
different rule asking, different tree.

The second half matters as much. Given `b{,/*!important`, the rule that
builds a block member tries its `#TX #CL` alternative and fails on the
first token, so the unterminated comment behind it is never read and the
parse fails as `unexpected`. Reading the whole sequence up front would
read it and fail as `unterminated_comment` instead: a different error
code for the same document.

## Node ownership

In the canonical ports a child rule inherits its parent's node by
reference, so a selector pushed three rules down lands in the parent's
node. Rust has no such aliasing without interior mutability, and reaching
for `Rc<RefCell<_>>` here would buy runtime borrow panics in exchange for
a shape the language does not need.

So the node moves instead. A pushed child takes the node; a constructor
hands it back before installing its own; a popping rule either returns it
or delivers its own node to the parent as the child. Only the top frame
ever runs, so the node is always where the running rule is, and the
ownership is static.

## Depth is bounded by memory, not by the stack

The rule machine keeps its own stack rather than recursing, so a
stylesheet nested twenty thousand rules deep parses. That promise has a
second half that is easy to miss: the tree it produces is as deep as the
source, so a derived drop would recurse once per level and abort the
process the moment the value went out of scope. Dropping a `Value` and
writing one as JSON are both iterative for that reason.

Cloning and comparing are not. They walk the tree the way the derived
implementations do, which is fine for any stylesheet a person wrote and
worth knowing before cloning one an attacker sent.

## Why reuse one parser

Building a `Css` reads and parses the grammar. That costs more than
parsing a small stylesheet does, so a loop that builds one per document
spends most of its time on the grammar. `tabnas_css::parse` builds one
process-wide parser on first use for exactly this reason, and a caller
who needs options should hold a `Css` rather than call `parse_with` in a
loop. A `Css` is immutable after construction, so sharing one behind a
reference across threads needs no lock.

## Differences from the TypeScript version

TypeScript is canonical. Where this port and the canonical one disagree,
the canonical one is right, and these are the places where the two are
deliberately not identical.

### API shape

TypeScript installs a plugin on an engine
(`new Tabnas().use(jsonic).use(Css)`) and returns plain JavaScript
objects. Rust exposes `parse`, `parse_with` and a `Css` value, and
returns `Result<Value, Error>` rather than throwing. Options are a
struct with two `bool` fields rather than an object with optional
properties, so both are always present and both default to `false`.

### AST representation

JavaScript has one universal object type and Rust does not, so the tree
is a `Node`, an insertion-ordered map, with `Value` for what a field can
hold. Insertion order is kept because the AST is usually read back as
JSON and `{"type": …}` first reads better than a sorted map. Nothing
depends on it: the conformance runners compare key sets.

`Value::Undefined` exists to reproduce one JavaScript behaviour exactly.
The canonical port writes `end: undefined` into a `position` and fills
it in later, and `JSON.stringify` omits a key whose value is
`undefined`. A node whose end is never recorded therefore serialises
with a `start` and no `end`, and `Undefined` is how a key can exist in
order and still serialise to nothing.

### Columns are UTF-16 code units

A column here counts UTF-16 code units, which is what a JavaScript
string index counts, rather than bytes or characters. For `#©{…}` a byte
count is wrong, and for `#𝄞{…}` a character count is wrong. The
conformance fixtures pin both.

### Slicing where the scanners overshoot

The scanners overshoot on purpose in one case: a `\` at the last
character of the source is an escape whose escaped character is not
there, and the two-step skip runs past the end. JavaScript clamps an
out-of-range slice bound; Rust panics on one, and panics on a bound
inside a multi-byte character as well. Every index these scanners return
is at an ASCII character or at the end of the source, so the clamping
helper is unreachable in practice. It is there so that a future change
degrades to a shorter span instead of a panic in a caller's parse.

The column arithmetic does count the overshoot, because the canonical
port counts it. `@host\` is six characters and yields a stylesheet
ending at column eight.

### Single-sourced grammar

`css-grammar.jsonic` is copied verbatim into all three ports by one
embed script. The other two read it with a jsonic engine they already
depend on. This crate has a reader for the subset the file is written in
(`#` comments, maps with bare or quoted keys, lists, and commas
optional), which is enough for that document and is not a jsonic
implementation.

## Accepted and rejected: the edge cases

| Input | Result | Why |
|---|---|---|
| `""` | empty stylesheet | matches upstream |
| `"   "` | empty stylesheet | whitespace is not a statement |
| `/* c */` | one comment node | a comment is a statement |
| `a {}` | rule with no declarations | an empty block is legal |
| `p { color:; }` | declaration with an empty value | upstream accepts it |
| `a { color: red }` | no trailing `;` needed | the block closes it |
| `//prop: 1` | a property named `//prop` | `//` is not a CSS comment |
| `{size: large}` | error | no selector |
| `/*` | error | an unclosed comment is not a comment to the end of input |
| `a { x: 1` | error | unclosed block |
| `a{c:1} extra` | error | trailing text that is not a rule |

The last four are where the reworkcss model and CSS Syntax Level 3 part
company: Level 3 recovers from all of them, and the model rejects them.
This parser follows the model, because the model is what the tree shape
comes from.
