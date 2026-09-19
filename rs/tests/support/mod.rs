/* Copyright (c) 2025 Richard Rodger and other contributors, MIT License */

//! Shared test support: finding the repository, reading the shared `.tsv`
//! fixtures, reading JSON, and comparing two ASTs by value.
//!
//! The TypeScript and Go runners get all of this from `@tabnas/support`,
//! whose two halves cannot drift from each other. There is no Rust half, so
//! this module is it — deliberately small, and written to the SAME fixture
//! contract `test/AGENTS.md` sets out, so a row means the same thing in all
//! three runtimes.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

use tabnas_css::{Node, Value};

/// The repository root: the directory holding `test/spec`.
///
/// Walks up from this crate rather than counting `..` hops, so moving the
/// suite does not mean recounting them.
pub fn repo_root() -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        if dir.join("test").join("spec").is_dir() {
            return dir;
        }
        if !dir.pop() {
            panic!(
                "cannot find the repository root (no ancestor of {} holds test/spec)",
                env!("CARGO_MANIFEST_DIR")
            );
        }
    }
}

/// The shared fixture directory, `test/spec`.
pub fn spec_dir() -> PathBuf {
    repo_root().join("test").join("spec")
}

/// One fixture row.
pub struct Row {
    /// The fixture file this row came from.
    pub file: String,
    /// The 1-based line within that file, for `<file>:<line>` failures.
    pub line: usize,
    /// The CSS source, with `\n` `\r` `\t` `\\` decoded.
    pub input: String,
    /// The expected value: raw JSON, or `ERROR` / `ERROR:<code>`.
    pub expected: String,
    /// The row's plugin options, as raw JSON (empty means defaults).
    pub opts: String,
}

impl Row {
    /// `<file>:<line>`, the label a failure is reported under.
    pub fn label(&self) -> String {
        format!("{}:{}", self.file, self.line)
    }
}

/// Read every `.tsv` fixture in `dir`.
///
/// An empty fixture, and a directory with no fixtures in it, both PANIC. A
/// runner that reports green having run nothing is indistinguishable from
/// coverage that was never there.
pub fn read_spec_dir(dir: &Path) -> Vec<Row> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| "tsv" == e))
        .collect();
    files.sort();
    assert!(
        !files.is_empty(),
        "no .tsv fixtures in {} — a fixture runner with nothing to run \
         reports green while measuring nothing",
        dir.display()
    );

    let mut rows = Vec::new();
    for path in files {
        let before = rows.len();
        rows.extend(read_spec_file(&path));
        assert!(
            before < rows.len(),
            "{} holds no fixture rows",
            path.display()
        );
    }
    rows
}

/// Read one `.tsv` fixture.
///
/// Blank lines are skipped, and so are comment lines — a line starting with
/// `#` that contains no tab. (A data row always has at least one tab, so a
/// `#`-leading source such as a C preprocessor directive still works.)
pub fn read_spec_file(path: &Path) -> Vec<Row> {
    let file = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));

    let mut header: Option<Vec<String>> = None;
    let mut rows = Vec::new();
    for (i, line) in text.split('\n').enumerate() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.trim().is_empty() {
            continue;
        }
        if line.starts_with('#') && !line.contains('\t') {
            continue;
        }
        let cols: Vec<String> = line.split('\t').map(str::to_string).collect();
        let Some(header) = &header else {
            header = Some(cols);
            continue;
        };
        let named = |key: &str| -> String {
            header
                .iter()
                .position(|h| h == key)
                .and_then(|i| cols.get(i))
                .cloned()
                .unwrap_or_default()
        };
        rows.push(Row {
            file: file.clone(),
            line: i + 1,
            input: decode(&named("input")),
            expected: named("expected"),
            opts: named("opts"),
        });
    }
    rows
}

/// Decode the `\n` `\r` `\t` `\\` escapes the `input` column uses.
///
/// `expected` and `opts` are NOT decoded — they are raw JSON, so JSON's own
/// escape rules apply there.
pub fn decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if '\\' != c {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// A value's canonical JSON: keys sorted, `Undefined` keys dropped.
///
/// Comparing canonical forms is what lets a fixture's `expected` be written
/// in any key order, exactly as the TypeScript and Go runners compare after a
/// JSON round-trip.
pub fn canonical(value: &Value) -> String {
    let mut out = String::new();
    write_canonical(value, &mut out);
    out
}

/// The canonical JSON of a value with some keys removed, recursively. Used to
/// drop `position` (this plugin's is opt-in) and upstream's `source` (a
/// filename this plugin has no notion of).
pub fn canonical_without(value: &Value, drop: &[&str]) -> String {
    canonical(&without(value, drop))
}

/// Remove `drop` keys from a value, recursively.
pub fn without(value: &Value, drop: &[&str]) -> Value {
    match value {
        Value::List(items) => Value::List(items.iter().map(|v| without(v, drop)).collect()),
        Value::Node(node) => {
            let mut out = Node::new();
            for (k, v) in node.iter() {
                if drop.contains(&k) {
                    continue;
                }
                out.set(k, without(v, drop));
            }
            Value::Node(out)
        }
        other => other.clone(),
    }
}

fn write_canonical(value: &Value, out: &mut String) {
    match value {
        Value::Node(node) => {
            let mut keys: Vec<(&str, &Value)> = node
                .iter()
                .filter(|(_, v)| !matches!(v, Value::Undefined))
                .collect();
            keys.sort_by(|a, b| a.0.cmp(b.0));
            out.push('{');
            for (i, (k, v)) in keys.iter().enumerate() {
                if 0 < i {
                    out.push(',');
                }
                out.push_str(&Value::Str((*k).to_string()).to_json());
                out.push(':');
                write_canonical(v, out);
            }
            out.push('}');
        }
        Value::List(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if 0 < i {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        other => out.push_str(&other.to_json()),
    }
}

// --- A minimal JSON reader -------------------------------------------------
//
// The fixtures' `expected` column and the reworkcss corpus's `ast.json` files
// are JSON, and this crate has no dependencies — so the tests read JSON with
// this rather than pulling serde in for the test profile alone. It is only
// ever pointed at files in this repository.

/// Parse JSON into a [`Value`].
pub fn json(text: &str) -> Result<Value, String> {
    let bytes = text.as_bytes();
    let mut i = 0;
    let value = json_value(bytes, &mut i)?;
    json_space(bytes, &mut i);
    if i < bytes.len() {
        return Err(format!("trailing JSON content at byte {i}"));
    }
    Ok(value)
}

fn json_space(src: &[u8], i: &mut usize) {
    while *i < src.len() && matches!(src[*i], b' ' | b'\t' | b'\r' | b'\n') {
        *i += 1;
    }
}

fn json_value(src: &[u8], i: &mut usize) -> Result<Value, String> {
    json_space(src, i);
    match src.get(*i) {
        None => Err("unexpected end of JSON".to_string()),
        Some(b'{') => json_object(src, i),
        Some(b'[') => json_array(src, i),
        Some(b'"') => Ok(Value::Str(json_string(src, i)?)),
        Some(b't') => json_literal(src, i, "true", Value::Bool(true)),
        Some(b'f') => json_literal(src, i, "false", Value::Bool(false)),
        Some(b'n') => json_literal(src, i, "null", Value::Null),
        Some(_) => json_number(src, i),
    }
}

fn json_literal(src: &[u8], i: &mut usize, word: &str, value: Value) -> Result<Value, String> {
    if src[*i..].starts_with(word.as_bytes()) {
        *i += word.len();
        return Ok(value);
    }
    Err(format!("bad JSON literal at byte {i}", i = *i))
}

fn json_number(src: &[u8], i: &mut usize) -> Result<Value, String> {
    let start = *i;
    while *i < src.len() && matches!(src[*i], b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9') {
        *i += 1;
    }
    std::str::from_utf8(&src[start..*i])
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .map(Value::Num)
        .ok_or_else(|| format!("bad JSON number at byte {start}"))
}

fn json_string(src: &[u8], i: &mut usize) -> Result<String, String> {
    *i += 1; // the opening quote
    let mut out = String::new();
    while *i < src.len() {
        match src[*i] {
            b'"' => {
                *i += 1;
                return Ok(out);
            }
            b'\\' => {
                *i += 1;
                let esc = *src.get(*i).ok_or("unterminated JSON escape")?;
                *i += 1;
                match esc {
                    b'n' => out.push('\n'),
                    b'r' => out.push('\r'),
                    b't' => out.push('\t'),
                    b'b' => out.push('\u{08}'),
                    b'f' => out.push('\u{0c}'),
                    b'u' => out.push(json_unicode(src, i)?),
                    other => out.push(other as char),
                }
            }
            _ => {
                let end = json_char_end(src, *i);
                out.push_str(&String::from_utf8_lossy(&src[*i..end]));
                *i = end;
            }
        }
    }
    Err("unterminated JSON string".to_string())
}

/// A `\uXXXX` escape, pairing surrogates so astral characters survive.
fn json_unicode(src: &[u8], i: &mut usize) -> Result<char, String> {
    let unit = json_hex4(src, i)?;
    if !(0xD800..0xDC00).contains(&unit) {
        return char::from_u32(unit).ok_or_else(|| format!("bad \\u escape {unit:04x}"));
    }
    if src.get(*i) != Some(&b'\\') || src.get(*i + 1) != Some(&b'u') {
        return Err("lone high surrogate in JSON".to_string());
    }
    *i += 2;
    let low = json_hex4(src, i)?;
    let combined = 0x10000 + ((unit - 0xD800) << 10) + (low - 0xDC00);
    char::from_u32(combined).ok_or_else(|| format!("bad surrogate pair {unit:04x}{low:04x}"))
}

fn json_hex4(src: &[u8], i: &mut usize) -> Result<u32, String> {
    let hex = src.get(*i..*i + 4).ok_or("truncated \\u escape in JSON")?;
    *i += 4;
    u32::from_str_radix(&String::from_utf8_lossy(hex), 16)
        .map_err(|e| format!("bad \\u escape: {e}"))
}

fn json_char_end(src: &[u8], i: usize) -> usize {
    let mut e = i + 1;
    while e < src.len() && (src[e] & 0xC0) == 0x80 {
        e += 1;
    }
    e.min(src.len())
}

fn json_object(src: &[u8], i: &mut usize) -> Result<Value, String> {
    *i += 1; // '{'
    let mut node = Node::new();
    loop {
        json_space(src, i);
        match src.get(*i) {
            None => return Err("unterminated JSON object".to_string()),
            Some(b'}') => {
                *i += 1;
                return Ok(Value::Node(node));
            }
            Some(b',') => {
                *i += 1;
                continue;
            }
            Some(b'"') => {}
            Some(c) => return Err(format!("unexpected {:?} in JSON object", *c as char)),
        }
        let key = json_string(src, i)?;
        json_space(src, i);
        if src.get(*i) != Some(&b':') {
            return Err(format!("expected ':' after JSON key {key:?}"));
        }
        *i += 1;
        let value = json_value(src, i)?;
        node.set(key, value);
    }
}

fn json_array(src: &[u8], i: &mut usize) -> Result<Value, String> {
    *i += 1; // '['
    let mut items = Vec::new();
    loop {
        json_space(src, i);
        match src.get(*i) {
            None => return Err("unterminated JSON array".to_string()),
            Some(b']') => {
                *i += 1;
                return Ok(Value::List(items));
            }
            Some(b',') => {
                *i += 1;
                continue;
            }
            Some(_) => items.push(json_value(src, i)?),
        }
    }
}
