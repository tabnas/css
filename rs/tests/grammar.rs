/* Copyright (c) 2025 Richard Rodger and other contributors, MIT License */

//! The embedded grammar, and the rules the plugin installs from it.
//!
//! `css-grammar.jsonic` at the repository root is the single source of truth,
//! copied verbatim into the TypeScript, Go and Rust sources by
//! `ts/embed-grammar.js`. An embed that was never re-run is invisible at
//! runtime — the crate simply parses against the previous grammar — so it is
//! checked here rather than trusted.

mod support;

use tabnas_css::grammar::grammar_text;

use support::repo_root;

#[test]
fn embedded_grammar_matches_the_file_on_disk() {
    let path = repo_root().join("css-grammar.jsonic");
    let on_disk = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));

    // The embed writes a newline right after the opening `r##"`, so the
    // literal begins with one the file does not have.
    let embedded = grammar_text().strip_prefix('\n').unwrap_or(grammar_text());

    assert_eq!(
        embedded, on_disk,
        "the embedded grammar is not css-grammar.jsonic.\n\
         Never hand-edit between the BEGIN/END EMBEDDED markers — edit \
         css-grammar.jsonic and re-run `npm run embed` from ts/."
    );
}

/// The thirteen rules `css-grammar.jsonic` defines, in its order.
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

#[test]
fn the_plugin_installs_the_grammars_rules() {
    let parser = tabnas_css::make();
    let specs = parser.rule_specs();
    for name in RULES {
        let rule = specs
            .iter()
            .find(|spec| name == spec.name)
            .unwrap_or_else(|| panic!("the plugin installed no rule '{name}'"));
        assert!(
            !rule.open.is_empty(),
            "rule '{name}' has no open alternatives, so it can never match"
        );
    }
}

#[test]
fn on_a_bare_engine_the_rules_are_the_grammars_alone() {
    // jsonic's five rules stay installed under the plugin, with their
    // jsonic alternates excluded (`rule.exclude: "jsonic,imp"`); on a bare
    // engine there is nothing but the css grammar.
    let mut parser = tabnas::Tabnas::new();
    tabnas_css::css(&mut parser, &tabnas_css::Options::default()).expect("installs");
    let mut names: Vec<&str> = parser
        .rule_specs()
        .iter()
        .map(|spec| spec.name.as_str())
        .collect();
    names.sort_unstable();
    let mut want = RULES.to_vec();
    want.sort_unstable();
    assert_eq!(want, names);
}

#[test]
fn every_alternate_carries_the_css_group() {
    // `tn.grammar(def, { rule: { alt: { g: 'css' } } })` in the canonical
    // port: a caller can switch the whole grammar off with
    // `rule.exclude: 'css'`.
    let parser = tabnas_css::make();
    for spec in parser.rule_specs() {
        if !RULES.contains(&spec.name.as_str()) {
            continue;
        }
        for alt in spec.open.iter().chain(spec.close.iter()) {
            assert!(
                alt.g.split(',').any(|g| "css" == g.trim()),
                "an alternate of '{}' is not in the css group: {:?}",
                spec.name,
                alt.g
            );
        }
    }
}

#[test]
fn the_rules_carry_no_lifecycle_actions() {
    // The canonical grammar's rules are alternates and nothing else: no
    // before-open, after-open, before-close or after-close action. The Rust
    // plugin's own work (the arena hand-back at the end of the stylesheet)
    // runs inside the alternates' actions, so the installed rules stay the
    // canonical port's, rule for rule.
    let parser = tabnas_css::make();
    for spec in parser.rule_specs() {
        if !RULES.contains(&spec.name.as_str()) {
            continue;
        }
        let hooks = [
            spec.bo.len(),
            spec.ao.len(),
            spec.bc.len(),
            spec.ac.len(),
            spec.bo_fns.len(),
            spec.ao_fns.len(),
            spec.bc_fns.len(),
            spec.ac_fns.len(),
            spec.bo_state_fns.len(),
            spec.ao_state_fns.len(),
            spec.bc_state_fns.len(),
            spec.ac_state_fns.len(),
        ];
        assert_eq!(
            [0; 12], hooks,
            "rule '{}' has lifecycle actions (bo, ao, bc, ac; named, functions, state)",
            spec.name
        );
    }
}
