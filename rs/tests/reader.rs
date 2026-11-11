/* Copyright (c) 2026 Richard Rodger and other contributors, MIT License */

//! A parse from a reader (`Tabnas::parse_reader`, admin ADR-30 and ADR-33).
//!
//! [`make`] and [`make_with`] build the parser whole, jsonic's base with the
//! plugin installed, and declare `lex.window` on it, so that a reader is
//! read a window at a time. The `cssToken` matcher reads the source only
//! through the window: a scan that reaches the window's end fills it and the
//! token is planned again. These tests hold it to that: the constructors
//! declare the window and the plugin does not; every row of the shared
//! fixtures, under its `opts` and again with positions on, and every
//! document of the reworkcss corpus, with and without positions, parses the
//! same from a reader, in pieces of 1, 3 and 64 bytes, under windows of a
//! few characters as well as the engine's own, as from its text: the same
//! value, or the same error code, position, row, column and token text; and
//! tokens of every kind longer than the window, and a long document read
//! under a window of 256 characters that moves on hundreds of times, parse
//! the same too.

mod support;

use std::io::Read;
use std::time::{Duration, Instant};

use tabnas::{Tabnas, WindowSizes};
use tabnas_css::{css, make, make_with, plugin, Options, Value};

use support::corpus::{corpus_dir, extract_error_cases, fetch_corpus, read_case};
use support::{json, read_spec_dir, spec_dir};

/// A reader that hands its bytes over at most `size` at a time.
struct Pieces<'a> {
    bytes: &'a [u8],
    at: usize,
    size: usize,
}

impl<'a> Pieces<'a> {
    fn new(bytes: &'a [u8], size: usize) -> Self {
        Pieces { bytes, at: 0, size }
    }
}

impl Read for Pieces<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.size.min(buf.len()).min(self.bytes.len() - self.at);
        buf[..n].copy_from_slice(&self.bytes[self.at..self.at + n]);
        self.at += n;
        Ok(n)
    }
}

/// What a parse shows: its value, or its error's code, position, row,
/// column and token text. An error's excerpt is left out: a parse from a
/// reader shows a long line only as far back as the window reaches
/// (tabnas/parser `DIVERGENCE.md`), the one difference allowed.
fn outcome(parsed: Result<tabnas::Value, tabnas::TabnasError>) -> String {
    match parsed {
        Ok(value) => format!("ok {}", value.to_json()),
        Err(e) => format!(
            "err {} at {} {}:{} {:?}",
            e.code, e.pos, e.row, e.col, e.src
        ),
    }
}

/// The first `n` bytes of `s`, cut back to a character boundary.
fn cut(s: &str, n: usize) -> &str {
    let mut end = n.min(s.len());
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// The pieces a document is handed over in, and the windows (margin and
/// block, in characters) it is read under besides the engine's own: a few
/// characters, so that every scan of the matcher meets the window's edge.
const PIECES: [usize; 3] = [1, 3, 64];
const WINDOWS: [(usize, usize); 3] = [(1, 1), (3, 2), (64, 64)];

/// `text` parsed by `parser` whole, and from a reader in each of [`PIECES`]
/// under the engine's window and each of [`WINDOWS`]: how many reader
/// parses were compared, and where they differ.
fn same_from_reader(parser: &Tabnas, at: &str, text: &str) -> (usize, Vec<String>) {
    let whole = outcome(parser.parse(text));
    let bytes = text.as_bytes();
    let mut reads: Vec<(String, String)> = Vec::new();
    for size in PIECES {
        reads.push((
            format!("pieces of {size}"),
            outcome(parser.parse_reader(Pieces::new(bytes, size))),
        ));
        for (margin, block) in WINDOWS {
            let sizes = WindowSizes {
                margin,
                block,
                chunk: size,
            };
            reads.push((
                format!("pieces of {size}, margin {margin}, block {block}"),
                outcome(parser.parse_reader_with_sizes(Pieces::new(bytes, size), sizes)),
            ));
        }
    }
    (reads.len(), differences(at, text, &whole, &reads))
}

/// The reads that differ from the whole text's parse, described.
fn differences(at: &str, text: &str, whole: &str, reads: &[(String, String)]) -> Vec<String> {
    reads
        .iter()
        .filter(|(_, read)| read != whole)
        .map(|(how, read)| {
            format!(
                "{at}: {:?} from a reader in {how}\n  whole:  {}\n  reader: {}",
                cut(text, 80),
                cut(whole, 300),
                cut(read, 300)
            )
        })
        .collect()
}

/// A row's options from its `opts` column, raw JSON in the canonical
/// port's camelCase names.
fn options(opts: &str) -> Options {
    if opts.trim().is_empty() {
        return Options::default();
    }
    let parsed = json(opts).unwrap_or_else(|e| panic!("unreadable opts JSON {opts:?}: {e}"));
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
fn make_reads_a_reader_a_window_at_a_time() {
    assert!(make().options.lex.window);
    let positioned = Options {
        position: true,
        ..Options::default()
    };
    assert!(make_with(positioned).options.lex.window);
    // The plugin alone declares nothing: a host may install it beneath
    // matchers of its own.
    // jsonic's base declares the window itself (tabnas/jsonic#121), so the
    // plugin goes on that base with the window turned off.
    let mut parser = tabnas_jsonic::make_with(|options| options.lex.window = false);
    parser
        .use_plugin(plugin(), None)
        .expect("the plugin installs");
    assert!(!parser.options.lex.window);
    let mut bare = Tabnas::new();
    css(&mut bare, &Options::default()).expect("the plugin installs");
    assert!(!bare.options.lex.window);
}

/// Every fixture row, whole, through [`make_with`] and through the plugin
/// on jsonic's base, which is what it builds, but for the window.
#[test]
fn make_parses_as_the_plugin_on_jsonic() {
    let rows = read_spec_dir(&spec_dir());
    let mut failures = Vec::new();
    for row in &rows {
        let options = options(&row.opts);
        let mut composed = tabnas_jsonic::make();
        composed
            .use_plugin(plugin(), Some(options.to_value()))
            .expect("the plugin installs");
        let made = outcome(make_with(options).parse(&row.input));
        let base = outcome(composed.parse(&row.input));
        if made != base {
            failures.push(format!(
                "{}\n  make:     {made}\n  composed: {base}",
                row.label()
            ));
        }
    }
    assert!(rows.len() > 150, "{}", rows.len());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Every row of the shared fixtures, under its options and again with
/// positions on, whose columns count from the start of a line the window
/// may have dropped, parses the same from a reader as from its text.
#[test]
fn every_row_parses_the_same_from_a_reader() {
    let rows = read_spec_dir(&spec_dir());
    let mut compared = 0;
    let mut failures = Vec::new();
    for (n, row) in rows.iter().enumerate() {
        let options = options(&row.opts);
        let mut runs = vec![(options, row.label())];
        if !options.position {
            let positioned = Options {
                position: true,
                ..options
            };
            runs.push((positioned, format!("{} (position)", row.label())));
        }
        for (options, at) in runs {
            let (count, found) = same_from_reader(&make_with(options), &at, &row.input);
            compared += count;
            failures.extend(found);
        }
        if (n + 1) % 100 == 0 || n + 1 == rows.len() {
            eprintln!(
                "reader fixtures: {} of {} rows ({}%), {compared} reader parses",
                n + 1,
                rows.len(),
                (n + 1) * 100 / rows.len()
            );
        }
    }
    assert!(compared > 4_000, "{compared}");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Each scan of the matcher with what it skips (a comment, a string, an
/// escape, brackets) where a window of a few characters ends inside it,
/// and each of its errors and the end-of-input overshoot.
#[test]
fn every_scan_meets_the_window_edge() {
    let documents = [
        "a { b: c /* x */ d }",
        "a { b: /* x; } */ c; d: e }",
        "a { b: \"x;}\" c }",
        "a { b: c\\;d; e: f }",
        "a { b: f(x;y) [z}] }",
        "a /* x */ , b { c: d }",
        "a[x=\"{;}\"], b:not(.c, .d) { e: f }",
        "a\\{b { c: d }",
        "@media /* x { */ screen { a { b: c } }",
        "@import \"a;b\" /* c; */;",
        "@import url(a;b);",
        "@font-face { src: url(a) }",
        "@-webkit-keyframes k { 0%, /* x */ 50% { a: b } to { a: c } }",
        "a { b/**/: c }",
        "a { *zoom: 1; _x: 2; opacity[sqrt]: 3; //y: 4 }",
        "/* a */ /* b */ x { y: z }",
        "@host\\",
        "a { b: c\\",
        ".a\\",
        "a { b: c } /* open",
        "a { b: c /* open",
        "@media x /* open",
        "a /* open",
        "a { b: c } /",
        "\u{e9}\u{1d11e} { b: \u{1d11e}\u{e9} } /* \u{1d11e} */ c { d: e }",
    ];
    let mut compared = 0;
    let mut failures = Vec::new();
    for position in [false, true] {
        let parser = make_with(Options {
            position,
            ..Options::default()
        });
        for document in documents {
            let (count, found) =
                same_from_reader(&parser, &format!("position {position}"), document);
            compared += count;
            failures.extend(found);
        }
    }
    assert!(compared > 500, "{compared}");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A selector group's later items take the answer the first item's
/// lookahead found (`BraceScans`), read on from where it last stopped. With
/// no consumed tokens kept (`rewind.history` 0) a window of a few
/// characters drops the text behind the lookahead, the kept scan's among
/// it, and an item whose kept scan the window no longer holds is
/// classified afresh, to the same answer.
#[test]
fn a_kept_scan_the_window_dropped_is_passed_over() {
    let groups = "h1 a, h2 b, h3 [x=\"{\"], h4 d { x: y }\n@keyframes k { 0%, 50%, to { o: 1 } }\n";
    let document = groups.repeat(40);
    let mut compared = 0;
    let mut failures = Vec::new();
    for position in [false, true] {
        let mut parser = make_with(Options {
            position,
            ..Options::default()
        });
        parser
            .set_options(|options| options.rewind.history = Some(0))
            .expect("rewind.history sets");
        let (count, found) = same_from_reader(
            &parser,
            &format!("position {position}, rewind.history 0"),
            &document,
        );
        compared += count;
        failures.extend(found);
    }
    assert!(compared > 20, "{compared}");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Every case of the reworkcss corpus, with and without positions, and
/// every input of its accept/reject oracle, parses the same from a reader
/// as from its text.
#[test]
fn the_corpus_parses_the_same_from_a_reader() {
    fetch_corpus();
    let cases = corpus_dir().join("test").join("cases");
    let mut names: Vec<String> = std::fs::read_dir(&cases)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", cases.display()))
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    let plain = make();
    let positioned = make_with(Options {
        position: true,
        ..Options::default()
    });
    let mut compared = 0;
    let mut failures = Vec::new();
    for name in &names {
        let input = read_case(&cases.join(name).join("input.css"));
        for (parser, at) in [
            (&plain, format!("cases/{name}")),
            (&positioned, format!("cases/{name} (position)")),
        ] {
            let (count, found) = same_from_reader(parser, &at, &input);
            compared += count;
            failures.extend(found);
        }
    }
    let oracle = extract_error_cases();
    for case in &oracle {
        let (count, found) =
            same_from_reader(&plain, &format!("parse.js {}", case.kind), &case.src);
        compared += count;
        failures.extend(found);
    }
    eprintln!(
        "reader corpus: {} cases and {} oracle inputs (100%), {compared} reader parses",
        names.len(),
        oracle.len()
    );
    assert_eq!(names.len(), 46);
    assert!(compared > 1_000, "{compared}");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Tokens and lookaheads of every kind longer than the engine's window,
/// which reaches 64 KiB past the cursor before each token, and errors past
/// it, parse the same from a reader as from their text, with and without
/// positions.
#[test]
fn tokens_longer_than_the_window_parse_the_same_from_a_reader() {
    let long = 64 * 1024 + 2048;
    let third = long / 3;
    let documents = [
        // A selector, and a group whose `{` lies past the window, past a
        // comment too.
        format!(".{} {{ b: c }}", "s".repeat(long)),
        format!(
            "h1 {}, h2 {}, h3 {} {{ b: c }}",
            "x".repeat(third),
            "y".repeat(third),
            "z".repeat(third)
        ),
        format!("a /* {} */ , b {{ c: d }}", "{;}".repeat(third)),
        // A property name, and a value, with a string holding `;` and `}`,
        // and with a comment holding them.
        format!("a {{ {}: c }}", "p".repeat(long)),
        format!("a {{ b: {} }}", "v".repeat(long)),
        format!("a {{ b: \"{}\"; c: d }}", ";}".repeat(long / 2)),
        format!("a {{ b: c /* {} */ d; e: f }}", ";}".repeat(long / 2)),
        // A comment node, and a comment the declaration skips.
        format!("/* {} */ a {{ b: c }}", "c".repeat(long)),
        format!("a {{ b /* {} */ : c }}", "c".repeat(long)),
        // An at-rule's prelude, with a comment, a statement's params, and
        // its keyword.
        format!("@media {} {{ a {{ b: c }} }}", "m".repeat(long)),
        format!("@media x /* {} */ {{ a {{ b: c }} }}", "{;}".repeat(third)),
        format!("@import \"{}\"; a {{ b: c }}", "u".repeat(long)),
        format!("@{} x; a {{ b: c }}", "k".repeat(long)),
        // Escapes, brackets and parentheses the scans skip.
        format!(".{} {{ b: c }}", "\\:".repeat(long / 2)),
        format!("a[x=\"{}\"] {{ b: c }}", "q".repeat(long)),
        format!("a {{ b: url({}) }}", "w".repeat(long)),
        // Space and lines between tokens.
        format!("a {{{}b: c }}", " ".repeat(long)),
        format!("a {{{}b: c }}", "\n".repeat(long)),
        // Characters past ASCII, astral ones among them, before a node on
        // its line.
        format!("{}{{a:b}} c {{ d: \u{e9} }}", "\u{1d11e}".repeat(long)),
        // Errors: an unclosed comment, at the end and in a value, and a
        // selector with no block.
        format!("a {{ b: c }} /* {}", "u".repeat(long)),
        format!("a {{ b: c /* {}", "u".repeat(long)),
        "s".repeat(long),
    ];
    let mut failures = Vec::new();
    for position in [false, true] {
        let parser = make_with(Options {
            position,
            ..Options::default()
        });
        for (n, document) in documents.iter().enumerate() {
            let whole = outcome(parser.parse(document));
            let reads: Vec<(String, String)> = [4096, 64 * 1024 + 3]
                .iter()
                .map(|&size| {
                    (
                        format!("pieces of {size}"),
                        outcome(parser.parse_reader(Pieces::new(document.as_bytes(), size))),
                    )
                })
                .collect();
            failures.extend(differences(
                &format!("position {position}, document {n}"),
                document,
                &whole,
                &reads,
            ));
        }
        eprintln!(
            "reader long tokens: position {position}, {} documents",
            documents.len()
        );
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A long document, from a reader in pieces of 4 KiB, under the engine's
/// window and under one of 256 characters, which moves on hundreds of times
/// before the end, parses the same as its text, with and without positions;
/// so does the same document with an error at its end, which the reader
/// places where the whole text does.
#[test]
fn a_long_document_parses_the_same_from_a_reader() {
    let mut body = String::from("/* sheet */\n@charset \"utf-8\";\n");
    for i in 0..2_000 {
        body.push_str(&format!(
            ".c{i}, h{} > a[href^=\"x;{i}\"]:not(.d, .e) {{\n  color: #{}; /* note {i} */\n  \
             background: url(\"i{i}.png\") no-repeat; content: \"\u{e9}\u{1d11e}{i}\";\n}}\n",
            i % 6,
            i % 999
        ));
        if i % 100 == 0 {
            body.push_str(&format!(
                "@media (min-width: {i}px) {{ a {{ b: {i} }} }}\n\
                 @keyframes k{i} {{ from {{ o: 0 }} 50%, to {{ o: 1 }} }}\n"
            ));
        }
    }
    let text = format!("{body}z {{ last: 1 }}\n");
    let broken = format!("{body}z {{ last: 1 }} /* open\n");
    assert!(text.len() > 256 * 1024, "{}", text.len());
    let mut failures = Vec::new();
    for position in [false, true] {
        let parser = make_with(Options {
            position,
            ..Options::default()
        });
        for (n, document) in [&text, &broken].into_iter().enumerate() {
            let whole = outcome(parser.parse(document));
            assert_eq!(whole.starts_with("ok "), 0 == n, "{}", cut(&whole, 200));
            let small = WindowSizes {
                margin: 256,
                block: 256,
                chunk: 4096,
            };
            let reads = vec![
                (
                    "pieces of 4096".to_string(),
                    outcome(parser.parse_reader(Pieces::new(document.as_bytes(), 4096))),
                ),
                (
                    "pieces of 4096, margin 256, block 256".to_string(),
                    outcome(
                        parser
                            .parse_reader_with_sizes(Pieces::new(document.as_bytes(), 4096), small),
                    ),
                ),
            ];
            failures.extend(differences(
                &format!("position {position}, document {n}"),
                document,
                &whole,
                &reads,
            ));
        }
        eprintln!(
            "reader long document: position {position}, {} bytes, whole and broken",
            text.len()
        );
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The fastest of a few parses of `src` from a reader, in pieces of 4 KiB
/// under a window of 256 characters, which moves on many times, to keep a
/// loaded machine's stalls and a coarse clock out of the comparison.
fn fastest_read(src: &str) -> Duration {
    let parser = make();
    let sizes = WindowSizes {
        margin: 256,
        block: 256,
        chunk: 4096,
    };
    (0..3)
        .map(|_| {
            let start = Instant::now();
            parser
                .parse_reader_with_sizes(Pieces::new(src.as_bytes(), 4096), sizes)
                .expect("the document parses");
            start.elapsed()
        })
        .min()
        .expect("three runs")
}

/// A document from a reader parses in time linear in its length: ten times
/// the rules take about ten times as long, and twenty times a selector
/// group's items, each classified by the lookahead to the group's `{`,
/// about twenty times. Quadratic work would take a hundred and four
/// hundred; the bounds are generous so a busy machine cannot fail them.
#[test]
fn a_document_from_a_reader_parses_in_linear_time() {
    let small = fastest_read(&"a{b:c}\n".repeat(1_000));
    let large = fastest_read(&"a{b:c}\n".repeat(10_000));
    let ratio = large.as_secs_f64() / small.as_secs_f64().max(1e-6);
    assert!(
        ratio < 30.0,
        "1,000 rules took {small:?} and 10,000 took {large:?}: {ratio:.1}x"
    );
    let group = |n: usize| format!("{}b{{c:d}}", "a,".repeat(n));
    let small = fastest_read(&group(1_000));
    let large = fastest_read(&group(20_000));
    let ratio = large.as_secs_f64() / small.as_secs_f64().max(1e-6);
    assert!(
        ratio < 60.0,
        "1,000 selectors took {small:?} and 20,000 took {large:?}: {ratio:.1}x"
    );
    eprintln!("reader linear time: 20,000 selectors {large:?}, {ratio:.1}x 1,000");
}
