/* Copyright (c) 2025 Richard Rodger and other contributors, MIT License */

//! Every repetition in the grammar is a replace loop, never a push chain.
//!
//! An alternate that hands control to another rule either pushes it (`p`),
//! opening a frame for something the tree nests, or replaces the current
//! rule with it (`r`), re-entering in the same frame for the next item of a
//! sequence. Written as replace loops, the items of a list add no depth:
//! rule depth follows a stylesheet's nesting and never its length. The
//! engine's rule depth `d` is the observable, read here with a rule
//! subscriber: the maximum over ten thousand items of each repetition is
//! what one item needs, while real nesting still grows it.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The css grammar's own rules. The engine also lists jsonic's, whose
/// repetitions are that repository's to keep.
const RULES: [&str; 13] = [
    "stylesheet",
    "items",
    "statement",
    "sel",
    "declbody",
    "decls",
    "decl",
    "declval",
    "rulesbody",
    "kfbody",
    "kfitems",
    "keyframe",
    "kfsel",
];

/// The maximum rule depth `d` any rule reaches while `src` parses.
fn max_depth(src: &str) -> usize {
    let mut parser = tabnas_css::make();
    let max = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&max);
    parser.subscribe_rules(move |rule, _context| {
        seen.fetch_max(rule.d, Ordering::Relaxed);
    });
    if let Err(error) = parser.parse(src) {
        panic!("{:?} does not parse: {error}", &src[..src.len().min(40)]);
    }
    max.load(Ordering::Relaxed)
}

const ITEMS: usize = 10_000;

/// Each repetition, as one item and as ten thousand, with the depth one
/// item needs.
fn repetitions() -> Vec<(&'static str, String, String, usize)> {
    let n = ITEMS;
    vec![
        ("statements", "a{}".into(), "a{}".repeat(n), 4),
        ("comments", "/*x*/".into(), "/*x*/".repeat(n), 2),
        (
            "statement at-rules",
            "@import x;".into(),
            "@import x;".repeat(n),
            2,
        ),
        (
            "declarations",
            "a{b:c}".into(),
            format!("a{{{}}}", "b:c;".repeat(n)),
            7,
        ),
        (
            "selectors",
            "a{}".into(),
            format!("{}a{{}}", "a,".repeat(n)),
            4,
        ),
        (
            "keyframe blocks",
            "@keyframes k{from{}}".into(),
            format!("@keyframes k{{{}}}", "from{}".repeat(n)),
            7,
        ),
        (
            "keyframe values",
            "@keyframes k{from{}}".into(),
            format!("@keyframes k{{{}to{{}}}}", "0%,".repeat(n)),
            7,
        ),
        (
            "@media items",
            "@media x{a{}}".into(),
            format!("@media x{{{}}}", "a{}".repeat(n)),
            7,
        ),
    ]
}

#[test]
fn every_repetition_stays_at_the_depth_of_one_item() {
    for (name, one, many, depth) in repetitions() {
        assert_eq!(depth, max_depth(&one), "{name}: one item");
        assert_eq!(
            depth,
            max_depth(&many),
            "{name}: {ITEMS} items reach deeper than one"
        );
    }
}

/// Real nesting still nests: each style rule inside another takes four
/// frames (`decls`, `decl`, `sel`, `declbody`), each `@media` inside another
/// three (`items`, `statement`, `rulesbody`).
#[test]
fn nesting_still_grows_the_depth() {
    let rules = |levels: usize| format!("{}b:c{}", "a{".repeat(levels), "}".repeat(levels));
    let media = |levels: usize| format!("{}{}", "@media x{".repeat(levels), "}".repeat(levels));
    let (r1, r100) = (max_depth(&rules(1)), max_depth(&rules(100)));
    assert_eq!(4 * 99, r100 - r1, "100 nested rules: {r1} then {r100}");
    let (m1, m100) = (max_depth(&media(1)), max_depth(&media(100)));
    assert_eq!(3 * 99, m100 - m1, "100 nested @media: {m1} then {m100}");
}

/// The installed grammar says the same. The replaces are the five loops:
/// the statement, declaration and keyframe lists replacing themselves in
/// their close, and a selector group and a keyframe's value list replacing
/// themselves in their open. Every push is structure: a block's body, a
/// list's first item, or an item's parts.
#[test]
fn the_repetitions_are_replace_loops() {
    let parser = tabnas_css::make();
    let mut pushes = Vec::new();
    let mut replaces = Vec::new();
    for spec in parser.rule_specs() {
        if !RULES.contains(&spec.name.as_str()) {
            continue;
        }
        for (phase, alts) in [("open", &spec.open), ("close", &spec.close)] {
            for alt in alts {
                if let Some(target) = &alt.p {
                    pushes.push(format!("{} {phase} p:{target}", spec.name));
                }
                if let Some(target) = &alt.r {
                    replaces.push(format!("{} {phase} r:{target}", spec.name));
                }
            }
        }
    }
    pushes.sort();
    pushes.dedup();
    replaces.sort();
    replaces.dedup();
    assert_eq!(
        pushes,
        [
            "decl open p:declbody",
            "decl open p:declval",
            "decl open p:kfbody",
            "decl open p:rulesbody",
            "decl open p:sel",
            "declbody open p:decls",
            "decls open p:decl",
            "items open p:statement",
            "keyframe open p:kfsel",
            "kfbody open p:kfitems",
            "kfitems open p:keyframe",
            "kfsel open p:declbody",
            "rulesbody open p:items",
            "sel open p:declbody",
            "statement open p:declbody",
            "statement open p:kfbody",
            "statement open p:rulesbody",
            "statement open p:sel",
            "stylesheet open p:items",
        ]
    );
    assert_eq!(
        replaces,
        [
            "decls close r:decls",
            "items close r:items",
            "kfitems close r:kfitems",
            "kfsel open r:kfsel",
            "sel open r:sel",
        ]
    );
}

/// The fastest of a few parses of `src`, to keep a loaded machine's stalls
/// and a coarse clock out of the comparison.
fn fastest_parse(src: &str) -> Duration {
    let css = tabnas_css::Css::new();
    (0..3)
        .map(|_| {
            let start = Instant::now();
            css.parse(src).expect("the rules parse");
            start.elapsed()
        })
        .min()
        .expect("three runs")
}

/// Ten times the rules take about ten times as long. Quadratic work would
/// take a hundred; the bound is generous so a busy machine cannot fail it.
#[test]
fn parse_time_grows_linearly_with_the_rules() {
    let small = fastest_parse(&"a{b:c}\n".repeat(1_000));
    let large = fastest_parse(&"a{b:c}\n".repeat(10_000));
    let ratio = large.as_secs_f64() / small.as_secs_f64().max(1e-6);
    assert!(
        ratio < 30.0,
        "1,000 rules took {small:?} and 10,000 took {large:?}: {ratio:.1}x"
    );
}
