/* Copyright (c) 2025 Richard Rodger and other contributors, MIT License */

//! The divergence register, executed in the Rust port.
//!
//! `test/divergent.tsv` holds one row per input where this repo's three
//! ports DISAGREE, with a cell per runtime. This file reads the `rust`
//! column; `ts/test/divergent.test.ts` and `go/divergent_test.go` read the
//! `ts` and `go` columns of the same file through `@tabnas/support`, whose
//! two halves cannot drift from each other. This crate's tests keep their
//! own small loader (`tests/support`) rather than take that package's Rust
//! half, so the three checks it applies are written out here.
//!
//! WHY THIS IS NOT A FIXTURE. A fixture fails when behaviour REGRESSES.
//! The register fails BOTH ways: a port repaired to agree with the others
//! still faces a register claiming they differ, so the suite goes red and
//! names the row to delete. A divergence recorded as a passing test of
//! current behaviour survives its own repair with nothing red. That is why
//! the file sits beside `test/spec/` rather than in it, where all three
//! parity runners would run every row of it.
//!
//! The `rust` cell is what the engine's own API returns for the row: the
//! tree form of [`make_with`]. It agrees with `ts` on every row that records
//! a divergence of the GO port, because this port reproduces the canonical
//! behaviour, overshoot and all, and differs on one: the tree form is
//! bounded in depth ([`tabnas_css::TREE_RULE_DEPTH`]) where the canonical
//! port is not. [`Css::parse`] has no such bound and is checked against the
//! `ts` cell on EVERY row; if it ever has to differ from `ts`, that is a
//! defect in this port and not a licence to record it.

mod support;

use tabnas_css::{make_with, Css, Options, Value};

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
const ROW_COUNT: usize = 5;

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

/// What this runtime produces for a row through the engine's own API, in
/// the same vocabulary: the `rust` column.
fn produced(row: &Row) -> String {
    match make_with(options(&row.opts)).parse(&row.input) {
        Ok(tree) => match json(&tree.to_json().to_string()) {
            Ok(value) => canonical(&value),
            Err(why) => format!("unreadable engine JSON: {why}"),
        },
        Err(err) => format!("ERROR:{}", err.code),
    }
}

/// What [`Css::parse`] produces for a row, which must be the `ts` cell.
fn produced_by_css(row: &Row) -> String {
    match Css::with_options(options(&row.opts)).parse(&row.input) {
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

        // The crate's own entry point is the canonical behaviour on every
        // row, whatever the engine's API does.
        let canon = &cells
            .iter()
            .find(|(name, _)| "ts" == *name)
            .expect("RUNTIMES contains ts")
            .1;
        let by_css = produced_by_css(&row);
        if by_css != *canon {
            failures.push(format!(
                "{}: input {:?}\n  Css::parse produced {by_css}\n  the ts cell says   {canon}",
                row.label(),
                row.input
            ));
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

        // 3. When it does not, the row has MOVED, and there are two ways
        //    it can have moved. It is CLOSED only when this runtime now
        //    agrees with EVERY other cell.
        //
        //    Agreeing with ONE of several is not a repair. With three
        //    runtimes registered, a row that moves from its own value to
        //    the value in the `go` cell has this runtime crossing from one
        //    side of a three-way split to another, with the canonical
        //    runtime still alone on the far side -- which is a regression,
        //    and reporting it as closed would tell the reader to delete the
        //    row that had just caught it.
        let others: Vec<&(&str, String)> = cells.iter().filter(|cell| cell.0 != MINE).collect();
        let agree: Vec<&str> = others
            .iter()
            .filter(|cell| cell.1 == got)
            .map(|cell| cell.0)
            .collect();
        if agree.len() == others.len() {
            failures.push(format!(
                "{}: input {:?}\n  DIVERGENCE CLOSED: {MINE} now agrees with every other \
                 runtime ({}).\n  all produce {got}\n  Delete this row — a register that \
                 outlives its own repair is the failure this file exists to prevent.",
                row.label(),
                row.input,
                agree.join(", "),
            ));
        } else {
            let differ: Vec<String> = others
                .iter()
                .filter(|cell| cell.1 != got)
                .map(|cell| format!("{} {}", cell.0, cell.1))
                .collect();
            let crossed = if agree.is_empty() {
                String::new()
            } else {
                format!(
                    "\n  it now matches {}, which is NOT this divergence closing: \
                     the row still records a split",
                    agree.join(", "),
                )
            };
            failures.push(format!(
                "{}: input {:?}\n  register says {MINE} produces {mine}\n  \
                 it produced           {got}{crossed}\n  still differing: {}",
                row.label(),
                row.input,
                differ.join("; "),
            ));
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
