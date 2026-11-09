/* Copyright (c) 2025 Richard Rodger and other contributors, MIT License */

//! The reworkcss/css conformance corpus, as the tests read it: where it is,
//! the pinned fetch, its cases' inputs, and the accept/reject inputs of its
//! `test/parse.js`. `tests/reworkcss.rs` holds the parse to the corpus, and
//! `tests/reader.rs` holds a parse from a reader to the parse of the same
//! text; both read it through here, so the two cannot read it differently.
//! See `tests/reworkcss.rs` for what the corpus is and why it is fetched,
//! never skipped.

use std::path::{Path, PathBuf};
use std::sync::Once;

use super::repo_root;

/// A FAILURE message, not a skip message.
pub const ABSENT: &str = "MISSING CONFORMANCE CORPUS: reworkcss/css is not installed, so the \
     CSS conformance claim is UNVERIFIED. Fetch it (pinned) with \
     scripts/fetch-reworkcss-tests.sh. This test does NOT skip when the corpus is absent.";

pub fn corpus_dir() -> PathBuf {
    repo_root().join("test").join("reworkcss-css")
}

/// Run the pinned fetch once per test binary, then require the result.
///
/// The fetch's own exit status is deliberately not asserted on: when it
/// fails, the failure is reported by the check below — which names the
/// missing corpus and how to get it — rather than as an opaque process error.
pub fn fetch_corpus() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let script = repo_root().join("scripts").join("fetch-reworkcss-tests.sh");
        let _ = std::process::Command::new("bash").arg(&script).status();
    });
    assert!(
        corpus_dir().join("test").join("parse.js").is_file(),
        "{ABSENT}"
    );
}

pub fn read_case(path: &Path) -> String {
    let raw = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    // Byte-identical to upstream test/cases.js readFile, including its
    // SINGLE-occurrence CRLF replace, so the parser sees exactly the source
    // that produced the expected ASTs — line and column numbers depend on it.
    let src = raw.replacen("\r\n", "\n", 1);
    src.strip_suffix('\n').unwrap_or(&src).to_string()
}

/// One `assert.throws` / `assert.doesNotThrow` from upstream's `test/parse.js`.
pub struct ErrorCase {
    /// `throws` or `doesNotThrow`.
    pub kind: String,
    /// The CSS source, with the JavaScript literal's escapes decoded.
    pub src: String,
    /// Whether the call passed options — `parse(src, {silent: true})`.
    pub optioned: bool,
}

/// Extract the assertions mechanically from upstream's own `test/parse.js`,
/// so the must-fail set cannot drift from what reworkcss actually asserts.
///
/// This reads the same shape the Go runner's regular expression does —
/// `assert.X(function () { parse('…'[, …]) ` — so a change to upstream's
/// shape trips the census assertion rather than silently shrinking the suite.
pub fn extract_error_cases() -> Vec<ErrorCase> {
    let path = corpus_dir().join("test").join("parse.js");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read upstream test/parse.js: {e}"));
    let b = text.as_bytes();

    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let Some(at) = find(b, i, "assert.") else {
            break;
        };
        i = at + "assert.".len();
        let kind = if b[i..].starts_with(b"throws(") {
            "throws"
        } else if b[i..].starts_with(b"doesNotThrow(") {
            "doesNotThrow"
        } else {
            continue;
        };
        i += kind.len() + 1;

        // `\s*function\s*\(\)\s*\{\s*parse\('`
        let mut j = i;
        if !(word(b, &mut j, "function")
            && word(b, &mut j, "(")
            && word(b, &mut j, ")")
            && word(b, &mut j, "{")
            && word(b, &mut j, "parse")
            && word(b, &mut j, "(")
            && word(b, &mut j, "'"))
        {
            continue;
        }

        // The single-quoted literal, then an optional `, {...}` argument.
        let start = j;
        while j < b.len() {
            match b[j] {
                b'\\' => j += 2,
                b'\'' => break,
                _ => j += 1,
            }
        }
        if b.len() <= j {
            break;
        }
        let src = decode_js(&text[start..j]);
        j += 1;
        while j < b.len() && b[j].is_ascii_whitespace() {
            j += 1;
        }
        let optioned = b.get(j) == Some(&b',');
        out.push(ErrorCase {
            kind: kind.to_string(),
            src,
            optioned,
        });
        i = j;
    }
    out
}

fn find(b: &[u8], from: usize, needle: &str) -> Option<usize> {
    let n = needle.as_bytes();
    (from..b.len().saturating_sub(n.len() - 1)).find(|&i| b[i..].starts_with(n))
}

/// Skip whitespace, then require `word`.
fn word(b: &[u8], i: &mut usize, word: &str) -> bool {
    while *i < b.len() && b[*i].is_ascii_whitespace() {
        *i += 1;
    }
    if b[*i..].starts_with(word.as_bytes()) {
        *i += word.len();
        return true;
    }
    false
}

/// Decode a JavaScript single-quoted string literal.
fn decode_js(lit: &str) -> String {
    let mut out = String::new();
    let mut chars = lit.chars();
    while let Some(c) = chars.next() {
        if '\\' != c {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('f') => out.push('\u{0c}'),
            Some('b') => out.push('\u{08}'),
            Some('v') => out.push('\u{0b}'),
            Some('0') => out.push('\0'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}
