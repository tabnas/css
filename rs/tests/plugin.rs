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
    // The rules are installed afresh, not on top of the first install's:
    // installing over a rule puts the new alternates in front of the old,
    // which parse the same and are tried twice.
    let fresh = alternate_counts(&make());
    assert_eq!(13, fresh.len());
    for p in [&parser, &derived] {
        assert_eq!(fresh, alternate_counts(p));
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

/// Each css rule's name and its open and close alternate counts.
fn alternate_counts(parser: &Tabnas) -> Vec<(String, usize, usize)> {
    let css = [
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
    let mut counts: Vec<(String, usize, usize)> = parser
        .rule_specs()
        .into_iter()
        .filter(|spec| css.contains(&spec.name.as_str()))
        .map(|spec| (spec.name.to_string(), spec.open.len(), spec.close.len()))
        .collect();
    counts.sort();
    counts
}

#[test]
fn a_rule_of_the_same_name_from_another_grammar_is_not_this_install() {
    // An engine that already has a `stylesheet` rule of its own gets the
    // whole plugin, lex subscriber included: the end-of-input overshoot and
    // the bad-token lookahead both depend on it.
    let mut parser = Tabnas::new();
    parser
        .grammar_json(r##"{"rule":{"stylesheet":{"open":[{"s":"#ZZ"}]}}}"##)
        .expect("a foreign stylesheet rule installs");
    css(&mut parser, &POSITIONED).expect("installs");
    let tree = from_tree(&parser.parse("@host\\").expect("parses"));
    assert!(
        tree.to_json().ends_with(
            r#""position":{"start":{"line":1,"column":1},"end":{"line":1,"column":8}}}"#
        ),
        "{}",
        tree.to_json()
    );
    let err = parser.parse("a{b\\\"x;\"/*").expect_err("unclosed comment");
    assert_eq!("unterminated_comment", err.code);
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
fn a_merged_instance_parses_as_the_css_instance_does() {
    // `Tabnas::merge` numbers every custom token afresh without running the
    // plugin again, so the matcher must not emit numbers it captured at
    // install: `#CC` and the at-rule tokens are emitted by name.
    let css = make()
        .derive(|options| options.tag = "css".into())
        .expect("derives");
    let json = tabnas_jsonic::make()
        .derive(|options| options.tag = "json".into())
        .expect("derives");
    for merged in [
        css.merge(&json).expect("merges"),
        json.merge(&css).expect("merges"),
    ] {
        for src in [
            "@media x{a{b:c}}",
            "@import 'x';",
            "@host\\",
            "/*x*/",
            "@font-face{a:b}",
            "@keyframes k{from{a:b}}",
            "a,b{c:d}",
        ] {
            assert_eq!(
                css.parse(src).expect("parses").to_json(),
                merged.parse(src).expect("parses").to_json(),
                "{src:?}"
            );
        }
    }
}

#[test]
fn using_the_plugin_again_on_a_merged_instance_is_a_second_install() {
    // A merge renames `@cssSheet` to `@css:cssSheet`; the install must still
    // see its own grammar there, or it adds a second lex subscriber (the
    // end-of-input overshoot counted twice) and installs the rules on top
    // of the merged ones.
    let css = make()
        .derive(|options| options.tag = "css".into())
        .expect("derives");
    let json = tabnas_jsonic::make()
        .derive(|options| options.tag = "json".into())
        .expect("derives");
    let fresh = alternate_counts(&make());
    for mut merged in [
        css.merge(&json).expect("merges"),
        json.merge(&css).expect("merges"),
    ] {
        merged
            .use_plugin(plugin(), Some(POSITIONED.to_value()))
            .expect("installs again");
        let tree = from_tree(&merged.parse("@host\\").expect("parses"));
        assert!(
            tree.to_json().ends_with(
                r#""position":{"start":{"line":1,"column":1},"end":{"line":1,"column":8}}}"#
            ),
            "{}",
            tree.to_json()
        );
        assert_eq!(fresh, alternate_counts(&merged));
    }
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
fn a_later_plugins_depth_guard_leaves_the_bound_in_place() {
    // jsonic installs its guard as `depth`, the name the grammars layered on
    // it use to replace one another's; the tree form's bound has a name of
    // its own, so jsonic used after css does not remove it.
    let mut parser = Tabnas::new();
    css(&mut parser, &Options::default()).expect("installs");
    parser
        .use_plugin(tabnas_jsonic::plugin(), None)
        .expect("installs");
    parser
        .parse(&nested_rules(191))
        .expect("parses at the bound");
    let refused = parser
        .parse(&nested_rules(192))
        .expect_err("one level past the bound");
    assert_eq!("cancel", refused.code);
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
    let rule_a = r#"{"type":"rule","selectors":["a"],"declarations":[{"type":"declaration","property":"b","value":"c"}]}"#;
    // The value and the first error are the canonical port's. Where a
    // statement, declaration or keyframe fails before its constructor runs,
    // the canonical pusher pushes the enclosing node into itself, a cycle,
    // and these are its values with that one entry left out: a value here
    // cannot hold a cycle, and a copy would put a stylesheet inside its own
    // `rules` (the last five rows).
    for (src, value, first) in [
        (
            "a{b:c} d{",
            format!(r#"{{"type":"stylesheet","rules":[{rule_a}]}}"#),
            ("unexpected", 1, 10),
        ),
        (
            "/*",
            r#"{"type":"stylesheet","rules":[]}"#.to_string(),
            ("unterminated_comment", 1, 1),
        ),
        (
            "a{b:c} /*x",
            format!(r#"{{"type":"stylesheet","rules":[{rule_a}]}}"#),
            ("unterminated_comment", 1, 8),
        ),
        (
            "{",
            r#"{"type":"stylesheet","rules":[]}"#.to_string(),
            ("unexpected", 1, 1),
        ),
        (
            "a{b:c;;d:e}",
            format!(r#"{{"type":"stylesheet","rules":[{rule_a}]}}"#),
            ("unexpected", 1, 7),
        ),
        (
            "@media x{ , } a{b:c}",
            format!(
                r#"{{"type":"stylesheet","rules":[{{"type":"media","media":"x","rules":[]}},{rule_a}]}}"#
            ),
            ("unexpected", 1, 11),
        ),
        (
            "@keyframes k{ , }",
            r#"{"type":"stylesheet","rules":[{"type":"keyframes","name":"k","keyframes":[]}]}"#
                .to_string(),
            ("unexpected", 1, 15),
        ),
        (
            "@font-face{ , }",
            r#"{"type":"stylesheet","rules":[{"type":"font-face","declarations":[]}]}"#.to_string(),
            ("unexpected", 1, 13),
        ),
        // A stray opener, then a selector the matcher classifies with the
        // group scan: the scan from `(` never comes back to depth 0, so a
        // later start must not take its answer.
        (
            "(\"x\"a{b:c}",
            r#"{"type":"stylesheet","rules":[{"type":"rule","selectors":["\"x\"a"],"declarations":[{"type":"declaration","property":"b","value":"c"}]}]}"#.to_string(),
            ("unexpected", 1, 1),
        ),
        (
            "(a,b{c:d}",
            r#"{"type":"stylesheet","rules":[{"type":"rule","selectors":["a","b"],"declarations":[{"type":"declaration","property":"c","value":"d"}]}]}"#.to_string(),
            ("unexpected", 1, 1),
        ),
        (
            "[x,y{c:d}",
            r#"{"type":"stylesheet","rules":[{"type":"rule","selectors":["x","y"],"declarations":[{"type":"declaration","property":"c","value":"d"}]}]}"#.to_string(),
            ("unexpected", 1, 1),
        ),
    ] {
        let got = parser.parse_recover(src);
        let tree = from_tree(&got.value.expect("a partial value"));
        assert_eq!(value, tree.to_json(), "{src:?}");
        let error = Error::from(got.errors.into_iter().next().expect("an error"));
        assert_eq!(
            first,
            (error.code.as_str(), error.line, error.column),
            "{src:?}"
        );
    }
    // An unclosed comment behind a good token, under recovery: the engine
    // records the bad token as it is fetched and skips it, as the canonical
    // engine does (tabnas/parser#274), so the first error is the canonical
    // port's, `unterminated_comment` at 1:5. Before #274 the Rust engine
    // buffered the token and the first error was `unexpected` at the
    // property.
    let got = parser.parse_recover("a{b\\\"x;\"/*");
    let error = Error::from(got.errors.into_iter().next().expect("an error"));
    assert_eq!(
        ("unterminated_comment", 1, 5),
        (error.code.as_str(), error.line, error.column)
    );
    // `parse` with recovery on returns the same partial value.
    let tree = from_tree(&parser.parse("a{b:c} d{").expect("recovers"));
    assert_eq!(
        Some("stylesheet"),
        tree.as_node().expect("a node").node_type()
    );
}

#[test]
fn relexing_leaves_the_lookahead_alone() {
    // With relexing on, the canonical engine reports the good token ahead
    // of the unclosed comment, and so does this one. Without relexing the
    // comment is the error (`comments.tsv`).
    let parser = make()
        .derive(|options| options.lex.relex = true)
        .expect("derives");
    let error = Error::from(parser.parse("a{b\\\"x;\"/*").expect_err("fails"));
    assert_eq!(
        ("unexpected", 1, 3),
        (error.code.as_str(), error.line, error.column)
    );
}

#[test]
fn a_callers_meta_cannot_switch_the_store() {
    // `Css::parse` asks for the arena with a meta object recognised by its
    // address; the same key and value from a caller is only meta.
    let mut forged = Engine::object(Default::default());
    if let Some(fields) = forged.as_object_mut() {
        fields.insert("tabnas-css/arena".into(), Engine::Bool(true));
    }
    let parser = make_with(POSITIONED);
    assert_eq!(
        parser.parse("a{b:c}").expect("parses").to_json(),
        parser
            .parse_with_meta("a{b:c}", forged.clone())
            .expect("parses")
            .to_json()
    );
    let refused = make()
        .parse_with_meta(&nested_rules(192), forged)
        .expect_err("the bound still applies");
    assert_eq!("cancel", refused.code);
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
    // A group scan that claims a `;` at 1000 was found first from 0, with
    // its cursor at 0: the selector `a b` would read as a property, `a`.
    seed.u.insert(
        "tabnas-css/brace-scan".into(),
        Engine::array([0.0, 1.0, 1000.0, 0.0, 0.0].map(Engine::Number).to_vec()),
    );
    let parser = make_with(POSITIONED);
    for src in ["@host x", "a{b:c}", "a b{c:d}"] {
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
