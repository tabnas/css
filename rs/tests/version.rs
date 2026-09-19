/* Copyright (c) 2026 Richard Rodger, MIT License */

//! The baked-in `VERSION` must equal this crate's declared version AND the
//! version `ts/package.json` declares.
//!
//! This is the CI check for version drift, and it mirrors
//! `go/version_test.go` and `ts/test/version.test.ts`. It exists because the
//! constant HAS drifted in practice: a package shipped two releases while its
//! constant sat at the version before them, because nothing ever rewrote it.
//! A release that bumps one site and forgets another now fails here instead
//! of shipping a lie.

mod support;

use support::{json, repo_root};

#[test]
fn version_matches_cargo_toml() {
    assert_eq!(
        tabnas_css::VERSION,
        env!("CARGO_PKG_VERSION"),
        "VERSION drift: VERSION = {:?} but rs/Cargo.toml declares {:?}",
        tabnas_css::VERSION,
        env!("CARGO_PKG_VERSION")
    );
}

#[test]
fn version_matches_package_json() {
    let path = repo_root().join("ts").join("package.json");
    // Deliberately fatal, never skipped: a version check that silently does
    // not run is the failure mode this test exists to prevent.
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "cannot read {}, so VERSION cannot be checked: {e}",
            path.display()
        )
    });
    let pkg = json(&raw).expect("ts/package.json is not readable JSON");
    let pkg = pkg.as_node().expect("ts/package.json is not an object");
    let declared = pkg
        .get("version")
        .and_then(tabnas_css::Value::as_str)
        .expect("ts/package.json has no version field");
    let name = pkg
        .get("name")
        .and_then(tabnas_css::Value::as_str)
        .unwrap_or("@tabnas/css");

    assert_eq!(
        tabnas_css::VERSION,
        declared,
        "VERSION drift: rust VERSION = {:?} but {name} package.json = {declared:?}.\n\
         All of them are rewritten by admin/publish.sh at release; if you bumped \
         one by hand, bump the others.",
        tabnas_css::VERSION
    );
}
