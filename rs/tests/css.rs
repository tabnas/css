/* Copyright (c) 2025 Richard Rodger and other contributors, MIT License */

//! In-language AST cases, and the crate's own API surface.
//!
//! The parse cases that can be written as input → output belong in
//! `test/spec/*.tsv`, where all three runtimes run them — see
//! `test/AGENTS.md`. What is here is what a shared fixture cannot express:
//! a handful of representative shapes so this file reads as a suite, plus the
//! things that are true of THIS port and not of the others — key order in the
//! emitted JSON, an absent `position.end` (not a null one), the error's line
//! and column, the depth a stack-based machine can take, and that the cached
//! entry point is safe to share across threads.

mod support;

use tabnas_css::{Css, Options, Value};

use support::canonical;

fn ast(src: &str) -> Value {
    Css::new()
        .parse(src)
        .unwrap_or_else(|e| panic!("parse({src:?}) failed: {e}"))
}

fn json(src: &str) -> String {
    ast(src).to_json()
}

// --- Representative AST shapes --------------------------------------------

#[test]
fn empty_and_whitespace_only_sources_are_empty_stylesheets() {
    let want = r#"{"type":"stylesheet","rules":[]}"#;
    assert_eq!(want, json(""));
    assert_eq!(want, json("   \n  "));
}

#[test]
fn declaration_order_and_duplicates_are_preserved() {
    assert_eq!(
        r#"{"type":"stylesheet","rules":[{"type":"rule","selectors":["a"],"declarations":[{"type":"declaration","property":"color","value":"red"},{"type":"declaration","property":"color","value":"blue"}]}]}"#,
        json("a { color: red; color: blue }")
    );
}

#[test]
fn a_selector_group_becomes_a_list_but_a_comma_inside_not_does_not() {
    assert_eq!(
        r#"{"type":"stylesheet","rules":[{"type":"rule","selectors":["a:not(.x, .y)","b"],"declarations":[{"type":"declaration","property":"top","value":"0"}]}]}"#,
        json("a:not(.x, .y), b { top: 0 }")
    );
}

#[test]
fn comments_are_nodes_at_list_positions_and_dropped_mid_construct() {
    assert_eq!(
        r#"{"type":"stylesheet","rules":[{"type":"comment","comment":" only "}]}"#,
        json("/* only */")
    );
    // Between a property and its `:` a comment is read under a rule that
    // builds an item, not one that reads a list, so it is skipped.
    assert_eq!(
        r#"{"type":"stylesheet","rules":[{"type":"rule","selectors":["a"],"declarations":[{"type":"declaration","property":"color","value":"red"}]}]}"#,
        json("a /* x */ { color /* y */ : red }")
    );
}

#[test]
fn at_rules_carry_their_prelude_comments_but_selectors_do_not() {
    assert!(json("@media screen /*x*/ { a { b: c } }").contains(r#""media":"screen /*x*/""#));
    assert!(json("a /*x*/ { b: c }").contains(r#""selectors":["a"]"#));
}

#[test]
fn nesting_lands_in_the_parent_rules_declarations_in_source_order() {
    assert_eq!(
        r#"{"type":"stylesheet","rules":[{"type":"rule","selectors":["a"],"declarations":[{"type":"declaration","property":"color","value":"red"},{"type":"rule","selectors":["& b"],"declarations":[{"type":"declaration","property":"top","value":"0"}]}]}]}"#,
        json("a { color: red; & b { top: 0 } }")
    );
}

#[test]
fn an_unclosed_comment_is_an_error_not_a_comment_to_end_of_input() {
    let err = Css::new().parse("/*").expect_err("'/*' must not parse");
    assert_eq!("unterminated_comment", err.code);
    // The at-rule prelude path reaches the same verdict, which it did not
    // always do: running the scan to end of input instead is how `@import/*red`
    // quietly became an import of "/*red".
    for src in ["@import/*red", "@media/*x", "@font-face/*a", "a{b:c/*x"] {
        match Css::new().parse(src) {
            Ok(value) => panic!("{src:?} must not parse, but gave {}", value.to_json()),
            Err(err) => assert_eq!(
                "unterminated_comment", err.code,
                "{src:?} must be rejected as an unterminated comment"
            ),
        }
    }
}

#[test]
fn a_document_without_a_selector_is_rejected_as_reworkcss_rejects_it() {
    // CSS Syntax Level 3 would accept these as a qualified rule with an empty
    // prelude. reworkcss — the model this plugin follows — throws "selector
    // missing", and so does this port.
    for src in ["{size: large}", "{a:1}", "{}", "}", "a{c:1} extra"] {
        assert!(
            Css::new().parse(src).is_err(),
            "{src:?} must be rejected, as reworkcss rejects it"
        );
    }
}

// --- What a shared fixture cannot express ---------------------------------

#[test]
fn emitted_json_puts_type_first_and_keeps_source_order() {
    // The fixtures compare canonical (sorted-key) forms, so key ORDER is
    // deliberately invisible to them. It is what makes an AST readable, so
    // it is pinned here instead.
    let out = json("@-webkit-keyframes x { to { opacity: 1 } }");
    assert!(
        out.starts_with(r#"{"type":"stylesheet","rules":["#),
        "stylesheet key order changed: {out}"
    );
    assert!(
        out.contains(r#"{"type":"keyframes","name":"x","vendor":"-webkit-","keyframes":["#),
        "keyframes key order changed: {out}"
    );
}

#[test]
fn an_unrecorded_position_end_is_absent_from_the_json_not_null() {
    // A declaration with an empty value never runs the action that records
    // its end, so `end` stays unset. The canonical TypeScript port writes
    // `end: undefined`, which JSON.stringify omits; this port keeps the key
    // in place and omits it too. A fixture cannot pin this, because the Go
    // port emits `"end": null` there and a shared row must pass everywhere.
    let ast = Css::with_options(Options {
        position: true,
        ..Options::default()
    })
    .parse("p { color:; }")
    .expect("must parse");
    let out = ast.to_json();
    assert!(
        out.contains(r#""value":"","position":{"start":{"line":1,"column":5}}}"#),
        "a declaration with no recorded end must serialise without one: {out}"
    );
    assert!(
        !out.contains("\"end\":null"),
        "no key may serialise as null: {out}"
    );
}

#[test]
fn the_end_of_input_overshoot_follows_the_canonical_column_arithmetic() {
    // A `\` at the last character of the source is an escape whose escaped
    // character is not there, so the scanners step ONE PAST the end. The
    // canonical TypeScript port counts that step in the column while taking
    // the clamped substring for the token, so `@host\` — six characters —
    // ends at column 8. This port follows it, because TypeScript is
    // canonical. The Go port clamps the index first and reports 7, so a
    // shared fixture for this would be red there by construction and the
    // assertion lives here instead.
    let ast = Css::with_options(Options {
        position: true,
        ..Options::default()
    })
    .parse("@host\\")
    .expect("must parse");
    let out = ast.to_json();
    assert!(
        out.contains(r#""position":{"start":{"line":1,"column":1},"end":{"line":1,"column":7}}"#),
        "the at-rule node ends at the clamped width, six characters on: {out}"
    );
    assert!(
        out.ends_with(r#""position":{"start":{"line":1,"column":1},"end":{"line":1,"column":8}}}"#),
        "the stylesheet ends one past the source, as the canonical port has it: {out}"
    );
}

#[test]
fn an_error_reports_where_it_stopped() {
    // The shared fixtures pin the error CODE and nothing else, so the line
    // and column are pinned here.
    let err = Css::new()
        .parse("a { x: 1 }\n\n  }")
        .expect_err("a stray '}' must not parse");
    assert_eq!("unexpected", err.code);
    assert_eq!(3, err.line);
    assert_eq!(3, err.column);
    assert!(
        err.to_string().contains("line 3, column 3"),
        "the Display form should say where: {err}"
    );
}

#[test]
fn deep_nesting_does_not_exhaust_the_stack() {
    // The rule machine keeps its own stack rather than recursing, so depth is
    // bounded by memory and not by the thread's stack. A recursive-descent
    // port would abort the process here rather than fail a test, which is why
    // this is worth pinning.
    const DEPTH: usize = 20_000;
    let src = format!("{}color: red{}", "a { ".repeat(DEPTH), " }".repeat(DEPTH));
    let ast = Css::new()
        .parse(&src)
        .expect("deeply nested CSS must parse");

    // Serialising, formatting, cloning and comparing are iterative too, and
    // each of them would abort the process here if it were derived. They are
    // exercised on the same tree rather than in tests of their own, because
    // what is being pinned is that NOTHING reachable from a parse result
    // recurses once per level.
    assert!(ast.to_json().starts_with(r#"{"type":"stylesheet""#));
    assert!(format!("{ast:?}").starts_with(r#"{"type":"stylesheet""#));
    let copy = ast.clone();
    assert!(copy == ast, "a deep clone compares equal to its source");

    let mut node = ast.as_node().expect("a stylesheet node");
    let mut depth = 0;
    while let Some(Value::List(children)) = node.get("rules").or_else(|| node.get("declarations")) {
        let Some(Value::Node(child)) = children.first() else {
            break;
        };
        if Some("rule") != child.node_type() {
            break;
        }
        node = child;
        depth += 1;
    }
    assert_eq!(DEPTH, depth, "every nesting level should be in the tree");
}

#[test]
fn the_debug_view_shows_an_undefined_key_that_the_json_drops() {
    // The debug view exists to show what is there, so it keeps a key the
    // JSON output omits. That is the one place the two writers differ.
    let ast = Css::with_options(Options {
        position: true,
        ..Options::default()
    })
    .parse("p { color:; }")
    .expect("must parse");
    assert!(
        format!("{ast:?}").contains(r#""end":undefined"#),
        "the debug view should show the unrecorded end: {ast:?}"
    );
    // The same declaration serialises with a `start` and nothing after it.
    // (Its rule and the stylesheet DO have ends, so this looks at the one
    // node that does not rather than at the word "end" anywhere.)
    let json = ast.to_json();
    assert!(
        json.contains(r#""value":"","position":{"start":{"line":1,"column":5}}}"#),
        "the JSON should drop the key entirely: {json}"
    );
    assert!(
        !json.contains("undefined") && !json.contains("null"),
        "and should render no placeholder for it: {json}"
    );
}

#[test]
fn text_is_trimmed_the_way_javascript_trims_it() {
    // `str::trim` is NOT `String.prototype.trim`. The two sets differ by
    // exactly two code points, and both change the AST:
    //
    //   U+FEFF is ECMAScript whitespace and not Unicode White_Space;
    //   U+0085 is Unicode White_Space and not ECMAScript whitespace.
    //
    // The canonical TypeScript port trims the first and keeps the second, so
    // this port does too. The Go port uses strings.TrimSpace, which is the
    // Unicode set, so it does the opposite on both. That is a third TS/Go
    // divergence, and the reason these cannot be shared fixtures.
    let cases: [(&str, &str); 4] = [
        // A byte-order mark before a selector is trimmed away,
        ("\u{feff}a{b:c}", r#""selectors":["a"]"#),
        // and after a value.
        ("a{b:c\u{feff}}", r#""value":"c""#),
        // U+0085 is not whitespace here, so it stays in the value,
        ("a{b:c\u{85}}", "\"value\":\"c\u{85}\""),
        // and in an at-rule prelude.
        ("@media x \u{85}{a{b:c}}", "\"media\":\"x \u{85}\""),
    ];
    for (src, want) in cases {
        let out = ast(src).to_json();
        assert!(out.contains(want), "{src:?} should contain {want:?}: {out}");
    }

    // `@custom-media` splits its params at the first whitespace after the
    // name, and a JavaScript regex's whitespace class is that same set
    // again, so U+0085 does not end the name.
    let out = ast("@custom-media --n\u{85}(x);").to_json();
    assert!(
        out.contains("\"name\":\"--n\u{85}(x)\",\"media\":\"\""),
        "the name should run through U+0085: {out}"
    );
}

#[test]
fn pathological_escapes_and_unterminated_strings_do_not_panic() {
    // The scanners deliberately overshoot (a `\` at the last character is an
    // escape whose escaped character is not there), and Rust panics on a
    // slice bound that is out of range OR inside a character. Neither may
    // reach a caller. The Go port's equivalent hazard was a live
    // `slice bounds out of range` panic before it was found.
    for src in [
        "@host\\",
        "@viewport\\",
        "a{b:\"x\\",
        "a{b:'x\\",
        "a\\",
        ".\\",
        "#\u{a9}\\",
        "#\u{1d11e}\\",
        "a{b:c\\",
        "@media \u{a9}\\",
        ".x\\\u{1d11e}{y:z}",
        "a[\u{a9}\\",
    ] {
        // The verdict does not matter here; not aborting the process does.
        let _ = Css::new().parse(src);
        let _ = Css::with_options(Options {
            position: true,
            ..Options::default()
        })
        .parse(src);
    }
}

#[test]
fn the_cached_parser_is_shared_safely_across_threads() {
    const SRC: &str = "a { color: red } @media x { b { c: d } }";
    let want = canonical(&ast(SRC));
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let want = want.clone();
            std::thread::spawn(move || {
                for _ in 0..200 {
                    let got = tabnas_css::parse(SRC).expect("threaded parse failed");
                    assert_eq!(want, canonical(&got));
                }
            })
        })
        .collect();
    for handle in handles {
        handle.join().expect("a worker thread panicked");
    }
}

// --- The public API -------------------------------------------------------

#[test]
fn options_default_to_the_canonical_ports_defaults() {
    let defaults = Options::default();
    assert!(!defaults.lowercase_properties);
    assert!(!defaults.position);
    assert_eq!(defaults, Css::new().options());
    assert!(
        !json("A { COLOR: Red }").contains(r#""property":"color""#),
        "property names must keep their case by default"
    );
}

#[test]
fn parse_with_applies_options_without_building_a_named_parser() {
    let ast = tabnas_css::parse_with(
        "A { COLOR: Red }",
        Options {
            lowercase_properties: true,
            ..Options::default()
        },
    )
    .expect("must parse");
    assert!(ast
        .to_json()
        .contains(r#""property":"color","value":"Red""#));
}

#[test]
fn values_can_be_walked_without_going_through_json() {
    let ast = ast("h1, h2 { margin: 0 }");
    let sheet = ast.as_node().expect("a stylesheet node");
    assert_eq!(Some("stylesheet"), sheet.node_type());

    let rules = sheet.get("rules").and_then(Value::as_list).expect("rules");
    assert_eq!(1, rules.len());

    let rule = rules[0].as_node().expect("a rule node");
    let selectors: Vec<&str> = rule
        .get("selectors")
        .and_then(Value::as_list)
        .expect("selectors")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(vec!["h1", "h2"], selectors);

    let decl = rule
        .get("declarations")
        .and_then(Value::as_list)
        .and_then(|d| d.first())
        .and_then(Value::as_node)
        .expect("a declaration node");
    assert_eq!(Some("margin"), decl.get("property").and_then(Value::as_str));
}

#[test]
fn json_output_escapes_what_json_stringify_escapes() {
    assert_eq!(
        r#"{"type":"stylesheet","rules":[{"type":"comment","comment":" a\"b\\c\nd\te "}]}"#,
        json("/* a\"b\\c\nd\te */")
    );
    assert_eq!(
        "\"\\u0001\"",
        Value::Str("\u{1}".to_string()).to_json(),
        "control characters take the \\u form"
    );
    assert_eq!(
        "2",
        Value::Num(2.0).to_json(),
        "integral numbers lose the .0"
    );
}
