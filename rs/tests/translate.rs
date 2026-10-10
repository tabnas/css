/* Copyright (c) 2026 Richard Rodger, MIT License */

//! The translation parts: what the manifest says and what the crate
//! embeds are the same files.
//!
//! A packaged crate holds nothing outside `rs/`, so the crate embeds its
//! own copies, `rs/translate/manifest.json` of `tabnas.plugin.json` and
//! `rs/translate/render.alc` of the render the manifest names, and
//! `translate()` hands them to a host. The copies are the only texts a
//! host sees, so they must be the files: this holds the embedded manifest
//! to the repository's, and the render the manifest names, read from the
//! repository, to the embedded one, as it would an embed the manifest
//! named. Change the file at the root and run `npm run embed` in `ts/`,
//! which copies it into `rs/translate/`; this fails until both are the
//! same.

mod support;

use tabnas_css::{Node, Value};

use support::{json, repo_root};

fn read(path: &str) -> String {
    let file = repo_root().join(path);
    std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("cannot read {}: {e}", file.display()))
}

/// The manifest's `translate` object.
fn translate_object() -> Node {
    let manifest = json(&read("tabnas.plugin.json")).expect("the manifest is JSON");
    manifest
        .as_node()
        .and_then(|node| node.get("translate"))
        .and_then(Value::as_node)
        .cloned()
        .expect("the manifest carries a translate object")
}

fn text<'a>(translate: &'a Node, key: &str) -> Option<&'a str> {
    translate.get(key).and_then(Value::as_str)
}

#[test]
fn the_manifest_the_crate_embeds_is_the_repositorys() {
    let parts = tabnas_css::translate().expect("CSS carries translation parts");
    assert_eq!(
        parts.manifest,
        read("tabnas.plugin.json"),
        "rs/translate/manifest.json is not tabnas.plugin.json: run npm run embed in ts"
    );
}

#[test]
fn the_render_the_manifest_names_is_the_one_the_crate_embeds() {
    let translate = translate_object();
    let path = text(&translate, "render").expect("translate.render names a file");
    let parts = tabnas_css::translate().expect("CSS carries translation parts");
    let render = parts.render.expect("CSS carries a render");
    assert_eq!(render.entry, "css-render");
    assert_eq!(
        render.source,
        Some(read(path).as_str()),
        "translate.render names {path}, and the crate embeds another text: run npm run embed in ts"
    );
}

/// An embed takes a plain tree into a format's own schema. CSS's tree is
/// the reader's stylesheet, and a plain tree has no CSS form, so its
/// manifest names none and the crate carries none; a manifest that named
/// one would be held to its file here, as the render is above.
#[test]
fn the_embed_the_manifest_names_is_the_one_the_crate_embeds() {
    let translate = translate_object();
    let parts = tabnas_css::translate().expect("CSS carries translation parts");
    let Some(path) = text(&translate, "embed") else {
        assert_eq!(
            parts.embed, None,
            "the manifest names no embed, and the crate carries one"
        );
        return;
    };
    let embed = parts
        .embed
        .unwrap_or_else(|| panic!("translate.embed names {path}, and the crate carries no embed"));
    assert_eq!(embed.entry, "css-embed");
    assert_eq!(
        embed.source,
        Some(read(path).as_str()),
        "translate.embed names {path}, and the crate embeds another text: run npm run embed in ts"
    );
}

/// CSS is read as a tree and written from one, and the tree is the
/// reader's own: the schema names it, its root is the stylesheet, an
/// object, and there is no lift, since the events carry the tree already.
#[test]
fn css_reads_and_writes_its_own_tree() {
    let manifest = json(&read("tabnas.plugin.json")).expect("the manifest is JSON");
    assert_eq!(
        manifest
            .as_node()
            .and_then(|node| node.get("languageId"))
            .and_then(Value::as_str),
        Some("css")
    );
    let translate = translate_object();
    assert_eq!(text(&translate, "reads"), Some("tree"));
    assert_eq!(text(&translate, "writes"), Some("tree"));
    assert_eq!(text(&translate, "root"), Some("object"));
    assert_eq!(text(&translate, "schema"), Some("css-ast"));
    assert!(translate.get("lift").is_none());
    let parts = tabnas_css::translate().expect("CSS carries translation parts");
    assert_eq!(parts.lift, None);
}

/// The host prints the loss lines verbatim, so each is a sentence.
#[test]
fn the_loss_is_a_list_of_sentences() {
    let translate = translate_object();
    let loss = translate
        .get("loss")
        .and_then(Value::as_list)
        .expect("translate.loss is a list");
    assert!(!loss.is_empty());
    for line in loss {
        let line = line.as_str().expect("each loss line is a string");
        assert!(
            line.starts_with(char::is_uppercase) && line.ends_with('.'),
            "{line:?} is not a sentence"
        );
    }
}

/// A host links the render with its own program and other formats'
/// parts, so every definition is named for CSS, the entry point is
/// `css-render`, and the file defines no `export` of its own.
#[test]
fn the_render_is_a_library_named_for_css() {
    let render = tabnas_css::translate()
        .and_then(|parts| parts.render)
        .and_then(|render| render.source)
        .expect("CSS carries the render's text");
    let names: Vec<&str> = render
        .lines()
        .filter_map(|line| line.strip_prefix("def "))
        .filter_map(|rest| rest.split_whitespace().next())
        .collect();
    assert!(names.contains(&"css-render"), "{names:?}");
    for name in &names {
        assert!(name.starts_with("css-"), "{name} is not named for CSS");
    }
}
