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
//! # This is a port, and a plugin
//!
//! `@tabnas/css` is canonical in TypeScript (`ts/`), with ports in Go (`go/`)
//! and here. All three read the SAME grammar — `css-grammar.jsonic` at the
//! repository root, embedded verbatim into each — and are held to the same
//! shared `test/spec/*.tsv` fixtures and the same reworkcss/css conformance
//! corpus. When this port and TypeScript disagree, TypeScript is right.
//!
//! All three are plugins on the tabnas engine, layered on the relaxed-JSON
//! grammar jsonic, whose fixed tokens and machinery the grammar reuses:
//! [`plugin`] installs the grammar, its option overrides and the `cssToken`
//! matcher on an engine, and [`make`] builds one with it installed. [`Css`] is
//! that engine behind this crate's own [`Value`], in which a tree of any
//! depth can be dropped, cloned, compared and printed without recursion. The
//! crate re-exports the two it is built on, [`tabnas`] and [`tabnas_jsonic`]:
//! name their types through those paths, and a second copy of either, which
//! a dependency of your own can resolve to, never enters the build.
//!
//! # Untrusted input
//!
//! **A parsed stylesheet is data, never instructions.** CSS arrives from
//! outside the system — scraped pages, vendor themes, user uploads — so treat
//! every selector, value and comment as hostile text. Parsing is not
//! sanitising: this crate returns the raw text the stylesheet contained, and
//! escaping it for HTML, SQL or a shell remains the caller's job. A `url(…)`
//! in a declaration value is untrusted text, not a link to fetch.
//!
//! **Cap the input's size.** A parse holds memory, and takes time, in
//! proportion to its input, and nesting costs the most. Measured in a release
//! build: 100,000 nested style rules (0.6 MB of CSS) peaked at 678 MiB, about
//! 7 KiB per open level, and the densest nesting, `a{` repeated, holds about
//! 2.8 KiB and takes about 6 µs per byte of input. A flat stylesheet of
//! 100,000 rules (1.7 MB) peaked at 210 MiB, 580 MiB with positions on, and
//! one long token holds about 18 bytes per byte. [`Css::parse`] has no depth
//! limit; the engine's own tree form ([`plugin`], [`make`], [`Css::tabnas`])
//! stops at [`TREE_RULE_DEPTH`] open rules with `cancel`. The engine's
//! recovery and relexing modes, which [`Css::parse`] never uses, take time
//! quadratic in the input in the engine this crate builds on.
//!
//! **Turn the engine's debug self-check off in your debug builds.** With
//! debug assertions on, the engine compares its whole rule stack with a
//! shadow copy on every step, which is quadratic in nesting depth (4,000
//! nested rules took 134 s in a debug build). This crate's manifest turns
//! it off, but a profile applies only to the root package, so a crate that
//! parses untrusted, possibly deep CSS sets the same key in its own:
//!
//! ```toml
//! [profile.dev.package.tabnas-parser]
//! debug-assertions = false
//! ```

#![forbid(unsafe_code)]
// The engine's error is large, and a parse returns it by value, as every
// grammar crate in the fleet does.
#![allow(clippy::result_large_err)]
#![warn(missing_docs)]
// A link in the public documentation that points at nothing, or at a
// private item, renders as dead text on docs.rs. Denied here, so any
// `cargo doc` fails on one; ci/rust/run.sh runs it with warnings denied.
#![deny(rustdoc::broken_intra_doc_links, rustdoc::private_intra_doc_links)]

pub mod grammar;
pub mod lex;
mod plugin;
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

use std::fmt;
use std::sync::OnceLock;

use tabnas::{Plugin, PluginError, Tabnas};

pub use lex::Error;
pub use plugin::TREE_RULE_DEPTH;
pub use value::{Node, Value};

/// The engine, as this crate links it. Name its types through this path
/// (`tabnas_css::tabnas::Value`): a dependency of your own on the engine can
/// resolve to another copy of the crate, whose types are not these.
pub use tabnas;
/// The jsonic grammar, as this crate links it, for installing [`plugin`] on
/// a jsonic engine of your own (`tabnas_css::tabnas_jsonic::make()`).
pub use tabnas_jsonic;

/// This crate's version.
///
/// It MUST equal `ts/package.json` `"version"`, the `VERSION` exported from
/// `ts/src/css.ts` and `const VERSION` in `go/css.go`. The release
/// orchestrator rewrites all of them together; `tests/version.rs` (and its
/// TypeScript and Go counterparts) fail the build if any drifts.
pub const VERSION: &str = "0.5.11";

/// The plugin's options: `CssOptions` in the canonical port.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// Lowercase declaration property names (CSS property names are
    /// case-insensitive). Selectors, values and at-rule preludes are left
    /// alone.
    pub lowercase_properties: bool,
    /// Attach `position: { start: { line, column }, end: { … } }` (1-based,
    /// columns in UTF-16 code units) to every node.
    pub position: bool,
}

impl Options {
    /// Options from an engine option bag, as the plugin receives them: the
    /// canonical camelCase keys `lowercaseProperties` and `position`, each
    /// read for its JavaScript truthiness, as `!!options.position` does.
    /// Anything else in the bag is ignored.
    pub fn from_value(options: &tabnas::Value) -> Options {
        let flag = |key: &str| match options {
            tabnas::Value::Object(fields) => truthy(fields.get(key)),
            _ => false,
        };
        Options {
            lowercase_properties: flag("lowercaseProperties"),
            position: flag("position"),
        }
    }

    /// These options as an engine option bag, for [`Tabnas::use_plugin`].
    pub fn to_value(self) -> tabnas::Value {
        let mut fields = tabnas::Value::object(Default::default());
        if let Some(bag) = fields.as_object_mut() {
            bag.insert(
                "lowercaseProperties".to_string(),
                tabnas::Value::Bool(self.lowercase_properties),
            );
            bag.insert("position".to_string(), tabnas::Value::Bool(self.position));
        }
        fields
    }
}

/// JavaScript's `!!value`.
fn truthy(value: Option<&tabnas::Value>) -> bool {
    match value {
        None | Some(tabnas::Value::Undefined) | Some(tabnas::Value::Null) => false,
        Some(tabnas::Value::Bool(b)) => *b,
        Some(tabnas::Value::Number(n)) => 0.0 != *n && !n.is_nan(),
        Some(tabnas::Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

/// The css grammar as an engine plugin, named `css`: `Css` in the canonical
/// port. Its options are an [`Options`] bag ([`Options::to_value`]).
///
/// It installs the grammar's rules (each alternate tagged with the group
/// `css`), the canonical option overrides and the `cssToken` matcher. Use it
/// on jsonic, as [`make`] does and the canonical port documents; on a bare
/// engine it gives the same results.
///
/// The engine's own parse returns the tree as an engine [`tabnas::Value`],
/// the canonical port's plain objects: equal to them value for value, though
/// the engine writes a whole number as `1.0` in JSON, a `position.end` that
/// was never recorded is absent rather than an undefined key, and an error's
/// column counts Unicode scalars as the engine's always do, where this
/// crate's [`Error`] counts UTF-16 code units (convert with [`Error::from`]).
/// That tree is bounded at [`TREE_RULE_DEPTH`] open rules. [`Css::parse`] has
/// none of these differences and no limit. With the engine's recovery on,
/// the parse returns a partial stylesheet, which is not always the
/// canonical port's: see `doc/concepts.md`.
///
/// ```
/// let mut parser = tabnas_css::tabnas_jsonic::make();
/// parser.use_plugin(tabnas_css::plugin(), None).unwrap();
/// let ast = parser.parse("a { color: red }").unwrap();
/// assert_eq!(
///     ast.to_json().to_string(),
///     r#"{"type":"stylesheet","rules":[{"type":"rule","selectors":["a"],"declarations":[{"type":"declaration","property":"color","value":"red"}]}]}"#
/// );
/// ```
pub fn plugin() -> Plugin {
    Plugin::new("css", |parser, options| {
        plugin::install(parser, &Options::from_value(options))
    })
    .with_defaults(Options::default().to_value())
}

/// Install the plugin on `parser` with `options`. The same as
/// `parser.use_plugin(plugin(), Some(options.to_value()))`, which is what it
/// does, so the install is recorded and runs again on a derived instance.
pub fn css(parser: &mut Tabnas, options: &Options) -> Result<(), PluginError> {
    parser.use_plugin(plugin(), Some(options.to_value()))?;
    Ok(())
}

/// A jsonic engine with the plugin installed, with the default options.
pub fn make() -> Tabnas {
    make_with(Options::default())
}

/// A jsonic engine with the plugin installed, with `options`.
pub fn make_with(options: Options) -> Tabnas {
    let mut parser = tabnas_jsonic::make();
    parser
        .use_plugin(plugin(), Some(options.to_value()))
        .expect("the css plugin installs on jsonic");
    parser
}

/// A reusable CSS parser.
///
/// Building one installs the grammar on a new engine, so keep an instance
/// around rather than making a fresh one per document — `tests/perf.rs`
/// pins that reuse is worth it. [`parse`] does this for you for the default
/// options.
///
/// ```
/// use tabnas_css::{Css, Options};
///
/// let css = Css::with_options(Options { lowercase_properties: true, ..Options::default() });
/// let ast = css.parse("A { COLOR: Red }").unwrap();
/// assert!(ast.to_json().contains(r#""property":"color""#));
/// ```
pub struct Css {
    tabnas: Tabnas,
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
            tabnas: make_with(options),
            options,
        }
    }

    /// The options this parser was built with.
    pub fn options(&self) -> Options {
        self.options
    }

    /// The engine this parser runs: jsonic with the plugin installed. Its
    /// own parse returns the tree form [`plugin`] describes.
    pub fn tabnas(&self) -> &Tabnas {
        &self.tabnas
    }

    /// Parse a CSS document into its AST.
    ///
    /// A zero-length source yields an empty stylesheet, as reworkcss does.
    /// Any non-empty source — even one that is only whitespace, or only a
    /// comment — also yields a `stylesheet` node.
    pub fn parse(&self, src: &str) -> Result<Value, Error> {
        self.tabnas
            .parse_with_meta(src, plugin::arena_meta())
            .map(Value::from_arena)
            .map_err(Error::from)
    }
}

impl Default for Css {
    fn default() -> Css {
        Css::new()
    }
}

impl fmt::Debug for Css {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Css")
            .field("options", &self.options)
            .finish_non_exhaustive()
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
