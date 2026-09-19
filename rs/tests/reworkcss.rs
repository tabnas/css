/* Copyright (c) 2025 Richard Rodger and other contributors, MIT License */

//! LANGUAGE CONFORMANCE against the third-party reworkcss/css corpus.
//!
//! README.md and AGENTS.md both state that this plugin's AST "follows the
//! widely-used reworkcss/css model", so reworkcss's own
//! `test/cases/*/ast.json` files are the authoritative expected VALUES for a
//! parse — not merely accept/reject oracles. This runner compares the WHOLE
//! tree, both without and with source positions.
//!
//! The corpus is third-party and is NOT vendored. It is fetched (pinned) by
//! `scripts/fetch-reworkcss-tests.sh`, which [`fetch_corpus`] runs before any
//! test here, exactly as `TestMain` in `go/conformance_test.go` and `pretest`
//! in `ts/package.json` do for the other two runtimes.
//!
//! If the corpus is still absent afterwards these tests **FAIL**. They never
//! skip. A conformance suite that quietly does not run reports green while
//! measuring nothing, which is worse than no suite at all — that is not
//! hypothetical here: the Go conformance tests skipped on every CI run until
//! the fetch was added, so half the conformance claim was reported green
//! while measuring nothing.
//!
//! `ts/test/reworkcss.test.ts` and `go/reworkcss_test.go` run the SAME corpus
//! with the SAME derivation, so the three runtimes cannot drift without one
//! of them going red.

mod support;

use std::path::{Path, PathBuf};
use std::sync::Once;

use tabnas_css::{Css, Options, Value};

use support::{canonical, canonical_without, json, repo_root, without};

/// Pinned upstream. Keep in step with `scripts/fetch-reworkcss-tests.sh`,
/// `ts/test/reworkcss.test.ts` and `go/reworkcss_test.go`.
const UPSTREAM: &str = "https://github.com/reworkcss/css";
const SHA: &str = "ae6a6f9bf939cbcbc759a12d9f208afb5d4dde75";

/// Census of the pinned commit. A corpus that silently shrank must not
/// improve the conformance number unnoticed, so these are hard assertions.
const CASE_COUNT: usize = 46;
/// The cases the loops below compare. `cases/empty` is the other one, and it
/// is asserted on its own so that it stays EXPLICITLY checked. The Go and
/// TypeScript runners account for it the same way.
const COMPARED_COUNT: usize = 45;
const THROW_COUNT: usize = 4;
const NO_THROW_COUNT: usize = 2;
/// `parse(src, {silent: true})` — no tabnas equivalent.
const OPTIONED_COUNT: usize = 1;

/// A FAILURE message, not a skip message.
const ABSENT: &str = "MISSING CONFORMANCE CORPUS: reworkcss/css is not installed, so the \
     CSS conformance claim is UNVERIFIED. Fetch it (pinned) with \
     scripts/fetch-reworkcss-tests.sh. This test does NOT skip when the corpus is absent.";

fn corpus_dir() -> PathBuf {
    repo_root().join("test").join("reworkcss-css")
}

/// Run the pinned fetch once per test binary, then require the result.
///
/// The fetch's own exit status is deliberately not asserted on: when it
/// fails, the failure is reported by the check below — which names the
/// missing corpus and how to get it — rather than as an opaque process error.
fn fetch_corpus() {
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

fn read_case(path: &Path) -> String {
    let raw = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    // Byte-identical to upstream test/cases.js readFile, including its
    // SINGLE-occurrence CRLF replace, so the parser sees exactly the source
    // that produced the expected ASTs — line and column numbers depend on it.
    let src = raw.replacen("\r\n", "\n", 1);
    src.strip_suffix('\n').unwrap_or(&src).to_string()
}

/// Unwrap upstream's `{type, stylesheet: {rules}}` envelope; this plugin
/// emits `{type, rules}`.
fn upstream_rules(path: &Path) -> Value {
    let raw = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let ast = json(&raw).unwrap_or_else(|e| panic!("cannot parse {}: {e}", path.display()));
    ast.as_node()
        .and_then(|n| n.get("stylesheet"))
        .and_then(Value::as_node)
        .and_then(|n| n.get("rules"))
        .cloned()
        .unwrap_or_else(|| panic!("{}: no .stylesheet.rules", path.display()))
}

fn parse(src: &str, options: Options) -> Value {
    Css::with_options(options)
        .parse(src)
        .unwrap_or_else(|e| panic!("parse error: {e}"))
}

fn rules(value: &Value) -> Value {
    value
        .as_node()
        .and_then(|n| n.get("rules"))
        .cloned()
        .unwrap_or_else(|| panic!("parse produced no stylesheet: {}", value.to_json()))
}

fn case_names() -> Vec<String> {
    let dir = corpus_dir().join("test").join("cases");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(
        CASE_COUNT,
        names.len(),
        "expected {CASE_COUNT} cases at {UPSTREAM}@{SHA}, got {}; the pinned \
         corpus changed. Do not silently accept a different suite.",
        names.len()
    );
    names
}

/// The cases the comparison loops run over: every case but `cases/empty`,
/// which [`cases_empty`] asserts on its own.
fn compared_case_names() -> Vec<String> {
    case_names().into_iter().filter(|n| "empty" != n).collect()
}

/// `cases/empty` is asserted here rather than through the loops below, so the
/// case stays EXPLICITLY checked and can neither regress unnoticed nor
/// disappear from the suite.
///
/// NO LONGER DIVERGENT. A zero-length source used to yield nothing at all.
/// The cause was never a rule-iteration budget: the engine returns its empty
/// result for `""` before the rule loop is reachable, and the real cause was
/// that the plugin declared no empty result. All three ports now declare
/// `{type: 'stylesheet', rules: []}`, so `""` matches upstream.
#[test]
fn cases_empty() {
    fetch_corpus();
    let want = r#"{"rules":[],"type":"stylesheet"}"#;
    assert_eq!(want, canonical(&parse("", Options::default())));
    assert_eq!(want, canonical(&parse(" ", Options::default())));
}

/// The parse VALUE for every case in the corpus, positions removed — the
/// primary metric, since the `position` option is off by default.
#[test]
fn cases_ast() {
    fetch_corpus();
    let mut failures = Vec::new();
    let mut compared = 0;
    for name in compared_case_names() {
        let dir = corpus_dir().join("test").join("cases").join(&name);
        let input = read_case(&dir.join("input.css"));
        let want = canonical_without(&upstream_rules(&dir.join("ast.json")), &["position"]);
        let got = canonical_without(&rules(&parse(&input, Options::default())), &["position"]);
        compared += 1;
        if got != want {
            failures.push(format!("cases/{name}:\n  got  {got}\n  want {want}"));
        }
    }
    assert_eq!(COMPARED_COUNT, compared, "the compared set changed size");
    assert!(
        failures.is_empty(),
        "{} of {COMPARED_COUNT} corpus cases differ:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

/// The same trees WITH source positions, which upstream records for every
/// node — the secondary metric. `source` is upstream-only (a filename this
/// plugin has no notion of).
#[test]
fn cases_ast_with_position() {
    fetch_corpus();
    let options = Options {
        position: true,
        ..Options::default()
    };
    let mut failures = Vec::new();
    for name in case_names() {
        let dir = corpus_dir().join("test").join("cases").join(&name);
        let input = read_case(&dir.join("input.css"));
        let want = canonical(&without(
            &upstream_rules(&dir.join("ast.json")),
            &["source"],
        ));
        let got = canonical(&rules(&parse(&input, options)));
        if got != want {
            failures.push(format!(
                "cases/{name} (position):\n  got  {got}\n  want {want}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {CASE_COUNT} corpus cases differ with positions:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

/// One `assert.throws` / `assert.doesNotThrow` from upstream's `test/parse.js`.
struct ErrorCase {
    /// `throws` or `doesNotThrow`.
    kind: String,
    /// The CSS source, with the JavaScript literal's escapes decoded.
    src: String,
    /// Whether the call passed options — `parse(src, {silent: true})`.
    optioned: bool,
}

/// Extract the assertions mechanically from upstream's own `test/parse.js`,
/// so the must-fail set cannot drift from what reworkcss actually asserts.
///
/// This reads the same shape the Go runner's regular expression does —
/// `assert.X(function () { parse('…'[, …]) ` — so a change to upstream's
/// shape trips the census assertion rather than silently shrinking the suite.
fn extract_error_cases() -> Vec<ErrorCase> {
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

/// The inputs upstream asserts must, and must not, fail.
#[test]
fn accept_reject() {
    fetch_corpus();
    let cases = extract_error_cases();

    let optioned = cases.iter().filter(|c| c.optioned).count();
    let throws = cases
        .iter()
        .filter(|c| !c.optioned && "throws" == c.kind)
        .count();
    let no_throws = cases
        .iter()
        .filter(|c| !c.optioned && "throws" != c.kind)
        .count();
    assert!(
        THROW_COUNT == throws && NO_THROW_COUNT == no_throws && OPTIONED_COUNT == optioned,
        "upstream test/parse.js changed shape (throws={throws} doesNotThrow={no_throws} \
         optioned={optioned}, want {THROW_COUNT}/{NO_THROW_COUNT}/{OPTIONED_COUNT}); the \
         mechanical extraction is no longer reading the corpus it was written for. Fix \
         the extractor rather than accepting a smaller must-fail set."
    );

    let css = Css::new();
    let mut failures = Vec::new();
    for case in &cases {
        // parse(src, {silent: true}) asserts reworkcss's error-RECOVERY mode,
        // which this plugin does not implement and does not claim to. Not a
        // skip of a case this plugin is judged on: it is a case about a
        // different API.
        if case.optioned {
            assert_eq!(
                "doesNotThrow", case.kind,
                "unexpected optioned assertion kind {:?}",
                case.kind
            );
            continue;
        }
        let got = css.parse(&case.src);
        match (case.kind.as_str(), got) {
            ("throws", Ok(value)) => failures.push(format!(
                "[reject] {:?}: upstream reworkcss/css rejects this input; it must not \
                 parse, but it produced {}",
                case.src,
                value.to_json()
            )),
            ("doesNotThrow", Err(err)) => failures.push(format!(
                "[accept] {:?}: unexpected parse error: {err}",
                case.src
            )),
            _ => {}
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
