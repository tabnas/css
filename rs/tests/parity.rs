/* Copyright (c) 2025 Richard Rodger and other contributors, MIT License */

//! Cross-runtime conformance, driven by the shared `test/spec/*.tsv`
//! fixtures at the repository root (see `test/AGENTS.md`).
//!
//! The TypeScript runner (`ts/test/parity.test.ts`) and the Go runner
//! (`go/parity_test.go`) read the SAME files, so the three implementations
//! cannot drift without one of them going red. What is specific to this port
//! is only how a row's options build a parser.
//!
//! Fixtures are discovered by listing the directory, so adding a `.tsv` runs
//! it here without touching this file.

mod support;

use tabnas_css::{Css, Options};

use support::{canonical, json, read_spec_dir, spec_dir, Row};

#[test]
fn shared_fixtures() {
    let rows = read_spec_dir(&spec_dir());
    let mut failures = Vec::new();
    for row in &rows {
        if let Err(why) = check(row) {
            failures.push(format!("{}\n{why}", row.label()));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} shared fixture rows failed:\n\n{}",
        failures.len(),
        rows.len(),
        failures.join("\n\n")
    );
}

fn check(row: &Row) -> Result<(), String> {
    // A fresh parser per row: the `opts` column is per-case, and plugin
    // options must not leak from one row into the next.
    let css = Css::with_options(options(&row.opts)?);
    let expected = row.expected.trim();

    match css.parse(&row.input) {
        Err(err) => {
            if !expected.starts_with("ERROR") {
                return Err(format!(
                    "  input    {:?}\n  expected {expected}\n  got      ERROR:{} ({err})",
                    row.input, err.code
                ));
            }
            // `ERROR` alone asserts only that the document is rejected;
            // `ERROR:<code>` compares the code EXACTLY — it is the error's
            // code, not a substring of its message.
            match expected.strip_prefix("ERROR:") {
                Some(want) if want != err.code => Err(format!(
                    "  input    {:?}\n  expected ERROR:{want}\n  got      ERROR:{}",
                    row.input, err.code
                )),
                _ => Ok(()),
            }
        }
        Ok(got) => {
            if expected.starts_with("ERROR") {
                return Err(format!(
                    "  input    {:?}\n  expected {expected}\n  got      {}",
                    row.input,
                    got.to_json()
                ));
            }
            let want = json(expected).map_err(|e| format!("  unreadable expected JSON: {e}"))?;
            if canonical(&got) != canonical(&want) {
                return Err(format!(
                    "  input    {:?}\n  expected {}\n  got      {}",
                    row.input,
                    canonical(&want),
                    canonical(&got)
                ));
            }
            Ok(())
        }
    }
}

/// Build this row's options from its `opts` column, which is raw JSON using
/// the canonical port's camelCase names.
fn options(opts: &str) -> Result<Options, String> {
    if opts.trim().is_empty() {
        return Ok(Options::default());
    }
    let parsed = json(opts).map_err(|e| format!("  unreadable opts JSON {opts:?}: {e}"))?;
    let node = parsed
        .as_node()
        .ok_or_else(|| format!("  opts is not an object: {opts:?}"))?;
    let flag = |key: &str| matches!(node.get(key), Some(tabnas_css::Value::Bool(true)));
    Ok(Options {
        lowercase_properties: flag("lowercaseProperties"),
        position: flag("position"),
    })
}
