/* Copyright (c) 2025 Richard Rodger and other contributors, MIT License */

//! The embedded grammar, and the reader that turns it into rules.
//!
//! `css-grammar.jsonic` at the repository root is the single source of truth,
//! copied verbatim into the TypeScript, Go and Rust sources by
//! `ts/embed-grammar.js`. An embed that was never re-run is invisible at
//! runtime — the crate simply parses against the previous grammar — so it is
//! checked here rather than trusted.

mod support;

use tabnas_css::grammar::{grammar_text, Grammar};

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

#[test]
fn the_grammar_defines_the_rules_the_machine_runs() {
    let grammar = Grammar::load();
    for name in [
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
    ] {
        let rule = grammar
            .rule(name)
            .unwrap_or_else(|| panic!("the grammar defines no rule '{name}'"));
        assert!(
            !rule.open.is_empty(),
            "rule '{name}' has no open alternatives, so it can never match"
        );
    }
    assert_eq!(13, grammar.len(), "the grammar's rule count changed");
}

#[test]
fn alts_are_read_in_source_order_with_their_fields() {
    let grammar = Grammar::load();

    // `sel` is the rule that exercises every alt field this grammar uses:
    // a two-token `s`, a pushback `b`, a replace `r`, a push `p`, an action
    // `a` and a tag `g`.
    let sel = grammar.rule("sel").expect("no 'sel' rule");
    assert_eq!(2, sel.open.len());

    let group = &sel.open[0];
    assert_eq!(vec!["#TX", "#GC"], group.s);
    assert_eq!(0, group.b);
    assert_eq!(vec!["@cssSelector"], group.a);
    assert_eq!(Some("sel".to_string()), group.r);
    assert_eq!(None, group.p);
    assert_eq!("css,sel,group", group.g);

    let last = &sel.open[1];
    assert_eq!(vec!["#TX", "#OB"], last.s);
    assert_eq!(1, last.b, "the '{{' must be pushed back for declbody");
    assert_eq!(Some("declbody".to_string()), last.p);

    // An alt with no `s` matches unconditionally — that is how `stylesheet`
    // opens and how `items` loops.
    let stylesheet = grammar.rule("stylesheet").expect("no 'stylesheet' rule");
    assert!(stylesheet.open[0].s.is_empty());
    assert_eq!(vec!["@cssSheet"], stylesheet.open[0].a);
    assert_eq!(Some("items".to_string()), stylesheet.open[0].p);
}

#[test]
fn the_reader_accepts_the_jsonic_subset_the_grammar_is_written_in() {
    // Bare and quoted keys, single and double quotes, `#` comments, commas
    // optional, and an `a:` that is a LIST of action references — the shape
    // the Go port's buildGrammarAlts also handles.
    // `r##` rather than `r#`: the text holds `"#CB"`, and `"#` would end
    // an `r#` literal early.
    let text = r##"
# a comment, ignored
{
  rule: {
    'quoted': {
      open: [
        { s: '#TX #OB' b: 1 a: ['@one' '@two'] p: body g: 'x,y' }
        { s: "#CB", b: 0, r: "quoted" }
      ]
    }
  }
}
"##;
    let grammar = Grammar::parse(text).expect("the subset reader rejected its own dialect");
    let rule = grammar.rule("quoted").expect("no 'quoted' rule");
    assert_eq!(2, rule.open.len());
    assert!(rule.close.is_empty(), "an absent 'close' reads as no alts");
    assert_eq!(vec!["@one", "@two"], rule.open[0].a);
    assert_eq!(1, rule.open[0].b);
    assert_eq!(Some("body".to_string()), rule.open[0].p);
    assert_eq!(vec!["#CB"], rule.open[1].s);
    assert_eq!(Some("quoted".to_string()), rule.open[1].r);
}

#[test]
fn the_reader_reports_malformed_input_rather_than_panicking() {
    for bad in ["{", "{ rule", "{ a: 'unclosed }", "[1 2 3]", ""] {
        assert!(
            Grammar::parse(bad).is_err(),
            "the reader accepted malformed grammar text {bad:?}"
        );
    }
}

// A field this reader does not implement is an ERROR, at every level of the
// document.
//
// The canonical ports hand the grammar to an engine that understands the
// whole jsonic alt surface; this port implements the six alt fields
// `css-grammar.jsonic` uses. Reading an unknown one as absent would leave
// this runtime running a DIFFERENT grammar from the other two, silently and
// with every suite green, which is the failure mode the shared grammar file
// exists to prevent. Grammar::load panics on such a document, so the build
// stops instead.
#[test]
fn a_grammar_field_the_machine_does_not_run_is_rejected() {
    let cases = [
        // An alt field: `c` is jsonic's condition, which this port has no
        // machinery for.
        (
            "{ rule: { r1: { open: [ { s: '#TX' c: 'cond' } ] } } }",
            "\"c\"",
        ),
        // A rule-level hook: `bo` runs before the rule opens.
        ("{ rule: { r1: { bo: '@x' open: [] } } }", "\"bo\""),
        // A top-level section: `options` would change the lexer, not a rule.
        (
            "{ rule: { r1: { open: [] } } options: { x: 1 } }",
            "\"options\"",
        ),
    ];

    for (text, named) in cases {
        let error = Grammar::parse(text)
            .err()
            .unwrap_or_else(|| panic!("{text}: read without error, so the field was DROPPED"));
        assert!(
            error.contains(named) && error.contains("unknown field"),
            "{text}: the error must name {named}, got {error:?}"
        );
    }
}

// The fields the grammar DOES use are read, not rejected: the check above
// must not be satisfiable by refusing everything.
#[test]
fn every_field_the_grammar_uses_is_implemented() {
    Grammar::parse(
        "{ rule: { r1: { \
         open: [ { s: '#TX #OB' b: 1 p: sub a: '@cssRule' g: 'css' } ] \
         close: [ { s: '#CB' r: items a: [ '@cssEnd' ] g: 'css,end' } ] } } }",
    )
    .expect("the six alt fields css-grammar.jsonic uses must read");

    // And the grammar this crate ships reads, which is the case that matters.
    Grammar::parse(grammar_text()).expect("the embedded grammar must read");
}
