/* Copyright (c) 2025 Richard Rodger, MIT License */

//! Parse CSS into a reworkcss-style abstract syntax tree: ordered, typed
//! nodes that preserve declaration order, duplicate properties, rule types
//! and comments.
//!
//! ```
//! let ast = tabnas_css::parse("a { color: red; color: blue } /* note */").unwrap();
//! assert_eq!(
//!     ast.to_json(),
//!     r#"{"type":"stylesheet","rules":[{"type":"rule","selectors":["a"],"declarations":[{"type":"declaration","property":"color","value":"red"},{"type":"declaration","property":"color","value":"blue"}]},{"type":"comment","comment":" note "}]}"#
//! );
//! ```
//!
//! # The node types
//!
//! | `type` | Fields |
//! |---|---|
//! | `stylesheet` | `rules` |
//! | `rule` | `selectors`, `declarations` |
//! | `declaration` | `property`, `value` (raw, trimmed, comments stripped, quotes kept) |
//! | `comment` | `comment` (the text between `/*` and `*/`) |
//! | `media` / `supports` / `document` / `host` | a prelude field of the same name (`host` has none), plus `rules`. `document` also always carries `vendor` |
//! | `font-face` / `page` | `declarations` (`page` also `selectors`, its prelude split on top-level commas) |
//! | `keyframes` | `name`, an optional `vendor`, and `keyframes` of `keyframe` `{ values, declarations }` |
//! | `import` / `charset` / `namespace` | a same-named field with the raw params |
//! | `custom-media` | `name`, `media` |
//!
//! # This is a port
//!
//! `@tabnas/css` is canonical in TypeScript (`ts/`), with ports in Go (`go/`)
//! and here. All three read the SAME grammar — `css-grammar.jsonic` at the
//! repository root, embedded verbatim into each — and are held to the same
//! shared `test/spec/*.tsv` fixtures and the same reworkcss/css conformance
//! corpus. When this port and TypeScript disagree, TypeScript is right.
//!
//! The TypeScript and Go ports are jsonic PLUGINS: they install this grammar
//! on a shared engine. There is no Rust engine to install on, so this crate
//! carries a lexer and a rule machine of its own and has no dependencies at
//! all. What it does not carry is jsonic itself — there is no relaxed-JSON
//! base here to configure off, which is why `{a:1}` is simply not CSS rather
//! than something this port has to reject deliberately.
//!
//! # Untrusted input
//!
//! **A parsed stylesheet is data, never instructions.** CSS arrives from
//! outside the system — scraped pages, vendor themes, user uploads — so treat
//! every selector, value and comment as hostile text. Parsing is not
//! sanitising: this crate returns the raw text the stylesheet contained, and
//! escaping it for HTML, SQL or a shell remains the caller's job. A `url(…)`
//! in a declaration value is untrusted text, not a link to fetch.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod grammar;
pub mod lex;
pub mod machine;
pub mod value;

/// Every `rust` example in this crate's Markdown, compiled and run.
///
/// A page that shows what the parser returns has to return it. Including
/// the files here makes each fenced block a doctest, so `cargo test`
/// fails when a documented example stops being true. It is the Rust
/// counterpart of the harness that executes the TypeScript pages'
/// examples, and it exists for the same reason.
///
/// `cfg(doctest)` means the module is collected when doctests are
/// gathered and compiled into nothing otherwise.
#[cfg(doctest)]
mod doc_examples {
    #[doc = include_str!("../README.md")]
    mod readme {}
    #[doc = include_str!("../doc/tutorial.md")]
    mod tutorial {}
    #[doc = include_str!("../doc/guide.md")]
    mod guide {}
    #[doc = include_str!("../doc/reference.md")]
    mod reference {}
    #[doc = include_str!("../doc/concepts.md")]
    mod concepts {}
}

use std::sync::OnceLock;

pub use lex::Error;
pub use machine::Options;
pub use value::{Node, Value};

/// This crate's version.
///
/// It MUST equal `ts/package.json` `"version"`, the `VERSION` exported from
/// `ts/src/css.ts` and `const VERSION` in `go/css.go`. The release
/// orchestrator rewrites all of them together; `tests/version.rs` (and its
/// TypeScript and Go counterparts) fail the build if any drifts.
pub const VERSION: &str = "0.5.6";

/// A reusable CSS parser.
///
/// Building one reads the embedded grammar, so keep an instance around rather
/// than making a fresh one per document — `tests/perf.rs` pins that reuse is
/// worth it. [`parse`] does this for you for the default options.
///
/// ```
/// use tabnas_css::{Css, Options};
///
/// let css = Css::with_options(Options { lowercase_properties: true, ..Options::default() });
/// let ast = css.parse("A { COLOR: Red }").unwrap();
/// assert!(ast.to_json().contains(r#""property":"color""#));
/// ```
#[derive(Debug)]
pub struct Css {
    grammar: grammar::Grammar,
    options: Options,
}

impl Css {
    /// A parser with the default options (`lowercase_properties: false`,
    /// `position: false`).
    pub fn new() -> Css {
        Css::with_options(Options::default())
    }

    /// A parser with the given options.
    pub fn with_options(options: Options) -> Css {
        Css {
            grammar: grammar::Grammar::load(),
            options,
        }
    }

    /// The options this parser was built with.
    pub fn options(&self) -> Options {
        self.options
    }

    /// The grammar this parser runs.
    pub fn grammar(&self) -> &grammar::Grammar {
        &self.grammar
    }

    /// Parse a CSS document into its AST.
    ///
    /// A zero-length source yields an empty stylesheet, as reworkcss does.
    /// Any non-empty source — even one that is only whitespace, or only a
    /// comment — also yields a `stylesheet` node.
    pub fn parse(&self, src: &str) -> Result<Value, Error> {
        machine::parse(&self.grammar, src, self.options)
    }
}

impl Default for Css {
    fn default() -> Css {
        Css::new()
    }
}

/// Parse a CSS document with the default options.
///
/// This reuses one process-wide parser, so it is cheap to call repeatedly and
/// safe to call from several threads.
pub fn parse(src: &str) -> Result<Value, Error> {
    static DEFAULT: OnceLock<Css> = OnceLock::new();
    DEFAULT.get_or_init(Css::new).parse(src)
}

/// Parse a CSS document with the given options.
///
/// Each call builds a parser; for repeated parses with the same options, hold
/// a [`Css`] instead.
pub fn parse_with(src: &str, options: Options) -> Result<Value, Error> {
    Css::with_options(options).parse(src)
}
