/* Copyright (c) 2025 Richard Rodger, MIT License */

//! Parse CSS and print the AST as JSON.
//!
//! ```text
//! echo 'a { color: red }' | cargo run --example parse
//! cargo run --example parse -- --position < style.css
//! ```
//!
//! With `--lines`, each line of stdin is one document, with `\n`, `\r`, `\t`
//! and `\\` decoded — the same escape codec the shared `test/spec/*.tsv`
//! fixtures use — and one result is printed per line. That is the mode the
//! TypeScript/Rust differential probe drives.
//!
//! A document that fails to parse prints `ERROR:<code>` rather than the AST,
//! so accept/reject shows up in the same stream as the values.

use std::io::Read;

use tabnas_css::{Css, Options};

fn main() {
    let mut options = Options::default();
    let mut lines = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--position" => options.position = true,
            "--lowercase-properties" => options.lowercase_properties = true,
            "--lines" => lines = true,
            other => {
                eprintln!("parse: unknown argument {other:?}");
                eprintln!("usage: parse [--position] [--lowercase-properties] [--lines]");
                std::process::exit(2);
            }
        }
    }

    let mut input = String::new();
    if std::io::stdin().read_to_string(&mut input).is_err() {
        eprintln!("parse: stdin is not valid UTF-8");
        std::process::exit(2);
    }

    let css = Css::with_options(options);
    if lines {
        // Split on '\n' only: `str::lines` also strips a trailing '\r',
        // which would quietly drop the very characters a carriage-return
        // probe is there to test.
        let mut rows: Vec<&str> = input.split('\n').collect();
        if Some(&"") == rows.last() {
            rows.pop();
        }
        for line in rows {
            println!("{}", report(&css, &decode(line)));
        }
    } else {
        println!("{}", report(&css, &input));
    }
}

fn report(css: &Css, src: &str) -> String {
    match css.parse(src) {
        Ok(ast) => ast.to_json(),
        Err(err) => format!("ERROR:{}", err.code),
    }
}

/// Decode the `\n` `\r` `\t` `\\` escapes the shared fixtures use.
fn decode(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if '\\' != c {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}
