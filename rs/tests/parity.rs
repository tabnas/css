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
//!
//! Every row runs three ways: through [`Css::parse`], through the engine's
//! own API with the plugin on jsonic (the tree form a plugin user gets), and
//! through the plugin on a bare engine. The last is what `ts/test/leniency.test.ts`
//! checks in the canonical port: the verdicts do not come from jsonic.

mod support;

use tabnas_css::{css, make_with, Css, Options, Value};

use support::{canonical, json, read_spec_dir, spec_dir, Row};

/// A parse outcome in the fixtures' terms: the AST, or the error's code and
/// its rendered report.
type Outcome = Result<Value, (String, String)>;

/// Through the crate's own entry point.
fn through_css(src: &str, options: Options) -> Outcome {
    Css::with_options(options)
        .parse(src)
        .map_err(|err| (err.code.clone(), err.to_string()))
}

/// Through the engine's own API, on jsonic, as a plugin user parses.
fn through_plugin(src: &str, options: Options) -> Outcome {
    tree(make_with(options).parse(src))
}

/// Through the plugin on a bare engine: the Rust half of the canonical
/// port's `new Tabnas().use(Css)` check, which gives the same verdicts
/// without jsonic underneath.
fn through_bare_engine(src: &str, options: Options) -> Outcome {
    let mut parser = tabnas::Tabnas::new();
    css(&mut parser, &options).expect("the plugin installs on a bare engine");
    tree(parser.parse(src))
}

fn tree(result: Result<tabnas::Value, tabnas::TabnasError>) -> Outcome {
    match result {
        Ok(value) => {
            json(&value.to_json().to_string()).map_err(|why| ("unreadable".to_string(), why))
        }
        Err(err) => Err((err.code.clone(), err.to_string())),
    }
}

#[test]
fn shared_fixtures() {
    run(through_css);
}

#[test]
fn shared_fixtures_through_the_plugin() {
    run(through_plugin);
}

#[test]
fn shared_fixtures_on_a_bare_engine() {
    run(through_bare_engine);
}

fn run(parse: fn(&str, Options) -> Outcome) {
    let rows = read_spec_dir(&spec_dir());
    let mut failures = Vec::new();
    for row in &rows {
        if let Err(why) = check(row, parse) {
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

fn check(row: &Row, parse: fn(&str, Options) -> Outcome) -> Result<(), String> {
    // A fresh parser per row: the `opts` column is per-case, and plugin
    // options must not leak from one row into the next.
    let expected = row.expected.trim();

    match parse(&row.input, options(&row.opts)?) {
        Err((code, report)) => {
            if !expected.starts_with("ERROR") {
                return Err(format!(
                    "  input    {:?}\n  expected {expected}\n  got      ERROR:{code} ({report})",
                    row.input
                ));
            }
            // `ERROR` alone asserts only that the document is rejected;
            // `ERROR:<code>` compares the code EXACTLY — it is the error's
            // code, not a substring of its message.
            match expected.strip_prefix("ERROR:") {
                Some(want) if want != code => Err(format!(
                    "  input    {:?}\n  expected ERROR:{want}\n  got      ERROR:{code}",
                    row.input
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
    let flag = |key: &str| matches!(node.get(key), Some(Value::Bool(true)));
    Ok(Options {
        lowercase_properties: flag("lowercaseProperties"),
        position: flag("position"),
    })
}
