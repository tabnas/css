/* Copyright (c) 2025 Richard Rodger, MIT License */

//! The lexer: the single `cssToken` matcher the plugin installs on the
//! engine, and the scanners it reads spans with.
//!
//! CSS is context-sensitive — the same characters can begin a selector, a
//! property or a value — so the matcher owns the hard tokenisation and the
//! grammar only assembles typed nodes from what it emits. Which token comes
//! out depends on the rule that is active when the character is read: the
//! engine reads a token only when the alternate being tried needs one, under
//! the rule trying it, and passes that rule to the matcher.
//!
//! See `ts/src/css.ts` for the canonical commentary; the three ports are kept
//! in step, and this module follows `buildCssTokenMatcher` there line for
//! line.

use std::fmt;

use tabnas::{
    Context, Lexer, Rule, TabnasError, Tin as EngineTin, Token, Value, TIN_BD, TIN_TX, TIN_VL,
    TIN_ZZ,
};

/// A token kind.
///
/// `Ob`/`Cb`/`Cl`/`Ca` are the fixed punctuation the canonical port borrows
/// from the engine (`{` `}` `:` and `;`, the last remapped onto jsonic's
/// member separator). The rest are this grammar's own: a custom name is
/// required for a comment NODE because the engine's builtin comment tin is in
/// the parser's ignore set, so emitting it would silently drop the node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tin {
    /// `{` — start of a block.
    Ob,
    /// `}` — end of a block.
    Cb,
    /// `:` — declaration separator.
    Cl,
    /// `;` — declaration terminator.
    Ca,
    /// A selector, keyframe value, or property name.
    Tx,
    /// `,` — selector-group separator.
    Gc,
    /// A declaration value (raw text).
    Vl,
    /// A comment, at a statement position.
    Cc,
    /// An at-rule with a rules body (`@media`, `@supports`, …).
    Atr,
    /// An at-rule with a declarations body (`@font-face`, `@page`, …).
    Atd,
    /// `@keyframes`.
    Atk,
    /// A statement at-rule (`@import`, `@charset`, …).
    Ats,
    /// End of input.
    Zz,
}

impl Tin {
    /// The token's grammar name, as `css-grammar.jsonic` writes it.
    pub fn name(self) -> &'static str {
        match self {
            Tin::Ob => "#OB",
            Tin::Cb => "#CB",
            Tin::Cl => "#CL",
            Tin::Ca => "#CA",
            Tin::Tx => "#TX",
            Tin::Gc => "#GC",
            Tin::Vl => "#VL",
            Tin::Cc => "#CC",
            Tin::Atr => "#ATR",
            Tin::Atd => "#ATD",
            Tin::Atk => "#ATK",
            Tin::Ats => "#ATS",
            Tin::Zz => "#ZZ",
        }
    }

    /// A human description, for diagnostics and diagram legends.
    pub fn describe(self) -> &'static str {
        match self {
            Tin::Ob => "{ — start of a block",
            Tin::Cb => "} — end of a block",
            Tin::Cl => ": — declaration separator",
            Tin::Ca => "; — declaration terminator",
            Tin::Tx => "a selector, keyframe value, or property name",
            Tin::Gc => ", — selector-group separator",
            Tin::Vl => "a declaration value (raw text)",
            Tin::Cc => "a comment",
            Tin::Atr => "an at-rule with a rules body (@media, @supports, …)",
            Tin::Atd => "an at-rule with a declarations body (@font-face, @page)",
            Tin::Atk => "@keyframes",
            Tin::Ats => "a statement at-rule (@import, @charset, …)",
            Tin::Zz => "end of input",
        }
    }
}

/// A parse failure.
///
/// `code` is the contract: the shared fixtures pin `ERROR:<code>` and compare
/// it exactly. [`Css::parse`](crate::Css::parse) raises only the two codes the
/// canonical port inherits from the engine — `unterminated_comment` and
/// `unexpected` — and this crate declares none of its own, matching
/// `tabnas.plugin.json` (`errorCodes: []`). The engine's own tree form can
/// also stop with `cancel`, at [`TREE_RULE_DEPTH`](crate::TREE_RULE_DEPTH).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    /// The error code, compared exactly by the conformance fixtures.
    pub code: String,
    /// A human-readable explanation.
    pub message: String,
    /// 1-based line where the failure was found.
    pub line: usize,
    /// 1-based column (UTF-16 code units) where the failure was found.
    pub column: usize,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[css/{}]: {} (line {}, column {})",
            self.code, self.message, self.line, self.column
        )
    }
}

impl std::error::Error for Error {}

/// The scanners' "there is no valid end here" result: an unclosed
/// `/* ... */`.
///
/// A distinct value rather than an index, because the caller's response is a
/// REJECTION, not a shorter span. Running the scan to end of source instead is
/// how an unclosed comment in an at-rule prelude quietly became part of the
/// prelude: `@import/*red` parsed as an import of `/*red`.
const UNTERMINATED: usize = usize::MAX;

/// What [`scan_to_brace_or_end`] found first.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    /// A top-level `{` — so the run just scanned is a selector or a prelude.
    Selector,
    /// A top-level `;`/`}` or end of input — so it is a property or params.
    Decl,
}

/// Rule names at which a `/* */` comment is captured as a NODE: the
/// statement / declaration / keyframe LIST readers, which lex the first token
/// of each item.
///
/// The item builders (`statement`/`decl`/`keyframe`) are deliberately absent.
/// They reuse the token the list reader already produced, so a comment seen
/// mid-construct — between a property and its `:`, say — is read under a
/// builder, declined here, and skipped as insignificant whitespace.
///
/// The block wrappers are present because their empty-block `#OB #CB`
/// lookahead lexes the first body token, so a comment right after `{` is
/// captured there.
const COMMENT_NODE_RULES: [&str; 6] = [
    "items",
    "decls",
    "kfitems",
    "declbody",
    "rulesbody",
    "kfbody",
];

fn is_comment_node_rule(name: &str) -> bool {
    COMMENT_NODE_RULES.contains(&name)
}

/// At-keywords whose body is a declaration list rather than a rules list.
const DECLS_KW: [&str; 7] = [
    "font-face",
    "page",
    "viewport",
    "-ms-viewport",
    "counter-style",
    "property",
    "font-palette-values",
];

/// `^(-[a-z]+-)?keyframes$`, without a regex engine.
fn is_keyframes_kw(kw: &str) -> bool {
    if kw == "keyframes" {
        return true;
    }
    match vendor_prefix(kw) {
        Some(v) => &kw[v.len()..] == "keyframes",
        None => false,
    }
}

/// `^(-[a-z]+-)`: a vendor prefix on an at-keyword, e.g. `-webkit-keyframes`.
pub fn vendor_prefix(kw: &str) -> Option<&str> {
    let b = kw.as_bytes();
    if b.first() != Some(&b'-') {
        return None;
    }
    let mut i = 1;
    while i < b.len() && b[i].is_ascii_lowercase() {
        i += 1;
    }
    if 1 < i && b.get(i) == Some(&b'-') {
        Some(&kw[..i + 1])
    } else {
        None
    }
}

/// Whether `c` is whitespace as ECMAScript defines it, which is NOT what
/// [`char::is_whitespace`] answers.
///
/// The canonical port trims with JavaScript's `String.prototype.trim`, whose
/// set is ECMAScript WhiteSpace plus LineTerminator. Rust's set is the Unicode
/// `White_Space` property. They differ by exactly two code points, and both
/// differences change the AST:
///
/// - **U+FEFF** (zero-width no-break space, the byte-order mark) is
///   ECMAScript whitespace and is not Unicode `White_Space`. A stylesheet that
///   opens with a byte-order mark yields the selector `a` in the canonical
///   port and would yield `\u{feff}a` under [`str::trim`].
/// - **U+0085** (next line) is Unicode `White_Space` and is not ECMAScript
///   whitespace. `@media x \u{85}{…}` keeps that character in its prelude in
///   the canonical port and would lose it under [`str::trim`].
///
/// Everything else in either set is in both: the ASCII controls, U+0020, the
/// `Zs` category, and the two line separators U+2028 and U+2029.
pub fn es_is_whitespace(c: char) -> bool {
    ('\u{feff}' == c) || (c.is_whitespace() && '\u{85}' != c)
}

/// Trim as JavaScript's `String.prototype.trim` does. See
/// [`es_is_whitespace`] for why this is not [`str::trim`].
pub fn es_trim(s: &str) -> &str {
    s.trim_matches(es_is_whitespace)
}

/// The column width of `s`: its length in UTF-16 code units, which is what a
/// JavaScript string index counts.
pub(crate) fn col_width(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

/// Slice `src` by byte offsets, clamping to something sliceable.
///
/// This is the Rust half of the Go port's `clampLen`, and it exists for the
/// same reason: the scanners deliberately overshoot. An `i += 2` over a `\`
/// escape, an unterminated string or an unterminated comment can step past the
/// end of the source, and JavaScript CLAMPS an out-of-range slice bound where
/// Go panics — so the Go port clamps to reproduce the JavaScript result rather
/// than invent a new one.
///
/// Rust panics on one bound more than Go does: an index inside a multi-byte
/// character. Every index these scanners RETURN is at an ASCII character or at
/// the end of the source, because each one stops at `{`, `}`, `;`, `,`, `*/`,
/// a quote or end of input, so the fallback below is unreachable in practice —
/// it is here so that a future scanner change degrades to a shorter span
/// instead of a panic in a caller's parse.
fn safe_slice(src: &str, start: usize, end: usize) -> &str {
    let mut start = start.min(src.len());
    let mut end = end.clamp(start, src.len());
    while 0 < start && !src.is_char_boundary(start) {
        start -= 1;
    }
    while start < end && !src.is_char_boundary(end) {
        end -= 1;
    }
    &src[start..end]
}

/// The index of the `*` that opens the `*/` closing the comment at `i`, or
/// `None` if there is no closing `*/`.
fn find_comment_end(src: &[u8], i: usize) -> Option<usize> {
    let mut j = i + 2;
    while j + 1 < src.len() {
        if src[j] == b'*' && src[j + 1] == b'/' {
            return Some(j);
        }
        j += 1;
    }
    None
}

/// The index after a closed `/* ... */`, or [`UNTERMINATED`].
///
/// It does NOT run to end of source. An unclosed `/* ... */` is an error, not
/// a comment to the end of the file — which is what both the engine's builtin
/// comment matcher and reworkcss do, and what `test/spec/reworkcss.tsv` pins.
fn skip_comment(src: &[u8], i: usize) -> usize {
    match find_comment_end(src, i) {
        // `e + 1` is the `/`, so `e + 2` is always within the source.
        Some(e) => e + 2,
        None => UNTERMINATED,
    }
}

/// Skip a quoted string; returns the index after the closing quote.
fn skip_string(src: &[u8], mut i: usize) -> usize {
    let quote = src[i];
    i += 1;
    while i < src.len() {
        if src[i] == b'\\' {
            i += 2;
            continue;
        }
        if src[i] == quote {
            return i + 1;
        }
        i += 1;
    }
    // A trailing backslash steps i ONE PAST the end. That overshoot is not a
    // defect to clamp away here: the canonical port returns it too, and its
    // column arithmetic counts it. See [`css_token`] and [`safe_slice`].
    i
}

/// Scan a key prelude: where it ends, and whether it is a selector (a
/// top-level `{` comes first) or a declaration (a top-level `;`/`}` or end of
/// input comes first). Strings, `()`, `[]` and comments are skipped.
fn scan_to_brace_or_end(src: &[u8], mut i: usize) -> (Kind, usize) {
    let mut depth = 0usize;
    while i < src.len() {
        let c = src[i];
        if c == b'"' || c == b'\'' {
            i = skip_string(src, i);
            continue;
        }
        if c == b'/' && src.get(i + 1) == Some(&b'*') {
            let n = skip_comment(src, i);
            if UNTERMINATED == n {
                return (Kind::Decl, UNTERMINATED);
            }
            i = n;
            continue;
        }
        // A CSS escape (`\(`, `\'`, `\3A `, …) hides the next character from
        // the structural scan.
        if c == b'\\' {
            i += 2;
            continue;
        }
        if c == b'(' || c == b'[' {
            depth += 1;
        } else if c == b')' || c == b']' {
            depth = depth.saturating_sub(1);
        } else if 0 == depth {
            if c == b'{' {
                return (Kind::Selector, i);
            }
            if c == b';' || c == b'}' {
                return (Kind::Decl, i);
            }
        }
        i += 1;
    }
    // i may be one past the end; see skip_string.
    (Kind::Decl, i)
}

/// Scan a single selector: to the next top-level `,` (a group separator) or
/// `{`, or end of input. Strings, `()`, `[]` and comments are skipped.
fn scan_selector_end(src: &[u8], mut i: usize) -> usize {
    let mut depth = 0usize;
    while i < src.len() {
        let c = src[i];
        if c == b'"' || c == b'\'' {
            i = skip_string(src, i);
            continue;
        }
        if c == b'/' && src.get(i + 1) == Some(&b'*') {
            let n = skip_comment(src, i);
            if UNTERMINATED == n {
                return UNTERMINATED;
            }
            i = n;
            continue;
        }
        if c == b'\\' {
            i += 2;
            continue;
        }
        if c == b'(' || c == b'[' {
            depth += 1;
        } else if c == b')' || c == b']' {
            depth = depth.saturating_sub(1);
        } else if 0 == depth && (c == b',' || c == b'{') {
            return i;
        }
        i += 1;
    }
    // i may be one past the end; see skip_string.
    i
}

/// Scan a declaration value or at-rule params: to the next top-level `;` or
/// `}`, or end of input. Strings, `()`, `[]` and comments are skipped.
fn scan_value_end(src: &[u8], mut i: usize) -> usize {
    let mut depth = 0usize;
    while i < src.len() {
        let c = src[i];
        if c == b'"' || c == b'\'' {
            i = skip_string(src, i);
            continue;
        }
        if c == b'/' && src.get(i + 1) == Some(&b'*') {
            let n = skip_comment(src, i);
            if UNTERMINATED == n {
                return UNTERMINATED;
            }
            i = n;
            continue;
        }
        if c == b'\\' {
            i += 2;
            continue;
        }
        if c == b'(' || c == b'[' {
            depth += 1;
        } else if c == b')' || c == b']' {
            depth = depth.saturating_sub(1);
        } else if 0 == depth && (c == b';' || c == b'}') {
            return i;
        }
        i += 1;
    }
    // i may be one past the end; see skip_string.
    i
}

/// Remove `/* ... */` comments from a selector or value run, leaving quoted
/// strings (which may contain `/*`) untouched.
pub fn strip_comments(s: &str) -> String {
    if !s.contains("/*") {
        return s.to_string();
    }
    let src = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(src.len());
    let mut i = 0;
    while i < src.len() {
        let c = src[i];
        if c == b'"' || c == b'\'' {
            let j = skip_string(src, i);
            out.extend_from_slice(&src[i..j.min(src.len())]);
            i = j;
            continue;
        }
        if c == b'/' && src.get(i + 1) == Some(&b'*') {
            let n = skip_comment(src, i);
            if UNTERMINATED == n {
                // Unreachable once the callers reject an unclosed comment,
                // but a sentinel assigned to i would spin. The rest of the
                // span is comment text; drop it.
                break;
            }
            i = n;
            continue;
        }
        out.push(c);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
}

/// A CSS property name character.
///
/// Beyond the identifier characters this admits the legacy IE hack prefixes
/// `*`, `#`, `/` (as in `//prop`) and a literal backslash, matching the
/// reworkcss property pattern `\*?[-#\/\*\\\w]+(\[[0-9a-z_-]+\])?` — real
/// stylesheets rely on them.
fn is_prop_char(c: u8) -> bool {
    c.is_ascii_alphanumeric()
        || c == b'-'
        || c == b'_'
        || c == b'#'
        || c == b'*'
        || c == b'/'
        || c == b'\\'
}

/// A `[0-9a-z_-]` character, the only content allowed in a property name's
/// bracket suffix (e.g. `opacity[sqrt]`).
fn is_prop_suffix_char(c: u8) -> bool {
    c.is_ascii_digit() || c.is_ascii_lowercase() || c == b'-' || c == b'_'
}

/// Scan a property name: the run of property characters plus an optional
/// `[...]` suffix. Returns the index after the name (`== i` if there is none).
fn scan_prop_end(src: &[u8], i: usize) -> usize {
    let mut e_i = i;
    while e_i < src.len() && is_prop_char(src[e_i]) {
        e_i += 1;
    }
    if e_i == i {
        return i;
    }
    if src.get(e_i) == Some(&b'[') {
        let mut b_i = e_i + 1;
        while b_i < src.len() && is_prop_suffix_char(src[b_i]) {
            b_i += 1;
        }
        if e_i + 1 < b_i && src.get(b_i) == Some(&b']') {
            return b_i + 1;
        }
    }
    e_i
}

/// An at-keyword character: letters, digits and `-`.
fn is_at_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'-'
}

/// Split a selector-group prelude on its top-level commas (commas inside
/// strings, `()` or `[]` are part of a single selector), stripping comments
/// and trimming each selector. An empty prelude yields an empty list.
pub fn split_selectors(prelude: &str) -> Vec<String> {
    let src = prelude.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i <= src.len() {
        let mut end = scan_selector_end(src, i);
        if UNTERMINATED == end {
            // Unreachable in practice: the prelude was scanned (and an
            // unclosed comment rejected) before it got here. Treated as "the
            // rest" so a sentinel can never index or spin.
            end = src.len();
        }
        let one = es_trim(&strip_comments(safe_slice(prelude, i, end))).to_string();
        if !one.is_empty() {
            out.push(one);
        }
        if end >= src.len() {
            break;
        }
        i = end + 1;
    }
    out
}

// ---------------------------------------------------------------------------
// The `cssToken` matcher, as the engine runs it.

/// The engine's numbers for the tokens this grammar names itself, which
/// `Tabnas::token` allocates at install.
///
/// Captured by value in the matcher: resolving a name per token would go
/// through `Lexer::token_tin`, which clones the engine's options each time.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Tins {
    pub(crate) cc: EngineTin,
    pub(crate) atr: EngineTin,
    pub(crate) atd: EngineTin,
    pub(crate) atk: EngineTin,
    pub(crate) ats: EngineTin,
    pub(crate) gc: EngineTin,
}

/// Where a scan ran past the end of the source, and by how much: the
/// `ctx.u` key [`css_token`] records it under and [`lex_subscriber`] reads.
pub(crate) const OVERSHOOT: &str = "tabnas-css/overshoot";

/// The most of the source an `unterminated_comment` diagnostic shows. The
/// span runs from the scan's start to the end of the source, which for a
/// large stylesheet would be most of it; the message is informative, not the
/// contract, so it is cut here.
const BAD_SPAN: usize = 32;

/// What [`plan_token`] decided for the characters at the cursor.
enum Plan {
    /// Not this matcher's: the engine's builtin matchers take it.
    Decline,
    /// An unclosed `/* ... */` behind the cursor: `unterminated_comment`.
    Bad,
    /// A token, and the byte offset just past it, which may be ONE past the
    /// end of the source (see [`css_token`]).
    Emit {
        name: &'static str,
        tin: EngineTin,
        val: String,
        raw: String,
        use_kv: Option<(&'static str, String)>,
        end: usize,
    },
}

/// Decide the token at byte `s_i` of `src`, read under the rule `rule`.
///
/// `buildCssTokenMatcher` in `ts/src/css.ts`, step for step: whitespace is
/// declined (the engine's space and line matchers take it), a comment is a
/// node only at a list position, a value runs to the next top-level `;` or
/// `}` under `declval`, a top-level `,` separates a selector group, `@`
/// starts an at-rule, `{` `}` `;` are the grammar's, and anything else is a
/// selector or a property name by `{`-before-`;` lookahead.
fn plan_token(src: &str, s_i: usize, rule: &str, lower: bool, tins: Tins) -> Plan {
    let bytes = src.as_bytes();
    let Some(&c) = bytes.get(s_i) else {
        return Plan::Decline;
    };
    if c == b' ' || c == b'\t' || c == b'\r' || c == b'\n' {
        return Plan::Decline;
    }
    let emit = |name, tin, val: String, raw: &str, use_kv, end| Plan::Emit {
        name,
        tin,
        val,
        raw: raw.to_string(),
        use_kv,
        end,
    };

    // Comments: a node at a list position, otherwise declined, and then
    // skipped by the engine's builtin comment matcher.
    if c == b'/' && bytes.get(s_i + 1) == Some(&b'*') {
        if !is_comment_node_rule(rule) {
            return Plan::Decline;
        }
        return match find_comment_end(bytes, s_i) {
            None => Plan::Bad,
            Some(e) => emit(
                "#CC",
                tins.cc,
                safe_slice(src, s_i + 2, e).to_string(),
                safe_slice(src, s_i, e + 2),
                None,
                e + 2,
            ),
        };
    }

    // Value position: a declaration value up to the next top-level `;`/`}`,
    // comments stripped and surrounding space trimmed.
    if "declval" == rule {
        if c == b'{' || c == b'}' || c == b';' || c == b':' {
            return Plan::Decline;
        }
        let end = scan_value_end(bytes, s_i);
        if UNTERMINATED == end {
            return Plan::Bad;
        }
        let raw = safe_slice(src, s_i, end);
        let val = es_trim(&strip_comments(raw)).to_string();
        return emit("#VL", TIN_VL, val, raw, None, end);
    }

    // A top-level selector-group comma.
    if c == b',' {
        return emit("#GC", tins.gc, ",".to_string(), ",", None, s_i + 1);
    }

    // An at-rule: block or statement by `{`-before-`;` lookahead, and a
    // block's body by keyword. The keyword rides in the value and the
    // prelude (block) or params (statement) in the token's `use` bag.
    if c == b'@' {
        let mut k_end = s_i + 1;
        while k_end < bytes.len() && is_at_char(bytes[k_end]) {
            k_end += 1;
        }
        let kw = safe_slice(src, s_i + 1, k_end).to_string();
        let (kind, index) = scan_to_brace_or_end(bytes, s_i);
        if UNTERMINATED == index {
            return Plan::Bad;
        }
        if Kind::Selector == kind {
            // At-rule preludes KEEP their comments (they are only trimmed),
            // matching reworkcss.
            let prelude = es_trim(safe_slice(src, k_end, index)).to_string();
            let (name, tin) = if is_keyframes_kw(&kw) {
                ("#ATK", tins.atk)
            } else if DECLS_KW.contains(&kw.as_str()) {
                ("#ATD", tins.atd)
            } else {
                ("#ATR", tins.atr)
            };
            let raw = safe_slice(src, s_i, index);
            return emit(name, tin, kw, raw, Some(("prelude", prelude)), index);
        }
        // A statement at-rule's params run to the next top-level `;`/`}`. A
        // `;` is consumed (it ends the statement); a `}` or the end of the
        // source is left for the enclosing rule.
        let p_end = scan_value_end(bytes, k_end);
        if UNTERMINATED == p_end {
            return Plan::Bad;
        }
        let params = es_trim(safe_slice(src, k_end, p_end)).to_string();
        let end = if bytes.get(p_end) == Some(&b';') {
            p_end + 1
        } else {
            p_end
        };
        let raw = safe_slice(src, s_i, end);
        return emit("#ATS", tins.ats, kw, raw, Some(("params", params)), end);
    }

    // Other fixed punctuation belongs to the grammar. `:` does not: a run
    // that starts with one is scanned below, as in the canonical port.
    if c == b'{' || c == b'}' || c == b';' {
        return Plan::Decline;
    }

    // A selector or a property name, by `{`-before-`;` lookahead.
    let (kind, index) = scan_to_brace_or_end(bytes, s_i);
    if UNTERMINATED == index {
        return Plan::Bad;
    }
    if Kind::Selector == kind {
        // One selector of a (possible) group: up to the next top-level `,`
        // or `{`, comments stripped and surrounding space trimmed.
        let end = scan_selector_end(bytes, s_i);
        if UNTERMINATED == end {
            return Plan::Bad;
        }
        let raw = safe_slice(src, s_i, end);
        let val = es_trim(&strip_comments(raw)).to_string();
        return emit("#TX", TIN_TX, val, raw, None, end);
    }

    // A property name: the run of property characters up to `:`,
    // whitespace, `;` or `}`. `/` and `*` are property characters (the
    // `*prop` and `//prop` IE hacks), so a trailing `/*...*/` hack comment
    // lands inside the scanned name; comments are stripped out of it, as
    // reworkcss does.
    let e_i = scan_prop_end(bytes, s_i);
    if e_i == s_i {
        return Plan::Decline;
    }
    let raw = safe_slice(src, s_i, e_i);
    let mut prop = strip_comments(raw);
    if prop.is_empty() {
        return Plan::Decline;
    }
    if lower {
        prop = prop.to_lowercase();
    }
    emit("#TX", TIN_TX, prop, raw, None, e_i)
}

/// The `cssToken` matcher, registered as `@css-token` and named by the
/// grammar's options at order 1e5, ahead of every builtin matcher.
///
/// Two things here are deliberate and easy to "fix" wrongly:
///
/// - The end of a token may be ONE PAST the source. A `\` at the last
///   character is an escape whose escaped character is not there, and the
///   scanners' `i += 2` steps off the end. The canonical port advances its
///   column by that unclamped width and takes the CLAMPED span for the
///   token's text, so `@host\` yields a node ending at column 7 inside a
///   stylesheet ending at column 8, one past a source six characters long.
///   The engine will not move its cursor past the end, so the overshoot is
///   recorded under [`OVERSHOOT`] and [`lex_subscriber`] adds it to the
///   end-of-source token, the only token that can follow it.
/// - An unclosed comment is reported WITHOUT advancing, so the diagnostic
///   points at the start of the scan, as the canonical port's does.
///
/// A matcher must never advance and then decline: the engine captured its
/// dispatch state before the custom matchers ran.
pub(crate) fn css_token(
    lexer: &mut Lexer<'_>,
    rule: &mut Rule,
    ctx: &mut Context,
    lower: bool,
    tins: Tins,
) -> Option<Token> {
    let point = lexer.point();
    let s_i = point.site.si;
    let (plan, chars, overshoot) = {
        let src = lexer.source();
        let plan = plan_token(src, s_i, rule.name.as_str(), lower, tins);
        let (chars, overshoot) = match &plan {
            Plan::Decline => return None,
            Plan::Bad => (
                safe_slice(src, s_i, src.len())
                    .chars()
                    .take(BAD_SPAN)
                    .count(),
                0,
            ),
            Plan::Emit { end, .. } => (
                safe_slice(src, s_i, (*end).min(src.len())).chars().count(),
                end.saturating_sub(src.len()),
            ),
        };
        (plan, chars, overshoot)
    };
    match plan {
        Plan::Decline => None,
        Plan::Bad => Some(lexer.bad_span(
            "unterminated_comment",
            point.site.pos,
            point.site.pos + chars,
        )),
        Plan::Emit {
            name,
            tin,
            val,
            raw,
            use_kv,
            ..
        } => {
            lexer.advance_chars(chars);
            if 0 < overshoot {
                ctx.u
                    .insert(OVERSHOOT.to_string(), Value::Number(overshoot as f64));
            }
            let mut token = lexer.token(name, tin, Value::String(val), raw, point);
            if let Some((key, text)) = use_kv {
                token
                    .use_data_mut()
                    .insert(key.to_string(), Value::String(text));
            }
            Some(token)
        }
    }
}

/// The plugin's lex subscriber: it sees every token the engine fetches.
///
/// - The end-of-source token takes the column overshoot [`css_token`]
///   recorded, so the stylesheet's end and an error at the end of the source
///   land where the canonical port puts them.
/// - A bad token fetched behind a good one becomes the first token of the
///   lookahead. The canonical engine throws a bad token the moment it is
///   fetched; this engine buffers it, and an error with no alternative
///   takes its code and position from the FIRST token only, so an unclosed
///   comment seen as the second token of an alternate (behind a property
///   whose escaped quote hid it from the property scan) came out as
///   `unexpected` at the property. Dropping the unconsumed lookahead makes
///   the bad token the one the error reports, which is what the throw did.
///   The engine only does this with neither recovery nor relexing on; in
///   those modes the lookahead is left alone.
pub(crate) fn lex_subscriber(token: &mut Token, _rule: &mut Rule, ctx: &mut Context) {
    if TIN_ZZ == token.tin {
        if let Some(Value::Number(n)) = ctx.u.get(OVERSHOOT) {
            token.site.ci += *n as usize;
        }
    } else if TIN_BD == token.tin
        && !ctx.options.parse.recover.enabled
        && !ctx.options.lex.relex
        && ctx.t.first().is_some_and(|t| TIN_BD != t.tin)
    {
        ctx.t.clear();
    }
}

/// The number of astral characters (those two UTF-16 code units wide) in
/// scalars `[from, to)` of `src`.
pub(crate) fn astral_between(src: &str, from: usize, to: usize) -> usize {
    src.chars()
        .skip(from)
        .take(to.saturating_sub(from))
        .filter(|c| 2 == c.len_utf16())
        .count()
}

/// An engine error as this crate's, with the column in UTF-16 code units.
///
/// The engine counts a column in Unicode scalars since the last reset (a
/// `\n` in any token, or the end of a run of line characters); the canonical
/// port counts UTF-16 code units since the same points. The two differ by
/// the astral characters between the reset and the error, which is what is
/// added here. The message is the engine's, and is not the contract.
impl From<TabnasError> for Error {
    fn from(error: TabnasError) -> Error {
        let from = error.pos.saturating_sub(error.col.saturating_sub(1));
        let wide = astral_between(&error.full_source, from, error.pos);
        Error {
            code: error.code,
            message: error.detail,
            line: error.row,
            column: error.col + wide,
        }
    }
}
