/* Copyright (c) 2025 Richard Rodger and other contributors, MIT License */

//! Composition test: the css grammar plugin layered with the tabnas-debug
//! introspection plugin, the Rust half of `ts/test/debug-model.test.ts`.
//!
//! The TypeScript suite resolves `@tabnas/debug` dynamically and SKIPS when
//! it is absent. Here tabnas-debug is a dev-dependency on the sibling
//! checkout, like every other crate this port takes, so the test FAILS to
//! build when the checkout is missing rather than reporting green having run
//! nothing.

use tabnas::Tabnas;
use tabnas_debug::{apply, model, DebugOptions};

/// A jsonic instance with css installed and the debug plugin layered on
/// top, quiet: introspection only, no `USE:` dump and no tracing, which is
/// what `{ print: false, trace: false }` asks for in TypeScript.
fn build() -> Tabnas {
    let mut parser = tabnas_css::make();
    apply(&mut parser, DebugOptions::quiet()).expect("the debug plugin installs over css");
    parser
}

#[test]
fn parses_normally_with_the_debug_plugin_installed() {
    let parser = build();
    let value = parser.parse("a { x: 1 } /* c */").expect("parses");
    assert_eq!(
        value.to_json().to_string(),
        r#"{"type":"stylesheet","rules":[{"type":"rule","selectors":["a"],"declarations":[{"type":"declaration","property":"x","value":"1"}]},{"type":"comment","comment":" c "}]}"#
    );
}

#[test]
fn the_model_is_the_structured_css_grammar() {
    let parser = build();
    let m = model(&parser);

    // The AST-building rules are present, and the entry rule is the
    // top-level stylesheet.
    let names: Vec<&str> = m.rules.iter().map(|rule| rule.name.as_str()).collect();
    for name in [
        "stylesheet",
        "items",
        "statement",
        "sel",
        "declbody",
        "decls",
        "decl",
    ] {
        assert!(names.contains(&name), "rules should include {name}");
    }
    assert_eq!("stylesheet", m.config.start);
    assert!(
        m.plugins.iter().any(|plugin| "css" == plugin.name),
        "plugins should list css: {:?}",
        m.plugins
            .iter()
            .map(|plugin| &plugin.name)
            .collect::<Vec<_>>()
    );

    // The rule-reference graph captures the recursive AST structure: the
    // stylesheet pushes an items list; each items pushes a statement and
    // close-replaces itself to iterate; a statement opens one of the bodies
    // or a selector list; declarations iterate via decls -> decl.
    let edge = |name: &str| {
        m.graph
            .iter()
            .find(|edges| edges.name == name)
            .unwrap_or_else(|| panic!("an edge entry for {name}"))
    };
    assert_eq!(edge("stylesheet").open_push, ["items"]);
    assert_eq!(edge("items").open_push, ["statement"]);
    assert_eq!(edge("items").close_replace, ["items"]);
    assert!(
        edge("statement").open_push.iter().any(|to| "sel" == to),
        "statement should push sel (a style rule)"
    );
    assert_eq!(edge("decls").close_replace, ["decls"]);
    assert_eq!(edge("sel").open_replace, ["sel"]);
    assert_eq!(edge("kfitems").close_replace, ["kfitems"]);
    assert_eq!(edge("kfsel").open_replace, ["kfsel"]);
}
