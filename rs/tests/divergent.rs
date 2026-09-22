/* Copyright (c) 2025 Richard Rodger and other contributors, MIT License */

//! The divergence register, executed in the Rust port.
//!
//! `test/divergent.tsv` holds one row per input where this repo's three
//! ports DISAGREE, with a cell per runtime. This file reads the `rust`
//! column; `ts/test/divergent.test.ts` and `go/divergent_test.go` read the
//! `ts` and `go` columns of the same file through `@tabnas/support`, whose
//! two halves cannot drift from each other. There is no Rust half of that
//! package in this crate's dependency set (it has none, deliberately), so
//! the three checks it applies are written out here.
//!
//! WHY THIS IS NOT A FIXTURE. A fixture fails when behaviour REGRESSES.
//! The register fails BOTH ways: a port repaired to agree with the others
//! still faces a register claiming they differ, so the suite goes red and
//! names the row to delete. A divergence recorded as a passing test of
//! current behaviour survives its own repair with nothing red. That is why
//! the file sits beside `test/spec/` rather than in it, where all three
//! parity runners would run every row of it.
//!
//! Every row records a divergence of the GO port from the canonical
//! TypeScript one, which is why the `ts` and `rust` cells agree: AGENTS.md
//! makes TypeScript canonical, and this port reproduces the canonical
//! behaviour, overshoot and all. If a row's `rust` cell ever has to differ
//! from `ts`, that is a defect in this port and not a licence to record it.

mod support;

use tabnas_css::{Css, Options, Value};

use support::{canonical, json, repo_root, Row};

/// Every runtime column in the file, this one included. Named rather than
/// inferred from the header, because inferring would silently treat `opts`
/// or `why` as a runtime and then report that the ports "agree" with a
/// sentence.
const RUNTIMES: [&str; 3] = ["ts", "go", "rust"];

/// The column this runtime answers for.
const MINE: &str = "rust";

/// The rows the register is measured against. An empty register is a
/// legitimate state (a repo with no known divergence) but an empty FILE is
/// not: it cannot be told apart from a loader that read nothing. Ratcheted
/// at what AGENTS.md records, so a row that quietly disappears fails here
/// rather than reducing the coverage in silence.
const ROW_COUNT: usize = 4;

fn register_rows() -> Vec<Row> {
    let path = repo_root().join("test").join("divergent.tsv");
    let rows = support::read_spec_file(&path);
    assert_eq!(
        ROW_COUNT,
        rows.len(),
        "{} holds {} rows, not the {ROW_COUNT} recorded in AGENTS.md under \
         \"Known cross-runtime divergence\". Update both together.",
        path.display(),
        rows.len()
    );
    rows
}

/// What a cell MEANS, so two cells can be compared by value rather than by
/// bytes: `ERROR:<code>` stays itself, and a JSON value is canonicalised
/// (keys sorted), exactly as the parity runner compares.
fn meaning(cell: &str) -> Result<String, String> {
    let cell = cell.trim();
    if let Some(code) = cell.strip_prefix("ERROR") {
        return Ok(format!("ERROR:{}", code.strip_prefix(':').unwrap_or("")));
    }
    Ok(canonical(&json(cell)?))
}

/// What this runtime produces for a row, in the same vocabulary.
fn produced(row: &Row) -> String {
    let css = Css::with_options(options(&row.opts));
    match css.parse(&row.input) {
        Ok(value) => canonical(&value),
        Err(err) => format!("ERROR:{}", err.code),
    }
}

fn options(opts: &str) -> Options {
    if opts.trim().is_empty() {
        return Options::default();
    }
    let parsed = json(opts).unwrap_or_else(|e| panic!("unreadable opts {opts:?}: {e}"));
    let node = parsed
        .as_node()
        .unwrap_or_else(|| panic!("opts is not an object: {opts:?}"));
    let flag = |key: &str| matches!(node.get(key), Some(Value::Bool(true)));
    Options {
        lowercase_properties: flag("lowercaseProperties"),
        position: flag("position"),
    }
}

#[test]
fn divergence_register() {
    let mut failures = Vec::new();

    for row in register_rows() {
        // Each runtime column must exist. A typo in RUNTIMES would
        // otherwise read as an empty cell and make every row look like
        // agreement, which is the opposite conclusion.
        let mut cells = Vec::new();
        for name in RUNTIMES {
            match row.named(name) {
                None => failures.push(format!(
                    "{}: no column named {name:?} (RUNTIMES)",
                    row.label()
                )),
                Some(cell) => match meaning(cell) {
                    Ok(value) => cells.push((name, value)),
                    Err(why) => {
                        failures.push(format!("{}: unreadable {name} cell: {why}", row.label()))
                    }
                },
            }
        }
        if cells.len() != RUNTIMES.len() {
            continue;
        }

        // 1. The row must record a divergence at all. A row whose cells all
        //    mean the same thing asserts nothing and would pass forever,
        //    which is the shape of the prose claims this file replaces.
        if cells.iter().all(|(_, value)| *value == cells[0].1) {
            failures.push(format!(
                "{}: every runtime column means {}, so this row records no \
                 divergence and can never fail meaningfully. Delete it, or \
                 correct the cells to what the runtimes actually do.",
                row.label(),
                cells[0].1
            ));
            continue;
        }

        // 2. This runtime must still produce what the register says it does.
        let mine = cells
            .iter()
            .find(|(name, _)| *name == MINE)
            .map(|(_, value)| value.clone())
            .expect("RUNTIMES contains MINE");
        let got = produced(&row);
        if got == mine {
            continue;
        }

        // 3. When it does not, and it now produces what ANOTHER runtime's
        //    cell says, the divergence is CLOSED and the row must go —
        //    reporting that as a regression would be the wrong conclusion.
        match cells
            .iter()
            .find(|(name, value)| *name != MINE && *value == got)
        {
            Some((name, _)) => failures.push(format!(
                "{}: input {:?}\n  DIVERGENCE CLOSED: {MINE} now agrees with {name}.\n  \
                 both produce {got}\n  Delete this row — a register that outlives its \
                 own repair is the failure this file exists to prevent.",
                row.label(),
                row.input
            )),
            None => failures.push(format!(
                "{}: input {:?}\n  register says {MINE} produces {mine}\n  \
                 it produced           {got}",
                row.label(),
                row.input
            )),
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
