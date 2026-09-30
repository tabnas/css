# Concepts (Rust)

How this parser is built, why it is shaped the way it is, and where it
departs from the canonical TypeScript version. For what the API does,
read the [reference](reference.md); for a first parse, the
[tutorial](tutorial.md).

## A grammar plugin on a shared engine

`@tabnas/css` is a grammar plugin, in all three ports. In Rust it runs
on two crates: the tabnas engine, `tabnas-parser`, a rule-based parser
over a matcher-based lexer, and `tabnas-jsonic`, the relaxed-JSON
grammar whose fixed punctuation tokens (`{`, `}`, `:`) and machinery
the CSS grammar reuses. The engine supplies the machinery; the plugin
supplies the CSS.

The rules come from `css-grammar.jsonic`, the file the other two ports
embed, copied in verbatim. The plugin reads it with jsonic, once per
process, and installs the rules with the canonical option overrides:
jsonic's own rules excluded, `;` remapped onto the member separator,
`[` and `]` dropped as tokens, the string, number, text and value
matchers off, and `/* */` the only comment. A lex matcher of its own,
`cssToken`, owns everything else.

So `{a:1}` is refused here for the reason it is refused in TypeScript
and Go: jsonic's leniency is switched off, and the shared leniency
fixtures prove it stayed off. The Rust suite also runs every shared
fixture with the plugin on a bare engine, with no jsonic at all, and
gets the same verdicts.

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
in `Css::parse` that collects failures and carries on. Nodes are plain
data.

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

**The lexer** owns all non-fixed text. The `cssToken` matcher runs at
order 100000, ahead of every builtin matcher, and emits, by position:
one selector or one property name (`#TX`), a top-level selector-group
comma (`#GC`), a declaration value (`#VL`), a comment at a list position
(`#CC`), and the four at-rule kinds (`#ATR`, `#ATD`, `#ATK`, `#ATS`),
which carry the keyword and the prelude together. What it declines falls
through to the engine's builtin matchers: `{`, `}`, `:` and `;` lex as
fixed punctuation, and a comment away from a list position is skipped
as whitespace.

**The engine** runs the rule table. A rule has an `open` phase and a
`close` phase, each with an ordered list of alternates. An alternate
matches when the next tokens are its token sequence, and then its
actions run, some of the matched tokens are pushed back for whatever
reads next, and the rule either pushes a child rule, replaces itself,
or closes.

The actions are the whole of the AST construction. Constructors
(`@cssRule`, `@cssDecl`, `@cssComment` and the at-rule ones) install a
fresh typed node; setters (`@cssSelector`, `@cssDeclVal`) fill its
fields; pushers (`@cssPushRule`, `@cssPushDecl`, `@cssPushKf`) append a
finished child to a parent's list. The rule shape reads like the
language: `stylesheet` to `items` to `statement`, and for a style rule
`sel` then `declbody` to `decls` to `decl` to `declval`.

## Lazy lookahead is behaviour

The engine reads a token only when an alternate being tried needs one,
under the rule trying it, and within an alternate only while it still
matches. That sounds like an optimisation. It decides the tree.

A comment right after `{` becomes a node because the block wrapper's
empty-block lookahead reads the first body token under the wrapper's own
name, and the wrapper is one of the rules at which the matcher makes a
comment a node. A comment between a property and its colon is read
under the rule that builds a declaration, which is not, so it is
skipped. Same characters, different rule asking, different tree.

The second half matters as much. Given `b{,/*!important`, the rule that
builds a block member tries its `#TX #CL` alternate and fails on the
first token, so the unterminated comment behind it is never read and the
parse fails as `unexpected`. Reading the whole sequence up front would
read it and fail as `unterminated_comment` instead: a different error
code for the same document.

## A bad token behind a good one

Lazy lookahead has one case where the two engines part company. The
canonical engine throws a bad token, such as an unclosed comment, the
moment the lexer produces it. This engine keeps it in the lookahead,
and an error with no matching alternate takes its code and position
from the first lookahead token only.

So in `a{b\"x;"/*`, where the escaped quote hides the comment from the
property scan, the unclosed comment is the second token of the
declaration alternate, and left alone the error would be `unexpected`
at the property rather than `unterminated_comment`. The plugin's lex
subscriber closes that gap: when a bad token arrives behind an
unconsumed good one, it drops the unconsumed lookahead, so the bad token
comes first and is the one the error reports, as the throw reports it.
It does so only with the engine's recovery and relexing off; in those
modes the lookahead is left as the engine keeps it. The shared comment
fixtures pin the reported code for inputs of this shape.

## Node ownership

In the canonical port a node constructor rebinds `r.node`, and a child
rule inherits its parent's node object by reference, so a selector
pushed three rules down lands in the parent's node.

The Rust engine shares one node cell, an `Rc<RefCell<…>>`, between a
rule and the rules it pushes or replaces into. A constructor that wrote
into the cell it was handed would therefore overwrite its parent's node.
So the constructors install a fresh cell, which is what rebinding a name
does in JavaScript, and the setters and pushers write through the
shared cell, as the canonical ones write through the shared object.

## Two node stores

A node lives in one of two stores, chosen per parse.

**The tree**, for a parse through the engine's own API (`make()`,
`plugin()` on an engine of yours, `Css::tabnas()`). Each cell holds the
node itself, an engine object, and a finished child moves into its
parent's list. The engine returns the stylesheet, the canonical port's
plain objects value for value, and with the engine's recovery on it
returns the partial stylesheet, as the canonical port does. That is
what a plugin user expects to get back.

**The arena**, for `Css::parse`, which asks for it with a meta key.
Every node is one flat record in a per-parse list, a cell holds the
record's id, and a child list holds ids. The engine never holds a nested
value. Every child's constructor runs after its parent's, so a child's
id is always greater, and walking the list from the last record to the
first finds each child already built and moves it into place. That
walk builds this crate's `Value` without recursion.

The two stores exist because of what the engine's `Value` does with a
deep tree: it drops, clones, compares and prints by recursion, one stack
frame per level. The arena never gives it a deep tree. The tree form
does, so a parse guard named `depth` stops it at `TREE_RULE_DEPTH` (768)
open rules: 191 nested style rules or 256 nested `@media` blocks, a
depth whose tree survives all of those operations on a 2 MiB thread in
a debug build. The canonical port has no such bound, and neither does
`Css::parse`, so it is a divergence of this port, registered as one.

## Depth is bounded by memory, not by the stack

The engine keeps its rule stack as data rather than recursing, and
`Css::parse` keeps its nodes flat, so a stylesheet nested twenty
thousand rules deep parses on a 2 MiB thread. The tree it produces is as
deep as the source, so nothing reachable from a parse result recurses
per level either. Dropping a `Value`, cloning one, comparing two,
writing one as JSON, and formatting one for `Debug` are all explicit
stack machines, and none of the five is derived. `Debug` is the one that
is easy to forget, because it is not part of the parse at all: it is
what a caller reaches for while looking at a tree, which makes it the
likeliest of the five to meet a hostile one.

What depth costs is memory. Each open level holds the engine's frames
for its rules, and measured in a release build that is about 6.8 KB per
nested style rule: 100,000 of them (1.2 MB of CSS) peaked at 678 MB.
Length costs memory too, far less per item: 100,000 flat rules (1.7 MB)
peaked at 210 MB. A host that parses untrusted CSS should cap the
input's size.

Length does not cost depth. Every repetition in the grammar, the items
of a stylesheet or a block, the declarations of a rule, the selectors of
a group, is a replace loop: the rule that reads one item replaces itself
with the reader of the next, in the same frame, rather than pushing a
new one. So the rule stack follows a stylesheet's nesting and never its
length, and a test holds each repetition, over ten thousand items, to
the depth one item needs.

## The rule history bound

When a rule replaces itself, the engine links the new rule to the one it
replaced (its `prev`), and by default keeps the whole chain. For a
replace loop that chain is every item of the list, all reachable until
the list closes. The grammar's options set `rule.history` to 1, so a
rule keeps a link to the one it replaced and to no further back. Measured
on 100,000 flat rules in a release build, the peak is 927 MB with the
chain unbounded and 210 MB with the bound.

The canonical options do not set it, and it changes no result: no
alternate in this grammar reads `prev`.

## Why reuse one parser

The plugin reads `css-grammar.jsonic` once per process. Building a
`Css` still builds a jsonic engine and installs the rules, the matcher
and the actions on it, which costs more than parsing a small stylesheet
does, so a loop that builds one per document spends most of its time on
the setup. `tabnas_css::parse` builds one process-wide parser on first
use for exactly this reason, and a caller who needs options should hold
a `Css` rather than call `parse_with` in a loop. A `Css` is immutable
after construction, so sharing one behind a reference across threads
needs no lock.

The engine's generality has a price per parse, too. Measured in a
release build, 100,000 flat rules took 1.71 s to parse and write as
JSON; the `tabnas-css` 0.5.9 on crates.io, which carries a parser
written for this grammar alone, took 0.40 s. A callgrind profile puts
about 85% of the work in the engine's own parse loop.

## Differences from the TypeScript version

TypeScript is canonical. Where this port and the canonical one disagree,
the canonical one is right, and these are the places where the two are
deliberately not identical.

### API shape

TypeScript installs the plugin on an engine
(`new Tabnas().use(jsonic).use(Css)`) and returns plain JavaScript
objects. Rust has the same form, `tabnas_jsonic::make()` with
`use_plugin(tabnas_css::plugin(), …)`, or `tabnas_css::make()`, which
returns the engine's `tabnas::Value`. It adds `parse`, `parse_with` and
a `Css` value, which return `Result<Value, Error>` rather than throwing.
Options are a struct with two `bool` fields rather than an object with
optional properties, so both are always present and both default to
`false`.

### AST representation

JavaScript has one universal object type and Rust does not, so the tree
from `Css::parse` is a `Node`, an insertion-ordered map, with `Value`
for what a field can hold. Insertion order is kept because the AST is
usually read back as JSON and `{"type": …}` first reads better than a
sorted map. Nothing depends on it: the conformance runners compare key
sets.

`Value::Undefined` exists to reproduce one JavaScript behaviour exactly.
The canonical port writes `end: undefined` into a `position` and fills
it in later, and `JSON.stringify` omits a key whose value is
`undefined`. A node whose end is never recorded therefore serialises
with a `start` and no `end`, and `Undefined` is how a key can exist in
order and still serialise to nothing.

The engine's own writer prints a whole number as `1.0`, so the tree
form's JSON carries `"line":1.0` where the canonical port writes
`"line":1`. `Value::to_json` writes what `JSON.stringify` writes, byte
for byte.

### Trimming is ECMAScript's, not Unicode's

Selectors, values and at-rule preludes are trimmed. The canonical port
trims with JavaScript `String.prototype.trim`, whose set is ECMAScript
WhiteSpace plus LineTerminator; `str::trim` uses the Unicode
`White_Space` property. They differ by exactly two code points, and both
differences change the tree.

U+FEFF, the byte-order mark, is ECMAScript whitespace and is not Unicode
`White_Space`, so a stylesheet that opens with one yields the selector
`a` rather than a selector with an invisible character on the front.
U+0085, next line, is the reverse: Unicode calls it whitespace and
ECMAScript does not, so it stays where the author put it. This crate
carries `es_trim` and `es_is_whitespace` and uses them at every site
that produces text for the tree, including the `@custom-media` split,
where a JavaScript regex matches the same set.

### Columns are UTF-16 code units

A column here counts UTF-16 code units, which is what a JavaScript
string index counts, rather than bytes or characters. For `#©{…}` a byte
count is wrong, and for `#𝄞{…}` a character count is wrong. The
conformance fixtures pin both.

The engine counts a column in Unicode scalars, from the last point it
resets the count: a `\n` in any token, or the end of a run of line
characters. The canonical port counts from the same points, in UTF-16
code units, so the two differ by the astral characters in between, the
ones two code units wide. The plugin corrects for them in two places.
For a position, it builds a sorted list of the source's astral characters on
the first position a parse records, and adds the count between the reset
and the token, found by binary search; a source with no astral character
pays nothing. For an error, `Error::from` counts the astral characters
between the reset and the error. The engine's own errors keep the scalar
count, so a caller of the tree form converts with `Error::from`.

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
port counts it. `@host\` is six characters, and its `host` node ends at
column seven inside a stylesheet ending at column eight. The engine will
not move its cursor past the end of the source, so the matcher records
the overshoot in the parse's context, and the plugin's lex subscriber
adds it to the column of the end-of-source token, the only token that
can follow it.

### Single-sourced grammar

`css-grammar.jsonic` is copied verbatim into all three ports by one
embed script, and all three read it with jsonic on the engine. A test
compares the Rust copy against the file, so that an embed nobody re-ran
cannot ship.

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
