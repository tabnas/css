/* Copyright (c) 2025 Richard Rodger and other contributors, MIT License */

//! A guard against the regression where the convenience [`tabnas_css::parse`]
//! rebuilds the grammar on every call instead of reusing a cached parser.
//!
//! Mirrors `go/perf_test.go`. The check is machine-INDEPENDENT: it compares
//! the cached entry point against explicit instance reuse on the SAME machine
//! in the SAME run, so a slow CI box cannot make it flaky — both sides scale
//! together. There is deliberately NO wall-clock budget.

use std::time::Instant;

use tabnas_css::Css;

#[test]
fn parse_reuses_one_parser() {
    const SRC: &str = "a { color: red; font: 12px sans-serif } .b > .c { margin: 0 }";
    const N: usize = 3000;

    // Warm both paths so the comparison is steady-state.
    for _ in 0..100 {
        tabnas_css::parse(SRC).expect("warm-up parse failed");
    }
    let css = Css::new();
    for _ in 0..100 {
        css.parse(SRC).expect("warm-up reuse parse failed");
    }

    let t0 = Instant::now();
    for _ in 0..N {
        tabnas_css::parse(SRC).expect("parse failed");
    }
    let convenience = t0.elapsed();

    let t1 = Instant::now();
    for _ in 0..N {
        css.parse(SRC).expect("reuse parse failed");
    }
    let reuse = t1.elapsed();

    let ratio = convenience.as_secs_f64() / reuse.as_secs_f64().max(f64::MIN_POSITIVE);
    assert!(
        convenience <= 4 * reuse,
        "parse() appears to build a grammar on every call: {N} parse() calls took \
         {convenience:?} vs {reuse:?} reusing one Css (ratio {ratio:.1}x, limit 4x). \
         Cache a lazy default instance (see parse / OnceLock)."
    );
}

/// Building a parser is the expensive part, so it had better be the part that
/// reuse skips. A [`Css`] that is cheap to build would make the guard above
/// pass for the wrong reason.
#[test]
fn building_a_parser_costs_more_than_parsing_with_one() {
    const SRC: &str = "a { color: red }";
    const N: usize = 200;

    let css = Css::new();
    for _ in 0..50 {
        css.parse(SRC).expect("warm-up parse failed");
    }

    let t0 = Instant::now();
    for _ in 0..N {
        std::hint::black_box(Css::new());
    }
    let build = t0.elapsed();

    let t1 = Instant::now();
    for _ in 0..N {
        std::hint::black_box(css.parse(SRC).expect("parse failed"));
    }
    let parse = t1.elapsed();

    assert!(
        parse < build,
        "building a Css ({build:?} for {N}) is no longer more expensive than \
         parsing with one ({parse:?}), so the reuse guard above no longer \
         measures what it claims to"
    );
}
