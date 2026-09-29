/* Copyright (c) 2025 Richard Rodger and other contributors, MIT License */

//! The crate as an engine plugin: what `plugin()`, `css()`, `make()` and
//! `Css::tabnas()` give a caller who uses the engine's own API, and the
//! guarantees that form has to keep.

mod support;

use tabnas::{ContextSeed, Tabnas, Value as Engine};
use tabnas_css::{css, make, make_with, plugin, Css, Error, Options, TREE_RULE_DEPTH};

use support::{canonical, json};

/// An engine value in this crate's value model, through its JSON.
fn from_tree(value: &Engine) -> tabnas_css::Value {
    json(&value.to_json().to_string()).expect("the engine writes JSON")
}

const POSITIONED: Options = Options {
    lowercase_properties: false,
    position: true,
};

#[test]
fn the_tree_form_equals_the_css_form_value_for_value() {
    for options in [Options::default(), POSITIONED] {
        let parser = make_with(options);
        let css = Css::with_options(options);
        for src in [
            "a { color: red; color: blue } /* note */",
            "@media screen { a, b { x: 1 } }\n@import 'x';",
            "@-webkit-keyframes spin { from { x: 0 } 50%, to { x: 1 } }",
            "@page toc, index:blank { margin: 1in }",
            "#\u{1D11E} a { b: c }",
        ] {
            let tree = parser.parse(src).expect("parses");
            let ast = css.parse(src).expect("parses");
            assert_eq!(
                canonical(&ast),
                canonical(&from_tree(&tree)),
                "{src:?} with {options:?}"
            );
        }
    }
}

#[test]
fn the_plugin_installs_on_a_bare_engine_as_on_jsonic() {
    let mut bare = Tabnas::new();
    bare.use_plugin(plugin(), None).expect("installs");
    let on_jsonic = make();
    let src = "a { color: red }";
    assert_eq!(
        bare.parse(src).expect("parses").to_json(),
        on_jsonic.parse(src).expect("parses").to_json()
    );
}

#[test]
fn options_are_read_for_their_javascript_truthiness() {
    let bag = |text: &str| serde_json_value(text);
    assert_eq!(Options::default(), Options::from_value(&bag("{}")));
    assert_eq!(Options::default(), Options::from_value(&Engine::Undefined));
    let on = Options::from_value(&bag(r#"{"position": 1, "lowercaseProperties": "yes"}"#));
    assert!(on.position && on.lowercase_properties);
    let off = Options::from_value(&bag(r#"{"position": 0, "lowercaseProperties": ""}"#));
    assert!(!off.position && !off.lowercase_properties);
    let round = Options {
        lowercase_properties: true,
        position: false,
    };
    assert_eq!(round, Options::from_value(&round.to_value()));
}

/// A JSON text as an engine value, through the engine's own JSON grammar.
fn serde_json_value(text: &str) -> Engine {
    tabnas_jsonic::make().parse(text).expect("JSON")
}

#[test]
fn a_second_use_applies_its_options_and_a_derived_instance_agrees() {
    // The canonical port re-reads its options on every `use`, so
    // `.use(Css).use(Css, {position: true})` has positions.
    let mut parser = make();
    css(
        &mut parser,
        &Options {
            lowercase_properties: true,
            position: true,
        },
    )
    .expect("installs again");
    let derived = parser.derive(|_| {}).expect("derives");
    for p in [&parser, &derived] {
        let got = p.parse("A{B:c}").expect("parses").to_json().to_string();
        assert!(got.contains(r#""property":"b""#), "{got}");
        assert!(got.contains(r#""position""#), "{got}");
    }
    // The subscriber is not doubled by the second install or the
    // derivation: the end-of-input overshoot is added once.
    for p in [&parser, &derived] {
        let tree = from_tree(&p.parse("@host\\").expect("parses"));
        assert!(
            tree.to_json().ends_with(
                r#""position":{"start":{"line":1,"column":1},"end":{"line":1,"column":8}}}"#
            ),
            "{}",
            tree.to_json()
        );
    }
}

#[test]
fn a_direct_install_is_kept_by_a_derived_instance() {
    let mut parser = Tabnas::new();
    css(&mut parser, &POSITIONED).expect("installs");
    let derived = parser.derive(|_| {}).expect("derives");
    assert_eq!(
        parser.parse("a{b:c}").expect("parses").to_json(),
        derived.parse("a{b:c}").expect("parses").to_json()
    );
}

#[test]
fn an_empty_source_is_an_empty_stylesheet_without_a_position() {
    let parser = make_with(POSITIONED);
    let empty = parser.parse("").expect("parses").to_json().to_string();
    assert_eq!(r#"{"rules":[],"type":"stylesheet"}"#, sorted(&empty));
    let space = parser.parse(" ").expect("parses").to_json().to_string();
    assert!(space.contains("position"), "{space}");
    let css = Css::with_options(POSITIONED);
    assert_eq!(
        r#"{"type":"stylesheet","rules":[]}"#,
        css.parse("").expect("parses").to_json()
    );
    assert!(css
        .parse(" ")
        .expect("parses")
        .to_json()
        .contains("position"));
}

/// `{"b":…,"a":…}` with its top-level keys sorted, for a comparison that
/// does not depend on key order.
fn sorted(text: &str) -> String {
    canonical(&json(text).expect("JSON"))
}

/// `depth` style rules nested one in the next.
fn nested_rules(depth: usize) -> String {
    format!("{}b:c{}", "a{".repeat(depth), "}".repeat(depth))
}

/// `depth` `@media` blocks nested one in the next, the innermost empty.
fn nested_media(depth: usize) -> String {
    format!("{}{}", "@media x{".repeat(depth), "}".repeat(depth))
}

#[test]
fn the_tree_form_is_bounded_and_the_css_form_is_not() {
    let parser = make();
    let css = Css::new();
    for (src, deepest) in [
        (nested_rules as fn(usize) -> String, 191),
        (nested_media, 256),
    ] {
        parser
            .parse(&src(deepest))
            .unwrap_or_else(|e| panic!("{deepest} levels: {e}"));
        let refused = parser
            .parse(&src(deepest + 1))
            .expect_err("one level past the bound");
        assert_eq!("cancel", refused.code);
        css.parse(&src(deepest + 1))
            .expect("the css form has no bound");
    }
    assert_eq!(768, TREE_RULE_DEPTH);
}

#[test]
fn a_tree_at_the_bound_is_safe_on_a_small_stack() {
    // A spawned thread's default stack, and a debug build: the engine's
    // value drops, clones, compares and prints by recursion, and the bound is
    // what keeps that inside the stack.
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(|| {
            let parser = make_with(POSITIONED);
            for src in [nested_rules(191), nested_media(256)] {
                let tree = parser.parse(&src).expect("parses");
                let copy = tree.clone();
                assert!(tree == copy);
                assert!(!format!("{tree:?}").is_empty());
                assert!(!format!("{tree}").is_empty());
                assert!(!tree.to_json().to_string().is_empty());
                drop(copy);
                drop(tree);
            }
        })
        .expect("spawns")
        .join()
        .expect("no stack overflow");
}

#[test]
fn recovery_returns_a_partial_stylesheet() {
    let parser = make()
        .derive(|options| options.parse.recover.enabled = true)
        .expect("derives");
    for got in [
        parser.parse("a{b:c} d{").expect("recovers"),
        parser
            .parse_recover("a{b:c} d{")
            .value
            .expect("a partial value"),
    ] {
        let tree = from_tree(&got);
        let node = tree.as_node().expect("an object, not a bare number");
        assert_eq!(Some("stylesheet"), node.node_type());
        assert!(
            tree.to_json().contains(r#""property":"b","value":"c""#),
            "{}",
            tree.to_json()
        );
    }
}

#[test]
fn a_seeded_context_does_not_reach_the_plugins_state() {
    let mut seed = ContextSeed::default();
    seed.u
        .insert("tabnas-css/overshoot".into(), Engine::Number(5.0));
    seed.u.insert(
        "tabnas-css/nodes".into(),
        Engine::array(vec![Engine::Number(1.0), Engine::Null]),
    );
    seed.u.insert(
        "tabnas-css/astral".into(),
        Engine::array(vec![Engine::Number(0.0)]),
    );
    let parser = make_with(POSITIONED);
    for src in ["@host x", "a{b:c}"] {
        let clean = parser.parse(src).expect("parses");
        let seeded = parser
            .parse_with_context(src, Engine::Undefined, &seed)
            .expect("parses");
        assert_eq!(clean.to_json(), seeded.to_json(), "{src:?}");
    }
}

#[test]
fn engine_errors_count_scalars_and_the_crate_error_utf16() {
    // The engine's column counts Unicode scalars, as all its errors do; the
    // crate's counts UTF-16 code units, as the canonical port's does.
    // `Error::from` is the conversion.
    for (src, scalar, utf16) in [
        ("#\u{1D11E}\u{1D11E} a{!}", 7, 9),
        ("a{b:\u{1D11E}\\", 8, 9),
    ] {
        let engine = make().parse(src).expect_err("fails");
        assert_eq!(scalar, engine.col, "{src:?}");
        assert_eq!(utf16, Error::from(engine).column, "{src:?}");
        assert_eq!(utf16, Css::new().parse(src).expect_err("fails").column);
    }
}

#[test]
fn the_engine_writes_whole_numbers_as_floats() {
    // Byte-identical canonical JSON comes from `Css::parse(..).to_json()`;
    // the engine's writer prints a line number as `1.0`.
    let tree = make_with(POSITIONED).parse("a{}").expect("parses");
    assert!(tree.to_json().to_string().contains(r#""line":1.0"#));
    let ast = Css::with_options(POSITIONED).parse("a{}").expect("parses");
    assert!(ast.to_json().contains(r#""line":1,"#));
}
