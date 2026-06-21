# Concepts

Background on how the CSS plugin is put together, and why. This is
understanding-oriented reading — for steps see the
[tutorial](tutorial.md) and [how-to guide](guide.md), and for exact
signatures and syntax see the [reference](reference.md).

## A grammar plugin on a shared engine

The plugin has no parser of its own. It is a thin layer on a stack of
three pieces:

- the **Tabnas engine** (`@tabnas/parser`) — a rule-based parser over a
  configurable, matcher-based lexer,
- the **relaxed-JSON grammar** (`@tabnas/jsonic`) — the rules and
  helper actions (`@object$`, the `key`/`setval`/`reset`/`value` action
  set) that turn tokens into objects, and
- **this plugin** (`@tabnas/css`) — the option overrides, one custom
  lex matcher, and small grammar overlay that retune that stack to read
  CSS instead of JSON.

Because the engine is configuration-driven, CSS support is mostly an
options change plus a handful of alternates — not a new parser. The
plugin embeds the canonical grammar text (from the repo-root
`css-grammar.jsonic`) as a string, parses it with a throwaway jsonic
instance to get a grammar object, attaches its option overrides to that
object, and hands the whole thing to the engine atomically via
`tn.grammar(grammarDef, { rule: { alt: { g: 'css' } } })`.

## The output model

A stylesheet parses to a nested map of `selector → { property → value
}`:

- each **rule** is a key (the selector prelude, kept verbatim) mapping
  to a map of its declarations;
- each **declaration** is a key (the property name) mapping to its
  value as a **raw string** (e.g. `'1px solid #fff'`);
- **nested at-rules** (e.g. `@media`) recurse — the prelude is the key
  and the block is a nested map of rules;
- **statement at-rules** (e.g. `@import`) become a key (the at-keyword)
  mapping to the rest of the statement as a string.

The deliberate simplification is that values and selectors are **not**
parsed into structure. A value is the text up to the next top-level
`;` or `}`; a selector is the text up to its `{`. This keeps the output
faithful to the source and the grammar small — a consumer that needs to
pick a selector or value apart can do so on the returned strings.

## CSS is not JSON

JSON and CSS share braces but little else, so the plugin reshapes the
jsonic stack rather than accepting both:

| | JSON / jsonic | CSS |
|---|---|---|
| Top level | a single value | an implicit, brace-free map of rules |
| Map keys | quoted strings | selectors / property names (text) |
| Key/value separator | `:` | `:` (declarations) |
| Member separator | `,` | `;` |
| Bare `[` `]` | lists | disabled (only inside selectors/values) |
| Strings | `"` `'` `` ` `` | values are raw text, not lexed strings |
| Comments | `#` `//` `/* */` | `/* */` only |

The plugin makes those swaps by **disabling** what JSON allows and
**adding** what CSS needs: it remaps the member separator from `,` to
`;`, disables the `[` `]` openers, turns off jsonic's string/number/
text matchers (one custom matcher owns all text), narrows the key set,
and limits comments to `/* */`.

## The one context-sensitive matcher

The heart of the plugin is a single custom lex matcher, `cssToken`,
registered with a high `order` so it runs ahead of the fixed-token
matcher and owns all non-punctuation text. It defers (returns
`undefined`) on whitespace, on `/* */` comments, and on the fixed
punctuation (`{` `}` `:` `;`), letting the builtin matchers handle
those.

What it emits depends on whether it is at a **key** or a **value**
position, which it decides from the active rule and a per-parse flag:

- **Value mode** — selected when the `val` rule is open (just after a
  `:`) or right after a statement at-keyword. It reads the run of text
  up to the next top-level `;` or `}` and emits one `#VL` token. So
  `1px solid #fff` becomes a single value.

- **Key mode** — otherwise. It peeks ahead: if a top-level `{` is
  reached before any `;`/`}`, the text is a **selector** (the whole
  prelude, trimmed, as `#TX`); if a `;`/`}`/end-of-input is reached
  first, it is a **property name** or statement at-keyword (the
  identifier, as `#TX`). A leading `@` marks a statement at-rule, whose
  params are then read as a value on the next call.

Both scans skip over quoted strings, `/* */` comments, and balanced
`()` / `[]`, so a `;` inside `url(...)` or a `{` inside an attribute
selector does not fool the classifier.

## Selector vs property vs at-rule

The matcher's lookahead is what lets one text token, `#TX`, stand for
three different things, disambiguated in the grammar by the **next**
token:

- `#TX #CL` (`property :`) → a **declaration**; the value follows.
- `#TX #OB` (`selector {`) → a **nested ruleset**; the block follows.
- `#TX #VL` (`@keyword params`) → a **statement at-rule**; the params
  are the value.

Because the classifier ran in the lexer, the grammar only ever sees an
already-disambiguated shape and a two-token-lookahead rule set is
enough. This is also why a pseudo-class selector like `a:hover` is not
mistaken for a property named `a`: in key position a `:` is allowed
inside the prelude, and the trailing `{` proves it is a selector.

## The implicit top-level map

Unlike a JSON document, a stylesheet has no surrounding braces — it is
just a sequence of rules. The `stylesheet` start rule models this as an
implicit top-level map, seeded with `@object$`, that runs the `pair`
rule until end-of-input (`#ZZ`). Each `pair` adds one
selector/property key and resolves its value via `@setval$`. The `val`
rule resets the parent-seeded node (`@reset$`) so a value does not
inherit the enclosing object, then resolves it (`@value$`) to a built
block (a nested map) or the `#VL` scalar.

This is why a **zero-length** source returns `undefined`: with no
input, no rule ever runs, an engine convention. Any non-empty source —
even whitespace or a lone comment — runs the start rule and yields at
least an empty stylesheet `{}`.

## The case-folding options

The two options act only in the matcher, on the text it is already
about to emit:

- `lowercaseProperties` lowercases the identifier in **key mode** when
  it resolved to a property name — never a selector, never a value.
- `lowercaseValues` lowercases the text emitted in **value mode**.

`lowercaseValues` is off by default because a value can carry
case-sensitive parts — quoted strings, `url()` contents, custom
identifiers — that lowercasing would corrupt. `lowercaseProperties` is
safe because CSS property names are case-insensitive.

## Why reuse one instance

Building the CSS grammar — parsing the embedded grammar text, applying
the option overlay, wiring the custom matcher — dominates the cost of a
parse; the parse itself, on a typical small stylesheet, is cheap by
comparison. The instance is stateless across parses (each parse builds
its own context and only reads instance state), so the right pattern is
to build the engine once and reuse it for every input.

## Accepted vs rejected — edge cases

- `''` → `undefined`; `'   '` and `/* c */` → `{}`. A zero-length
  source runs no rules; any non-empty source yields a stylesheet.
- `a {}` → `{ a: {} }`. An empty block is an empty map.
- `h1, h2 { ... }` → key `'h1, h2'`. Selector grouping is kept
  verbatim.
- `a:hover { ... }` → key `'a:hover'`, not a property `a`. A `:` is
  allowed inside a selector prelude.
- `1px solid #fff` → one value string. Values are not parsed further.
- `url(http://x/y.png)` → one value; the `:` and `/` inside `()` do not
  terminate it.
- `@media ... { ... }` → recurses into a nested map of rules.
- `@import "base.css";` → `{ '@import': '"base.css"' }`; the value
  keeps its quotes. A statement at-rule must end with `;`.
- `/* ... */` → discarded; `#` and `//` are **not** comments in CSS.

## Relationship to the Go port

The plugin ships in two implementations — this TypeScript one and a Go
port — built from the same canonical `css-grammar.jsonic`. The
TypeScript version is the reference. For the Go API shape, value types,
and any accepted differences, see
[../../go/doc/concepts.md](../../go/doc/concepts.md).
