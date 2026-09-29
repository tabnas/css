/* Copyright (c) 2025 Richard Rodger, MIT License */

//! The grammar: the embedded `css-grammar.jsonic` text, and the option
//! overrides the plugin installs with it.
//!
//! `css-grammar.jsonic` at the repository root is the SINGLE SOURCE OF TRUTH
//! for the rules. `ts/embed-grammar.js` copies it verbatim into `ts/src/css.ts`,
//! `go/css.go` and the `GRAMMAR_TEXT` literal below. Never hand-edit between
//! the `BEGIN/END EMBEDDED` markers — edit the `.jsonic` file and re-run
//! `npm run embed` from `ts/`.
//!
//! All three ports read that text the same way: with jsonic, on the engine,
//! into the rule set the engine then runs. The options beside it are
//! `grammarDef.options` in `ts/src/css.ts`, written out as JSON.

use std::sync::OnceLock;

use tabnas::GrammarSpec;

use crate::value::Value;

// --- BEGIN EMBEDDED css-grammar.jsonic ---
const GRAMMAR_TEXT: &str = r##"
# CSS Grammar Definition (AST)
# Parses CSS into a reworkcss-style abstract syntax tree: ordered, typed
# nodes that preserve declaration order, duplicate properties, rule types
# and comments.
#
#   { type: 'stylesheet', rules: [ Node, ... ] }
#   rule        { type:'rule',        selectors:[string], declarations:[Node] }
#   declaration { type:'declaration', property:string, value:string }
#   comment     { type:'comment',     comment:string }
#   at-rules (media/supports/import/keyframes/font-face/...) — see below
#
# Example:
#   a { color: red; color: blue } /* note */
# parses to:
#   { type:'stylesheet', rules: [
#     { type:'rule', selectors:['a'], declarations:[
#       { type:'declaration', property:'color', value:'red' },
#       { type:'declaration', property:'color', value:'blue' } ] },
#     { type:'comment', comment:' note ' } ] }
#
# The cssToken lex matcher emits: #TX (one selector or a property name),
# #GC (a top-level selector-group comma), #VL (a declaration value),
# #CC (a comment, at a statement position), and the at-rule tokens
# #ATR/#ATD/#ATK/#ATS (carrying the keyword in val and the prelude in
# use). Fixed "{" "}" ":" lex as #OB #CB #CL; ";" is remapped to #CA.
#
# Every alt is tagged with the 'css' group via { rule: { alt: { g: 'css' } } }.

{
  rule: {
    # The top-level stylesheet node. "items" fills its rules[].
    stylesheet: {
      open: [
        { a: '@cssSheet' p: items g: 'css,sheet' }
      ]
      close: [
        { s: '#ZZ' a: '@cssEnd' g: 'css,sheet,end' }
      ]
    }

    # A statement list (the stylesheet body or an at-rule's rules body). Reads
    # rule / at-rule / comment nodes into the enclosing node's rules[]. Each
    # iteration pushes one "statement" child, then @cssPushRule appends it.
    items: {
      open: [
        { s: '#ZZ' b: 1 g: 'css,items,end' }
        { s: '#CB' b: 1 g: 'css,items,endblock' }
        { p: statement g: 'css,items' }
      ]
      close: [
        { s: '#ZZ' b: 1 a: '@cssPushRule' g: 'css,items,end' }
        { s: '#CB' b: 1 a: '@cssPushRule' g: 'css,items,endblock' }
        { a: '@cssPushRule' r: items g: 'css,items,next' }
      ]
    }

    # Builds ONE statement node (resetting from the inherited list node).
    statement: {
      open: [
        # A comment node.
        { s: '#CC' a: '@cssComment' g: 'css,comment' }
        # A block at-rule whose body is rules (e.g. @media): push items.
        { s: '#ATR' a: '@cssAtRules' p: rulesbody g: 'css,atrules' }
        # A block at-rule whose body is declarations (e.g. @font-face).
        { s: '#ATD' a: '@cssAtDecls' p: declbody g: 'css,atdecls' }
        # @keyframes: a body of keyframe blocks.
        { s: '#ATK' a: '@cssKeyframes' p: kfbody g: 'css,keyframes' }
        # A statement at-rule (e.g. @import "x"): a leaf node.
        { s: '#ATS' a: '@cssAtStmt' g: 'css,atstmt' }
        # A style rule: selectors + declarations.
        { s: '#TX' b: 1 a: '@cssRule' p: sel g: 'css,rule' }
      ]
      close: [
        { b: 1 g: 'css,statement,end' }
      ]
    }

    # Reads a selector group into the (rule/page) node's selectors[], then the
    # declaration block. Each selector is one #TX; #GC separates a group.
    sel: {
      open: [
        { s: '#TX #GC' a: '@cssSelector' r: sel g: 'css,sel,group' }
        { s: '#TX #OB' b: 1 a: '@cssSelector' p: declbody g: 'css,sel,last' }
      ]
      close: [
        { b: 1 g: 'css,sel,end' }
      ]
    }

    # The "{ ... }" wrapper around a declaration list; fills the parent node's
    # declarations[].
    declbody: {
      open: [
        { s: '#OB #CB' b: 1 g: 'css,declbody,empty' }
        { s: '#OB' p: decls g: 'css,declbody' }
      ]
      close: [
        { s: '#CB' a: '@cssEnd' g: 'css,declbody,end' }
      ]
    }

    # A declaration list: declaration / comment nodes, ";"-separated.
    decls: {
      open: [
        { s: '#CB' b: 1 g: 'css,decls,empty' }
        { p: decl g: 'css,decls' }
      ]
      close: [
        { s: '#CA #CB' b: 1 a: '@cssPushDecl' g: 'css,decls,trailing' }
        { s: '#CB' b: 1 a: '@cssPushDecl' g: 'css,decls,end' }
        { s: '#CA' a: '@cssPushDecl' r: decls g: 'css,decls,next' }
        { a: '@cssPushDecl' r: decls g: 'css,decls,comment' }
      ]
    }

    # Builds ONE block member: a declaration, a comment, or — for CSS
    # Nesting — a nested style rule or at-rule. Disambiguated by the token
    # after the key: #TX #CL is a declaration; #TX followed by "{"/"," is a
    # nested rule; #ATR/#ATD/#ATK/#ATS is a nested at-rule.
    decl: {
      open: [
        { s: '#CC' a: '@cssComment' g: 'css,comment' }
        { s: '#TX #CL' a: '@cssDecl' p: declval g: 'css,decl' }
        # Nested at-rules (CSS Nesting).
        { s: '#ATR' a: '@cssAtRules' p: rulesbody g: 'css,nest,atrules' }
        { s: '#ATD' a: '@cssAtDecls' p: declbody g: 'css,nest,atdecls' }
        { s: '#ATK' a: '@cssKeyframes' p: kfbody g: 'css,nest,keyframes' }
        { s: '#ATS' a: '@cssAtStmt' g: 'css,nest,atstmt' }
        # A nested style rule (selector first, no ":").
        { s: '#TX' b: 1 a: '@cssRule' p: sel g: 'css,nest,rule' }
      ]
      close: [
        { b: 1 g: 'css,decl,end' }
      ]
    }

    # The value of a declaration (a single #VL run).
    declval: {
      open: [
        { s: '#VL' a: '@cssDeclVal' g: 'css,declval' }
        { b: 1 g: 'css,declval,empty' }
      ]
      close: [
        { b: 1 g: 'css,declval,end' }
      ]
    }

    # The "{ ... }" rules body of a block at-rule (@media/@supports/...).
    rulesbody: {
      open: [
        { s: '#OB #CB' b: 1 g: 'css,rulesbody,empty' }
        { s: '#OB' p: items g: 'css,rulesbody' }
      ]
      close: [
        { s: '#CB' a: '@cssEnd' g: 'css,rulesbody,end' }
      ]
    }

    # The "{ ... }" body of @keyframes: a list of keyframe blocks.
    kfbody: {
      open: [
        { s: '#OB #CB' b: 1 g: 'css,kfbody,empty' }
        { s: '#OB' p: kfitems g: 'css,kfbody' }
      ]
      close: [
        { s: '#CB' a: '@cssEnd' g: 'css,kfbody,end' }
      ]
    }

    # A list of keyframe blocks (and comments) -> the keyframes node's
    # keyframes[].
    kfitems: {
      open: [
        { s: '#CB' b: 1 g: 'css,kfitems,empty' }
        { p: keyframe g: 'css,kfitems' }
      ]
      close: [
        { s: '#CB' b: 1 a: '@cssPushKf' g: 'css,kfitems,end' }
        { a: '@cssPushKf' r: kfitems g: 'css,kfitems,next' }
      ]
    }

    # One keyframe block: values (0%, 50%, from, to) + declarations. Mirrors
    # "statement"+"sel" but builds a 'keyframe' node with values[].
    keyframe: {
      open: [
        { s: '#CC' a: '@cssComment' g: 'css,comment' }
        { s: '#TX' b: 1 a: '@cssKeyframe' p: kfsel g: 'css,keyframe' }
      ]
      close: [
        { b: 1 g: 'css,keyframe,end' }
      ]
    }

    kfsel: {
      open: [
        { s: '#TX #GC' a: '@cssKfValue' r: kfsel g: 'css,kfsel,group' }
        { s: '#TX #OB' b: 1 a: '@cssKfValue' p: declbody g: 'css,kfsel,last' }
      ]
      close: [
        { b: 1 g: 'css,kfsel,end' }
      ]
    }
  }
}
"##;
// --- END EMBEDDED css-grammar.jsonic ---

/// The embedded `css-grammar.jsonic` text, exactly as the file on disk holds
/// it. `tests/grammar.rs` checks the two are the same.
pub fn grammar_text() -> &'static str {
    GRAMMAR_TEXT
}

/// The option overrides the grammar installs with: `grammarDef.options` in
/// `ts/src/css.ts`, as a grammar document.
///
/// - jsonic's own rules are excluded (implicit maps and lists, top-level
///   commas, path dives) and `stylesheet` is the start rule.
/// - `;` is the member-separator token `#CA`, and `[` `]` are not tokens:
///   they only appear inside selectors and values, which the `cssToken`
///   matcher reads as text.
/// - `KEY` names `#TX`, as the canonical options do. The option merge
///   replaces an array by index, so the live set keeps its other entries;
///   no alternate here reads it.
/// - The string, number, text and keyword-value matchers are off: the
///   `cssToken` matcher owns all non-fixed text.
/// - Only `/* */` comments exist, and the builtin matcher skips one only
///   where `cssToken` declines it (away from a list position).
/// - An exactly empty source is an empty stylesheet, as in reworkcss: the
///   engine returns `emptyResult` for `""` before any rule runs.
/// - `cssToken` runs at order 1e5, ahead of every builtin matcher, and
///   `@css-prepare` clears the plugin's per-parse state before each parse.
/// - `rule.history` is 1, which the canonical options do not set: a rule
///   keeps a link to the one it replaced, and to no further back. Unbounded,
///   the engine's default, every item of a list stays reachable until the
///   list closes, which held a flat stylesheet of 100,000 rules at 927 MB
///   where the bound holds it at 210 MB (`tests/memory.rs`). No alternate
///   here reads `prev`, so no result changes.
///
/// The raw-string delimiter is `##` because `"#CA"` would end `r#"…"#`.
pub(crate) const OPTIONS_DOC: &str = r##"{"options": {
  "rule": {"exclude": "jsonic,imp", "start": "stylesheet", "history": 1},
  "fixed": {"token": {"#CA": ";", "#OS": null, "#CS": null}},
  "tokenSet": {"KEY": ["#TX"]},
  "string": {"chars": ""},
  "number": {"lex": false},
  "text": {"lex": false},
  "value": {"lex": false},
  "comment": {"lex": true, "def": {
    "hash": {"lex": false},
    "slash": {"lex": false},
    "multi": {"line": false, "start": "/*", "end": "*/", "lex": true}}},
  "lex": {
    "emptyResult": {"type": "stylesheet", "rules": []},
    "match": {"cssToken": {"order": 100000, "make": "@css-token"}}
  },
  "parse": {"prepare": {"css": "@css-prepare"}}
}}"##;

/// The rules `css-grammar.jsonic` defines, in the order it defines them.
/// A second install of the plugin removes these before installing them
/// again, since installing a rule that exists puts the new alternates in
/// front of the old ones rather than replacing them.
pub(crate) const RULES: [&str; 13] = [
    "stylesheet",
    "items",
    "statement",
    "sel",
    "declbody",
    "decls",
    "decl",
    "declval",
    "rulesbody",
    "kfbody",
    "kfitems",
    "keyframe",
    "kfsel",
];

/// The two grammar documents the plugin installs, read once per process:
/// the options, then the rules.
pub(crate) struct Specs {
    pub(crate) options: GrammarSpec,
    pub(crate) rules: GrammarSpec,
}

/// Read the grammar text with jsonic, as `new Tabnas().use(jsonic)` does in
/// the canonical port, and hand the engine the result as JSON.
///
/// The JSON is written by this crate's own writer rather than the engine's:
/// jsonic reads every number as an `f64`, the engine's writer prints `1` as
/// `1.0`, and the loader wants `b: 1`, an integer. The embedded text is this
/// crate's own and shallow, so reading it is not a depth risk.
pub(crate) fn specs() -> Result<&'static Specs, String> {
    static SPECS: OnceLock<Result<Specs, String>> = OnceLock::new();
    SPECS
        .get_or_init(|| {
            let options = GrammarSpec::from_json(OPTIONS_DOC).map_err(|e| e.0)?;
            let parsed = tabnas_jsonic::make()
                .parse(GRAMMAR_TEXT)
                .map_err(|e| format!("css-grammar.jsonic does not read as jsonic: {e}"))?;
            let rules =
                GrammarSpec::from_json(&Value::from_engine(&parsed).to_json()).map_err(|e| e.0)?;
            Ok(Specs { options, rules })
        })
        .as_ref()
        .map_err(Clone::clone)
}
