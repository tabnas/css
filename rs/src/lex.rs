/* Copyright (c) 2025 Richard Rodger, MIT License */

//! The lexer: the single `cssToken` matcher, the scanners it reads spans
//! with, and the token buffer the rule machine peeks into.
//!
//! CSS is context-sensitive — the same characters can begin a selector, a
//! property or a value — so the lexer owns the hard tokenisation and the
//! grammar only assembles typed nodes from what it emits. Which token comes
//! out depends on the rule that is active when the character is read, which is
//! why [`Lex::peek`] takes a rule name.
//!
//! See `ts/src/css.ts` for the canonical commentary; the three ports are kept
//! in step.

use std::fmt;

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

/// One lexed token.
#[derive(Clone, Debug)]
pub struct Token {
    /// The token kind.
    pub tin: Tin,
    /// The token's semantic value: the selector, property, value, comment
    /// text, or at-rule keyword.
    pub val: String,
    /// The source text the token consumed, used to derive its end position.
    pub src: String,
    /// 1-based line of the token's first character.
    pub r_i: usize,
    /// 1-based column (in UTF-16 code units) of the token's first character.
    pub c_i: usize,
    /// An at-rule's prelude (block at-rules) or params (statement at-rules).
    /// The canonical port carries this in the token's `use` field.
    pub use_text: String,
}

/// A parse failure.
///
/// `code` is the contract: the shared fixtures pin `ERROR:<code>` and compare
/// it exactly. This crate raises only the two codes the canonical port
/// inherits from the engine — `unterminated_comment` and `unexpected` — and
/// declares none of its own, matching `tabnas.plugin.json` (`errorCodes: []`).
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

/// The lexer's scan position.
#[derive(Clone, Copy, Debug)]
pub struct Point {
    /// Byte offset into the source.
    pub s_i: usize,
    /// 1-based line.
    pub r_i: usize,
    /// 1-based column, in UTF-16 code units.
    pub c_i: usize,
}

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

/// The lexer, plus the token buffer the rule machine peeks into.
///
/// Tokens are lexed lazily: [`Lex::peek`] produces only as many as the alt
/// being tried needs, under the rule that is trying it. A token already in the
/// buffer is NOT re-lexed when a later rule looks at it — that caching is what
/// makes a comment right after `{` a node (read under `declbody`) while a
/// comment between a property and its `:` is not (read under `decl`).
pub struct Lex {
    src: String,
    bytes: Vec<u8>,
    pnt: Point,
    buf: Vec<Token>,
    lowercase_properties: bool,
}

impl Lex {
    /// A lexer over `src`.
    pub fn new(src: &str, lowercase_properties: bool) -> Lex {
        Lex {
            src: src.to_string(),
            bytes: src.as_bytes().to_vec(),
            pnt: Point {
                s_i: 0,
                r_i: 1,
                c_i: 1,
            },
            buf: Vec::new(),
            lowercase_properties,
        }
    }

    /// Ensure the buffer holds `n` tokens, lexing any shortfall under `rule`,
    /// and return them.
    pub fn peek(&mut self, n: usize, rule: &str) -> Result<&[Token], Error> {
        while self.buf.len() < n {
            let tok = self.lex_one(rule)?;
            self.buf.push(tok);
        }
        Ok(&self.buf[..n])
    }

    /// Drop the first `n` buffered tokens. Anything left stays cached.
    pub fn consume(&mut self, n: usize) {
        let n = n.min(self.buf.len());
        self.buf.drain(..n);
    }

    /// The current scan position, for error reporting.
    pub fn point(&self) -> Point {
        self.pnt
    }

    fn err(&self, code: &str, message: String) -> Error {
        Error {
            code: code.to_string(),
            message,
            line: self.pnt.r_i,
            column: self.pnt.c_i,
        }
    }

    /// Lex one token under `rule`.
    ///
    /// The `cssToken` matcher runs first and owns all non-fixed text. What it
    /// declines falls through to the builtins: a `/* */` comment away from a
    /// list position is skipped as whitespace, and `{` `}` `:` `;` lex as
    /// fixed punctuation. Nothing else is CSS.
    fn lex_one(&mut self, rule: &str) -> Result<Token, Error> {
        loop {
            self.skip_space();
            if self.pnt.s_i >= self.bytes.len() {
                return Ok(Token {
                    tin: Tin::Zz,
                    val: String::new(),
                    src: String::new(),
                    r_i: self.pnt.r_i,
                    c_i: self.pnt.c_i,
                    use_text: String::new(),
                });
            }
            if let Some(tok) = self.css_token(rule)? {
                return Ok(tok);
            }

            let s_i = self.pnt.s_i;
            let c = self.bytes[s_i];

            // A comment the matcher declined: skipped as insignificant
            // whitespace, exactly as the engine's builtin comment matcher
            // does — and, like it, an unclosed one is an error rather than a
            // comment running to end of input.
            if c == b'/' && self.bytes.get(s_i + 1) == Some(&b'*') {
                let e = skip_comment(&self.bytes, s_i);
                if UNTERMINATED == e {
                    return Err(self.err(
                        "unterminated_comment",
                        "unterminated comment: no closing '*/'".to_string(),
                    ));
                }
                self.advance(s_i, e);
                continue;
            }

            let tin = match c {
                b'{' => Tin::Ob,
                b'}' => Tin::Cb,
                b':' => Tin::Cl,
                b';' => Tin::Ca,
                _ => {
                    let end = next_char_end(&self.bytes, s_i);
                    return Err(self.err(
                        "unexpected",
                        format!("unexpected character(s): {}", &self.src[s_i..end]),
                    ));
                }
            };
            let text = (c as char).to_string();
            let tok = self.make(tin, text.clone(), text, String::new());
            self.advance(s_i, s_i + 1);
            return Ok(tok);
        }
    }

    /// Whitespace the `cssToken` matcher defers to the engine's space and
    /// line matchers.
    ///
    /// `\r` is a LINE character, not a space one: it resets the column
    /// without starting a new line, so `a{}\r` ends at column 1 and
    /// `/*x*/\r/*x*/` ends at column 6 rather than 12. That is the engine's
    /// behaviour and this port reproduces it. A `\r` INSIDE a token the
    /// matcher consumes — in a selector, a value or a comment body — is not
    /// whitespace at all and counts as one column, which is what
    /// [`Lex::advance`] and [`end_pos`] do.
    fn skip_space(&mut self) {
        while self.pnt.s_i < self.bytes.len() {
            match self.bytes[self.pnt.s_i] {
                b' ' | b'\t' => {
                    self.pnt.s_i += 1;
                    self.pnt.c_i += 1;
                }
                b'\r' => {
                    self.pnt.s_i += 1;
                    self.pnt.c_i = 1;
                }
                b'\n' => {
                    self.pnt.s_i += 1;
                    self.pnt.r_i += 1;
                    self.pnt.c_i = 1;
                }
                _ => return,
            }
        }
    }

    /// Build a token at the current point.
    fn make(&self, tin: Tin, val: String, src: String, use_text: String) -> Token {
        Token {
            tin,
            val,
            src,
            r_i: self.pnt.r_i,
            c_i: self.pnt.c_i,
            use_text,
        }
    }

    /// Advance the scan point to `end`, updating line and column across any
    /// newlines in `[s_i, end)`.
    ///
    /// This matcher consumes multi-line runs the engine's space and line
    /// matchers never see, so it tracks lines itself. Columns are UTF-16 code
    /// units — what a JavaScript string index counts — so that this port
    /// reports the same column as the canonical one for `#©{…}` (where a byte
    /// count is wrong) and for `#𝄞{…}` (where a `char` count is wrong).
    ///
    /// `end` may be ONE PAST the source, because the scanners deliberately
    /// overshoot: a `\` at the last character is an escape whose escaped
    /// character is not there, and the `i += 2` step runs off the end. The
    /// canonical port advances its column by the unclamped width and takes
    /// the CLAMPED span for the token text, so `@host\` yields a node ending
    /// at column 7 inside a stylesheet ending at column 8 — one past a source
    /// six characters long. This port reproduces that rather than tidying it,
    /// because TypeScript is canonical (see the authority rules in
    /// AGENTS.md); the Go port clamps instead, and that difference is
    /// recorded there as a known divergence.
    fn advance(&mut self, s_i: usize, end: usize) {
        let stop = end.min(self.bytes.len());
        let overshoot = end - stop;
        let span = safe_slice(&self.src, s_i, stop);
        match span.rfind('\n') {
            Some(last_nl) => {
                self.pnt.r_i += span.matches('\n').count();
                self.pnt.c_i = 1 + col_width(&span[last_nl + 1..]) + overshoot;
            }
            None => self.pnt.c_i += col_width(span) + overshoot,
        }
        self.pnt.s_i = end;
    }

    /// The single `cssToken` matcher. `None` means "declined": the character
    /// belongs to a builtin matcher.
    fn css_token(&mut self, rule: &str) -> Result<Option<Token>, Error> {
        let s_i = self.pnt.s_i;
        let c = self.bytes[s_i];

        // Comments: a node at a list position, otherwise deferred (and
        // skipped by the caller).
        if c == b'/' && self.bytes.get(s_i + 1) == Some(&b'*') {
            if !is_comment_node_rule(rule) {
                return Ok(None);
            }
            let e = match find_comment_end(&self.bytes, s_i) {
                Some(e) => e,
                None => {
                    return Err(self.err(
                        "unterminated_comment",
                        "unterminated comment: no closing '*/'".to_string(),
                    ))
                }
            };
            let tok = self.make(
                Tin::Cc,
                safe_slice(&self.src, s_i + 2, e).to_string(),
                safe_slice(&self.src, s_i, e + 2).to_string(),
                String::new(),
            );
            self.advance(s_i, e + 2);
            return Ok(Some(tok));
        }

        // Value position: read a declaration value up to the next top-level
        // `;`/`}` and emit one #VL (comments stripped, surrounding space
        // trimmed).
        if "declval" == rule {
            if c == b'{' || c == b'}' || c == b';' || c == b':' {
                return Ok(None);
            }
            let end = self.scan_or_bad(scan_value_end(&self.bytes, s_i))?;
            let raw = safe_slice(&self.src, s_i, end);
            let tok = self.make(
                Tin::Vl,
                strip_comments(raw).trim().to_string(),
                raw.to_string(),
                String::new(),
            );
            self.advance(s_i, end);
            return Ok(Some(tok));
        }

        // A top-level selector-group comma.
        if c == b',' {
            let tok = self.make(Tin::Gc, ",".to_string(), ",".to_string(), String::new());
            self.advance(s_i, s_i + 1);
            return Ok(Some(tok));
        }

        // An at-rule.
        if c == b'@' {
            return self.at_rule(s_i).map(Some);
        }

        // Other fixed punctuation belongs to the grammar.
        if c == b'{' || c == b'}' || c == b';' {
            return Ok(None);
        }

        // A selector or a property name, by `{`-before-`;` lookahead.
        let (kind, index) = scan_to_brace_or_end(&self.bytes, s_i);
        self.scan_or_bad(index)?;
        if Kind::Selector == kind {
            // One selector of a (possible) group: up to the next top-level
            // `,`/`{`, comments stripped and surrounding space trimmed.
            let end = self.scan_or_bad(scan_selector_end(&self.bytes, s_i))?;
            let raw = safe_slice(&self.src, s_i, end);
            let tok = self.make(
                Tin::Tx,
                strip_comments(raw).trim().to_string(),
                raw.to_string(),
                String::new(),
            );
            self.advance(s_i, end);
            return Ok(Some(tok));
        }

        // A property name: the run of property characters up to `:`,
        // whitespace, `;` or `}`.
        let e_i = scan_prop_end(&self.bytes, s_i);
        if e_i == s_i {
            return Ok(None);
        }
        let raw = safe_slice(&self.src, s_i, e_i);
        // `/` and `*` are property characters (the `*prop` / `//prop` IE
        // hacks), so a trailing `/*...*/` hack comment lands inside the
        // scanned name; strip comments out of it, as reworkcss does.
        let mut prop = strip_comments(raw);
        if prop.is_empty() {
            return Ok(None);
        }
        if self.lowercase_properties {
            prop = prop.to_lowercase();
        }
        let tok = self.make(Tin::Tx, prop, raw.to_string(), String::new());
        self.advance(s_i, e_i);
        Ok(Some(tok))
    }

    /// Lex an at-rule starting at `@`.
    ///
    /// Classified block-vs-statement by a `{`-before-`;` lookahead and, for
    /// blocks, by keyword: `#ATK` for keyframes, `#ATD` for a declarations
    /// body, `#ATR` otherwise. The keyword rides in `val` and the
    /// prelude/params in `use_text`.
    fn at_rule(&mut self, s_i: usize) -> Result<Token, Error> {
        let mut k_end = s_i + 1;
        while k_end < self.bytes.len() && is_at_char(self.bytes[k_end]) {
            k_end += 1;
        }
        let kw = safe_slice(&self.src, s_i + 1, k_end).to_string();

        let (kind, index) = scan_to_brace_or_end(&self.bytes, s_i);
        let index = self.scan_or_bad(index)?;
        if Kind::Selector == kind {
            // Block at-rule: the prelude is the text between the keyword and
            // `{`. At-rule preludes KEEP their comments (they are only
            // trimmed), matching reworkcss.
            let prelude = safe_slice(&self.src, k_end, index).trim().to_string();
            let tin = if is_keyframes_kw(&kw) {
                Tin::Atk
            } else if DECLS_KW.contains(&kw.as_str()) {
                Tin::Atd
            } else {
                Tin::Atr
            };
            let tok = self.make(
                tin,
                kw,
                safe_slice(&self.src, s_i, index).to_string(),
                prelude,
            );
            self.advance(s_i, index);
            return Ok(tok);
        }

        // Statement at-rule: params run up to the next top-level `;`/`}`. A
        // `;` is consumed (it terminates the statement); a `}` or end of
        // input is left for the enclosing rule.
        let p_end = self.scan_or_bad(scan_value_end(&self.bytes, k_end))?;
        let params = safe_slice(&self.src, k_end, p_end).trim().to_string();
        let end = if self.bytes.get(p_end) == Some(&b';') {
            p_end + 1
        } else {
            p_end
        };
        let tok = self.make(
            Tin::Ats,
            kw,
            safe_slice(&self.src, s_i, end).to_string(),
            params,
        );
        self.advance(s_i, end);
        Ok(tok)
    }

    /// Turn a scanner's [`UNTERMINATED`] into the error it stands for.
    fn scan_or_bad(&self, index: usize) -> Result<usize, Error> {
        if UNTERMINATED == index {
            return Err(self.err(
                "unterminated_comment",
                "unterminated comment: no closing '*/'".to_string(),
            ));
        }
        Ok(index)
    }
}

/// The column width of `s`: its length in UTF-16 code units, which is what a
/// JavaScript string index counts.
fn col_width(s: &str) -> usize {
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

/// The end of the UTF-8 character starting at `i`.
fn next_char_end(src: &[u8], i: usize) -> usize {
    let mut e = i + 1;
    while e < src.len() && (src[e] & 0xC0) == 0x80 {
        e += 1;
    }
    e.min(src.len())
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
    // column arithmetic counts it. See [`Lex::advance`] and [`safe_slice`].
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
        let one = strip_comments(safe_slice(prelude, i, end))
            .trim()
            .to_string();
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

/// A token's first-character position (1-based line and column).
pub fn start_pos(tok: &Token) -> (usize, usize) {
    (tok.r_i, tok.c_i)
}

/// The position just after a token's last character.
pub fn end_pos(tok: &Token) -> (usize, usize) {
    match tok.src.rfind('\n') {
        Some(last_nl) => (
            tok.r_i + tok.src.matches('\n').count(),
            1 + col_width(&tok.src[last_nl + 1..]),
        ),
        None => (tok.r_i, tok.c_i + col_width(&tok.src)),
    }
}
