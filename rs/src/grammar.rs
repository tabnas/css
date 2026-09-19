/* Copyright (c) 2025 Richard Rodger, MIT License */

//! The grammar: the embedded `css-grammar.jsonic` text, and the reader that
//! turns it into the rule table [`machine`](crate::machine) runs.
//!
//! `css-grammar.jsonic` at the repository root is the SINGLE SOURCE OF TRUTH
//! for the rules. `ts/embed-grammar.js` copies it verbatim into `ts/src/css.ts`,
//! `go/css.go` and the `GRAMMAR_TEXT` literal below. Never hand-edit between
//! the `BEGIN/END EMBEDDED` markers — edit the `.jsonic` file and re-run
//! `npm run embed` from `ts/`.
//!
//! The TypeScript and Go ports read that text with a jsonic engine, which they
//! already depend on. This port has no engine under it, so it carries the
//! small reader below: enough of the jsonic dialect to read THIS document —
//! `#` comments, `{}` maps with bare or quoted keys, `[]` lists, bare and
//! quoted scalars, and newline-separated entries with commas optional. It is
//! deliberately not a jsonic implementation; `tests/grammar.rs` pins what it
//! accepts.

use std::collections::HashMap;

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

/// One alternative of a rule's `open` or `close` list.
///
/// The fields are the jsonic alt fields this grammar uses:
/// `s` (the token sequence to match), `b` (how many of those matched tokens to
/// push back rather than consume), `p` (push a child rule), `r` (replace this
/// rule), `a` (actions to run) and `g` (the alt's tag groups).
#[derive(Clone, Debug, Default)]
pub struct Alt {
    /// Token names this alt matches, in order. Empty matches unconditionally.
    pub s: Vec<String>,
    /// Tokens to push back — matched, recorded, but left for the next rule.
    pub b: usize,
    /// The child rule to push after this alt's actions run.
    pub p: Option<String>,
    /// The rule to replace this one with after this alt's actions run.
    pub r: Option<String>,
    /// Action references (`@cssXxx`), run in order.
    pub a: Vec<String>,
    /// Comma-separated tag groups, for diagnostics.
    pub g: String,
}

/// One grammar rule: the alts tried when it opens, and when it closes.
#[derive(Clone, Debug, Default)]
pub struct RuleDef {
    /// Alts tried, in order, when the rule opens.
    pub open: Vec<Alt>,
    /// Alts tried, in order, when the rule closes.
    pub close: Vec<Alt>,
}

/// The rule table read out of [`GRAMMAR_TEXT`].
#[derive(Clone, Debug, Default)]
pub struct Grammar {
    rules: HashMap<String, RuleDef>,
}

impl Grammar {
    /// Read the embedded `css-grammar.jsonic`.
    ///
    /// # Panics
    ///
    /// If the embedded text is malformed. That is a build-time defect in this
    /// crate, not anything a caller can provoke, and `tests/grammar.rs` fails
    /// the build before it can ship.
    pub fn load() -> Grammar {
        Grammar::parse(GRAMMAR_TEXT).expect("css: embedded grammar is malformed")
    }

    /// Read a grammar document in the jsonic subset described above.
    pub fn parse(text: &str) -> Result<Grammar, String> {
        let root = Reader::new(text).document()?;
        let mut rules = HashMap::new();
        if let Some(GVal::Map(rule_map)) = root.get("rule") {
            for (name, def) in rule_map {
                let GVal::Map(def) = def else { continue };
                rules.insert(
                    name.clone(),
                    RuleDef {
                        open: build_alts(def.get("open")),
                        close: build_alts(def.get("close")),
                    },
                );
            }
        }
        Ok(Grammar { rules })
    }

    /// The rule named `name`, if the grammar defines one.
    pub fn rule(&self, name: &str) -> Option<&RuleDef> {
        self.rules.get(name)
    }

    /// How many rules the grammar defines.
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    /// Whether the grammar defines no rules at all.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// The rule names, unordered.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.rules.keys().map(String::as_str)
    }
}

/// The verbatim `css-grammar.jsonic` text this crate was built from. Exposed
/// so a test can compare it against the file on disk and catch an embed that
/// was never re-run.
pub fn grammar_text() -> &'static str {
    GRAMMAR_TEXT
}

fn build_alts(def: Option<&GVal>) -> Vec<Alt> {
    let Some(GVal::List(items)) = def else {
        return Vec::new();
    };
    items
        .iter()
        .map(|item| {
            let GVal::Map(m) = item else {
                return Alt::default();
            };
            Alt {
                s: string_list(m.get("s")),
                // A non-integer `b` is not something this grammar writes; read
                // it as "push nothing back" rather than failing the build.
                b: m.get("b").and_then(GVal::as_num).unwrap_or(0.0).max(0.0) as usize,
                p: m.get("p").and_then(GVal::as_str).map(str::to_string),
                r: m.get("r").and_then(GVal::as_str).map(str::to_string),
                a: string_list(m.get("a")),
                g: m.get("g")
                    .and_then(GVal::as_str)
                    .unwrap_or_default()
                    .to_string(),
            }
        })
        .collect()
}

/// Read a field that is either one string or a list of them.
///
/// `s: '#TX #OB'` is a space-separated sequence, and `a:` may be a single
/// action or an array of them (`['@reset$' '@cssX']`) — the Go port's
/// `buildGrammarAlts` handles the same two shapes.
fn string_list(def: Option<&GVal>) -> Vec<String> {
    match def {
        Some(GVal::Str(s)) => s.split_whitespace().map(str::to_string).collect(),
        Some(GVal::List(items)) => items
            .iter()
            .filter_map(GVal::as_str)
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

// --- The jsonic-subset reader ---------------------------------------------

/// A value in a grammar document. An insertion-ordered map keeps the alt lists
/// in source order, which is the order they are tried in.
#[derive(Clone, Debug, PartialEq)]
enum GVal {
    Str(String),
    Num(f64),
    Bool(bool),
    Map(GMap),
    List(Vec<GVal>),
}

impl GVal {
    fn as_str(&self) -> Option<&str> {
        match self {
            GVal::Str(s) => Some(s),
            _ => None,
        }
    }
    fn as_num(&self) -> Option<f64> {
        match self {
            GVal::Num(n) => Some(*n),
            _ => None,
        }
    }
}

/// An insertion-ordered string map.
#[derive(Clone, Debug, Default, PartialEq)]
struct GMap(Vec<(String, GVal)>);

impl GMap {
    fn get(&self, key: &str) -> Option<&GVal> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
}

impl<'a> IntoIterator for &'a GMap {
    type Item = (&'a String, &'a GVal);
    type IntoIter = std::iter::Map<
        std::slice::Iter<'a, (String, GVal)>,
        fn(&'a (String, GVal)) -> (&'a String, &'a GVal),
    >;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter().map(|(k, v)| (k, v))
    }
}

struct Reader<'a> {
    src: &'a [u8],
    i: usize,
}

impl<'a> Reader<'a> {
    fn new(text: &'a str) -> Reader<'a> {
        Reader {
            src: text.as_bytes(),
            i: 0,
        }
    }

    /// The whole document: one value, then end of input.
    fn document(mut self) -> Result<GMap, String> {
        self.skip_space();
        let v = self.value()?;
        self.skip_space();
        if self.i < self.src.len() {
            return Err(format!("trailing content at byte {}", self.i));
        }
        match v {
            GVal::Map(m) => Ok(m),
            other => Err(format!("grammar is not a map: {other:?}")),
        }
    }

    /// Skip whitespace, `,` separators (optional in this dialect) and `#`
    /// line comments.
    fn skip_space(&mut self) {
        while self.i < self.src.len() {
            match self.src[self.i] {
                b' ' | b'\t' | b'\r' | b'\n' | b',' => self.i += 1,
                b'#' => {
                    while self.i < self.src.len() && self.src[self.i] != b'\n' {
                        self.i += 1;
                    }
                }
                _ => return,
            }
        }
    }

    fn value(&mut self) -> Result<GVal, String> {
        self.skip_space();
        match self.src.get(self.i) {
            None => Err("unexpected end of grammar".to_string()),
            Some(b'{') => self.map(),
            Some(b'[') => self.list(),
            Some(b'\'') | Some(b'"') => Ok(GVal::Str(self.quoted()?)),
            Some(_) => Ok(scalar(self.bare())),
        }
    }

    fn map(&mut self) -> Result<GVal, String> {
        self.i += 1; // '{'
        let mut out = GMap::default();
        loop {
            self.skip_space();
            match self.src.get(self.i) {
                None => return Err("unclosed '{' in grammar".to_string()),
                Some(b'}') => {
                    self.i += 1;
                    return Ok(GVal::Map(out));
                }
                _ => {}
            }
            let key = match self.src[self.i] {
                b'\'' | b'"' => self.quoted()?,
                _ => self.bare(),
            };
            self.skip_space();
            if self.src.get(self.i) != Some(&b':') {
                return Err(format!("expected ':' after key '{key}' in grammar"));
            }
            self.i += 1;
            let value = self.value()?;
            out.0.push((key, value));
        }
    }

    fn list(&mut self) -> Result<GVal, String> {
        self.i += 1; // '['
        let mut out = Vec::new();
        loop {
            self.skip_space();
            match self.src.get(self.i) {
                None => return Err("unclosed '[' in grammar".to_string()),
                Some(b']') => {
                    self.i += 1;
                    return Ok(GVal::List(out));
                }
                _ => out.push(self.value()?),
            }
        }
    }

    /// A `'...'` or `"..."` string. A backslash escapes the next character.
    fn quoted(&mut self) -> Result<String, String> {
        let quote = self.src[self.i];
        self.i += 1;
        let mut out = String::new();
        while self.i < self.src.len() {
            let c = self.src[self.i];
            if c == quote {
                self.i += 1;
                return Ok(out);
            }
            if c == b'\\' && self.i + 1 < self.src.len() {
                self.i += 1;
            }
            let next = next_char_end(self.src, self.i);
            out.push_str(&lossy(&self.src[self.i..next]));
            self.i = next;
        }
        Err("unclosed string in grammar".to_string())
    }

    /// An unquoted token, up to the next structural character.
    fn bare(&mut self) -> String {
        let start = self.i;
        while self.i < self.src.len() {
            match self.src[self.i] {
                b' ' | b'\t' | b'\r' | b'\n' | b',' | b':' | b'{' | b'}' | b'[' | b']' | b'#' => {
                    break
                }
                _ => self.i += 1,
            }
        }
        lossy(&self.src[start..self.i])
    }
}

/// Classify a bare token: `true`/`false`, a number, or a string.
fn scalar(token: String) -> GVal {
    match token.as_str() {
        "true" => return GVal::Bool(true),
        "false" => return GVal::Bool(false),
        _ => {}
    }
    match token.parse::<f64>() {
        Ok(n) => GVal::Num(n),
        Err(_) => GVal::Str(token),
    }
}

/// The end of the UTF-8 character starting at `i`.
fn next_char_end(src: &[u8], i: usize) -> usize {
    let mut e = i + 1;
    while e < src.len() && (src[e] & 0xC0) == 0x80 {
        e += 1;
    }
    e
}

fn lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}
