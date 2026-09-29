/* Copyright (c) 2025 Richard Rodger and other contributors, MIT License */

//! What a parse costs in memory, measured by a counting allocator.
//!
//! The engine keeps a snapshot per rule, and by default every rule keeps
//! the links to the rules it replaced, so a list's items were all held
//! until the list closed: a flat stylesheet of 100,000 rules (1.7 MB) peaked
//! at 927 MB. The grammar's options bound that history (`rule.history: 1`
//! in `src/grammar.rs`), which brought it to 210 MB. Nesting is bounded by
//! nothing but memory, and costs the engine's frames for every open level.
//! The ceilings below are about twice what each case measured when they were
//! set, so a return of per-item retention fails here, and ordinary noise
//! does not.
//!
//! One test in its own binary: the allocator counts the whole process, so
//! a second test running beside it would be counted too.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Counting;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = System.alloc(layout);
        if !p.is_null() {
            let live = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(live, Ordering::Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        System.dealloc(p, layout);
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
    }
}

#[global_allocator]
static COUNTING: Counting = Counting;

/// The most memory held at once while `src` parses, above what was held
/// before, in bytes.
fn peak_of(css: &tabnas_css::Css, src: &str) -> usize {
    let base = LIVE.load(Ordering::Relaxed);
    PEAK.store(base, Ordering::Relaxed);
    let ast = css.parse(src).expect("parses");
    let peak = PEAK.load(Ordering::Relaxed) - base;
    drop(ast);
    peak
}

#[test]
fn a_parse_holds_memory_in_proportion_to_its_input() {
    let css = tabnas_css::Css::new();
    let flat = "a { color: red }\n".repeat(10_000);
    let nested = format!("{}color: red{}", "a { ".repeat(2_000), " }".repeat(2_000));
    // Measured when set: 19.6 MB flat (87 MB with the history unbounded)
    // and 12.8 MB nested.
    const MB: usize = 1024 * 1024;
    let flat_peak = peak_of(&css, &flat);
    assert!(
        flat_peak < 40 * MB,
        "10,000 flat rules held {} MB at once",
        flat_peak / MB
    );
    let nested_peak = peak_of(&css, &nested);
    assert!(
        nested_peak < 26 * MB,
        "2,000 nested rules held {} MB at once",
        nested_peak / MB
    );
}
