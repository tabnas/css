/* Copyright (c) 2025 Richard Rodger, MIT License */

//! The plugin: installing the grammar on an engine, and the actions that
//! build the AST from what it matches.
//!
//! `makeActions` in `ts/src/css.ts` is the canonical form. Its node
//! constructors REBIND `r.node`, and its setters and pushers mutate the node
//! object a rule inherited from its parent by reference. The engine here
//! shares one node CELL between a rule and the rules it pushes or replaces
//! into, so the constructors install a fresh cell rather than write into the
//! one they were handed, which would overwrite the parent's node; the setters
//! and pushers write through the shared cell, as the canonical ones do
//! through the shared object.
//!
//! A node is kept in one of two stores, chosen per parse:
//!
//! - **The tree**, for a parse through the engine's own API
//!   ([`make`](crate::make), [`plugin`](crate::plugin),
//!   [`Css::tabnas`](crate::Css::tabnas)): each cell holds the node itself,
//!   an engine object, and a child is moved into its parent's list when it is
//!   complete. The engine returns the stylesheet, and with recovery on it
//!   returns the partial stylesheet, as the canonical port does. The engine's
//!   value drops, copies and prints by recursion, one frame per level, so this
//!   form is bounded at [`TREE_RULE_DEPTH`] open rules by a parse guard.
//! - **The arena**, for [`Css::parse`](crate::Css::parse), which asks for it
//!   with the [`ARENA`] meta key: every node is one flat record in a
//!   per-parse list in `ctx.u`, a cell holds the record's id, and a child list
//!   holds ids. The engine never holds a nested value, so no depth reaches a
//!   recursion; [`Value::from_arena`](crate::Value) builds the crate's own
//!   tree from the list without one, and this form has no depth limit.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use tabnas::{ActionError, Context, GrammarSetting, PluginError, Rule, Tabnas, Token, Value};

use crate::grammar::{specs, RULES};
use crate::lex::{
    self, col_width, es_is_whitespace, es_trim, split_selectors, vendor_prefix, Tins, OVERSHOOT,
};
use crate::Options;

/// The most rules the tree form lets be open at once.
///
/// 768 frames: 191 nested style rules (each takes four: `decls`, `decl`,
/// `sel`, `declbody`) or 256 nested `@media` blocks (three: `items`,
/// `statement`, `rulesbody`). A tree at the limit survives being dropped,
/// cloned, compared, printed and written as JSON on a 2 MiB thread, the
/// smallest stack a spawned thread gets by default, in a debug build. A
/// deeper document fails with `cancel`. [`Css::parse`](crate::Css::parse)
/// has no such limit, and neither does the canonical port: this is a
/// Rust-only refusal, registered in `test/divergent.tsv`.
pub const TREE_RULE_DEPTH: usize = 768;

/// The meta key that asks for the arena store. [`Css::parse`](crate::Css::parse)
/// passes it; a parse without it builds the tree.
pub(crate) const ARENA: &str = "tabnas-css/arena";

/// The arena: the `ctx.u` key holding the per-parse list of node records.
const NODES: &str = "tabnas-css/nodes";

/// The `ctx.u` key holding the scalar positions of the source's astral
/// characters, built when the first position is recorded.
const ASTRAL: &str = "tabnas-css/astral";

/// Whether this parse builds into the arena.
fn arena(ctx: &Context) -> bool {
    matches!(&ctx.meta, Value::Object(meta) if meta.contains_key(ARENA))
}

fn object() -> Value {
    Value::object(Default::default())
}

fn list() -> Value {
    Value::array(Vec::new())
}

fn text(s: &str) -> Value {
    Value::String(s.to_string())
}

/// Set `key`, keeping its place when it is already present, as assigning a
/// JavaScript object property does.
fn put(target: &mut Value, key: &str, value: Value) {
    if let Some(fields) = target.as_object_mut() {
        fields.insert(key.to_string(), value);
    }
}

/// Append to the list at `field`.
fn push_to(target: &mut Value, field: &str, item: Value) {
    let Some(fields) = target.as_object_mut() else {
        return;
    };
    match fields.get_mut(field) {
        Some(Value::Array(items)) => Arc::make_mut(items).push(item),
        _ => {
            fields.insert(field.to_string(), Value::array(vec![item]));
        }
    }
}

/// The arena's list of records.
fn nodes(ctx: &mut Context) -> &mut Vec<Value> {
    let slot = ctx.u.entry(NODES.to_string()).or_insert_with(list);
    if slot.as_array_mut().is_none() {
        *slot = list();
    }
    slot.as_array_mut().expect("the arena was just made a list")
}

/// Make `record` this rule's node, in a FRESH cell. The cell a rule starts
/// with is its parent's (a push or a replace shares it), and the canonical
/// constructor's `r.node = …` rebinds the child's name without touching the
/// parent's object; writing into the shared cell would overwrite the parent.
fn install_node(rule: &mut Rule, ctx: &mut Context, record: Value) {
    let node = if arena(ctx) {
        let records = nodes(ctx);
        let id = records.len();
        records.push(record);
        Value::Number(id as f64)
    } else {
        record
    };
    rule.node = Rc::new(RefCell::new(node));
}

/// Edit this rule's node where it lives: in the arena, or in the cell.
fn with_node(rule: &Rule, ctx: &mut Context, edit: impl FnOnce(&mut Value)) {
    if arena(ctx) {
        let id = match *rule.node.borrow() {
            Value::Number(id) => id as usize,
            _ => return,
        };
        if let Some(record) = nodes(ctx).get_mut(id) {
            edit(record);
        }
    } else {
        edit(&mut rule.node.borrow_mut());
    }
}

// ---------------------------------------------------------------------------
// Positions.

/// The column of scalar `pos`, in UTF-16 code units, from the engine's
/// column `ci` there.
///
/// The engine counts a column in Unicode scalars since the last reset (a
/// `\n` in any token, or the end of a run of line characters); the canonical
/// port counts UTF-16 code units since the same points. The difference is the
/// astral characters in between, looked up in a sorted list built once per
/// parse, so the cost per position is a binary search, and nothing at all for
/// a source with no astral character.
fn col16(ctx: &mut Context, pos: usize, ci: usize) -> usize {
    if !ctx.u.contains_key(ASTRAL) {
        let wide: Vec<Value> = if ctx.source.bytes().any(|b| 0xF0 <= b) {
            ctx.source
                .chars()
                .enumerate()
                .filter(|(_, c)| 2 == c.len_utf16())
                .map(|(i, _)| Value::Number(i as f64))
                .collect()
        } else {
            Vec::new()
        };
        ctx.u.insert(ASTRAL.to_string(), Value::array(wide));
    }
    let Some(Value::Array(wide)) = ctx.u.get(ASTRAL) else {
        return ci;
    };
    if wide.is_empty() {
        return ci;
    }
    let at = |v: &Value| match v {
        Value::Number(n) => *n as usize,
        _ => 0,
    };
    // The end-of-source token's column carries the scanners' overshoot, so
    // this window can start before the reset; the scalar there is a reset
    // character or before the source, never astral.
    let from = pos.saturating_sub(ci.saturating_sub(1));
    let low = wide.partition_point(|v| at(v) < from);
    let high = wide.partition_point(|v| at(v) < pos);
    ci + (high - low)
}

fn point(line: usize, column: usize) -> Value {
    let mut at = object();
    put(&mut at, "line", Value::Number(line as f64));
    put(&mut at, "column", Value::Number(column as f64));
    at
}

/// A token's first character: `startPos` in the canonical port.
fn start_pos(ctx: &mut Context, token: &Token) -> Value {
    let column = col16(ctx, token.site.pos, token.site.ci);
    point(token.site.ri, column)
}

/// Just after a token's last character: `endPos` in the canonical port,
/// measured over the token's source text.
fn end_pos(ctx: &mut Context, token: &Token) -> Value {
    let src = token.src.as_str();
    match src.rfind('\n') {
        Some(last) => point(
            token.site.ri + src.matches('\n').count(),
            1 + col_width(&src[last + 1..]),
        ),
        None => {
            let column = col16(ctx, token.site.pos, token.site.ci);
            point(token.site.ri, column + col_width(src))
        }
    }
}

/// `withPos` in the canonical port: a `position` for a node built from the
/// rule's first matched token, with its end when `end` is set. An end not
/// recorded here is absent until an action records it.
fn with_pos(rule: &Rule, ctx: &mut Context, position: bool, end: bool) -> Option<Value> {
    if !position {
        return None;
    }
    let token = rule.o0()?;
    let mut at = object();
    put(&mut at, "start", start_pos(ctx, token));
    if end {
        put(&mut at, "end", end_pos(ctx, token));
    }
    Some(at)
}

/// Record the end of this rule's node, if it has a position.
fn set_end(rule: &Rule, ctx: &mut Context, end: Value) {
    with_node(rule, ctx, |record| {
        if let Some(at) = record.as_object_mut().and_then(|f| f.get_mut("position")) {
            put(at, "end", end);
        }
    });
}

// ---------------------------------------------------------------------------
// Node builders: `makeAtRules`, `makeAtDecls`, `makeKeyframes` and
// `makeAtStmt` in the canonical port. Key order is JavaScript's: a key
// assigned twice keeps its first place.

fn token_text(token: &Token) -> String {
    match &token.val {
        Value::String(s) => s.clone(),
        _ => String::new(),
    }
}

fn use_text(token: &Token, key: &str) -> String {
    match token.use_data().get(key) {
        Some(Value::String(s)) => s.clone(),
        _ => String::new(),
    }
}

/// A block at-rule whose body is a list of rules (`@media`, `@supports`,
/// `@document`, `@host`, and any other with a rules body).
fn make_at_rules(token: &Token) -> Value {
    let kw = token_text(token);
    let prelude = use_text(token, "prelude");
    let mut node = object();
    match kw.as_str() {
        "host" => put(&mut node, "type", text("host")),
        _ if "document" == kw || kw.ends_with("-document") => {
            // `vendor` is always present on a document node, the empty
            // string when the at-keyword carries no prefix.
            put(&mut node, "type", text("document"));
            put(&mut node, "document", text(&prelude));
            put(&mut node, "vendor", text(vendor_prefix(&kw).unwrap_or("")));
        }
        _ => {
            put(&mut node, "type", text(&kw));
            put(&mut node, &kw, text(&prelude));
        }
    }
    put(&mut node, "rules", list());
    node
}

/// A block at-rule whose body is declarations (`@font-face`, `@page`, and
/// any other with a declarations body). A `@page` prelude is a selector
/// group: `@page toc, index:blank`.
fn make_at_decls(token: &Token) -> Value {
    let kw = token_text(token);
    let mut node = object();
    put(&mut node, "type", text(&kw));
    if "page" == kw {
        let selectors = split_selectors(&use_text(token, "prelude"))
            .into_iter()
            .map(Value::String)
            .collect();
        put(&mut node, "selectors", Value::array(selectors));
    }
    put(&mut node, "declarations", list());
    node
}

/// A `@keyframes` block, possibly vendor-prefixed.
fn make_keyframes(token: &Token) -> Value {
    let kw = token_text(token);
    let mut node = object();
    put(&mut node, "type", text("keyframes"));
    put(&mut node, "name", text(&use_text(token, "prelude")));
    if let Some(vendor) = vendor_prefix(&kw) {
        put(&mut node, "vendor", text(vendor));
    }
    put(&mut node, "keyframes", list());
    node
}

/// A statement at-rule (`@import`, `@charset`, `@namespace`, …): the
/// at-keyword is the node type and the field carrying its params.
///
/// `@custom-media --name <query>` splits its params into a name and a media
/// query, as `/^(--\S+)\s*([\s\S]*)$/` does in the canonical port: `--` and
/// at least one non-space character, then the rest, trimmed.
fn make_at_stmt(token: &Token) -> Value {
    let kw = token_text(token);
    let params = use_text(token, "params");
    let mut node = object();
    if "custom-media" == kw && params.starts_with("--") {
        let name_end = params.find(es_is_whitespace).unwrap_or(params.len());
        if 2 < name_end {
            put(&mut node, "type", text("custom-media"));
            put(&mut node, "name", text(&params[..name_end]));
            put(&mut node, "media", text(es_trim(&params[name_end..])));
            return node;
        }
    }
    put(&mut node, "type", text(&kw));
    put(&mut node, &kw, text(&params));
    node
}

// ---------------------------------------------------------------------------
// The actions.

type Outcome = Result<(), ActionError>;

/// The text of the rule's first matched token, which the engine keeps after
/// a `b: 1` backtrack.
fn o0_text(rule: &Rule) -> String {
    rule.o0().map(token_text).unwrap_or_default()
}

/// Append the completed child's node to this rule's node's `field`. A child
/// that built nothing is skipped, as `undefined !== c` skips it in the
/// canonical port.
fn push_child(rule: &Rule, ctx: &mut Context, field: &str) {
    if !rule.has_child_value() {
        return;
    }
    let child = rule.child_value();
    with_node(rule, ctx, |record| push_to(record, field, child));
}

/// Register every action the grammar names, capturing the options each
/// needs. Registration replaces by name, so a second install with other
/// options takes effect.
fn register_actions(parser: &mut Tabnas, position: bool) {
    parser.action_with_context("@cssSheet", move |rule, ctx| -> Outcome {
        let mut node = object();
        put(&mut node, "type", text("stylesheet"));
        put(&mut node, "rules", list());
        if position {
            let mut at = object();
            put(&mut at, "start", point(1, 1));
            put(&mut node, "position", at);
        }
        install_node(rule, ctx, node);
        Ok(())
    });

    // The constructors of nodes built from the rule's first token: a start
    // position always, and an end only for the single-token nodes.
    type Build = fn(&Rule) -> Value;
    let simple: [(&str, Build, bool); 4] = [
        (
            "@cssRule",
            |_| {
                let mut node = object();
                put(&mut node, "type", text("rule"));
                put(&mut node, "selectors", list());
                put(&mut node, "declarations", list());
                node
            },
            false,
        ),
        (
            "@cssDecl",
            |rule| {
                let mut node = object();
                put(&mut node, "type", text("declaration"));
                put(&mut node, "property", Value::String(o0_text(rule)));
                put(&mut node, "value", text(""));
                node
            },
            false,
        ),
        (
            "@cssComment",
            |rule| {
                let mut node = object();
                put(&mut node, "type", text("comment"));
                put(&mut node, "comment", Value::String(o0_text(rule)));
                node
            },
            true,
        ),
        (
            "@cssKeyframe",
            |_| {
                let mut node = object();
                put(&mut node, "type", text("keyframe"));
                put(&mut node, "values", list());
                put(&mut node, "declarations", list());
                node
            },
            false,
        ),
    ];
    for (name, build, end) in simple {
        parser.action_with_context(name, move |rule, ctx| -> Outcome {
            let mut node = build(rule);
            if let Some(at) = with_pos(rule, ctx, position, end) {
                put(&mut node, "position", at);
            }
            install_node(rule, ctx, node);
            Ok(())
        });
    }

    type AtBuild = fn(&Token) -> Value;
    let at_rules: [(&str, AtBuild, bool); 4] = [
        ("@cssAtRules", make_at_rules, false),
        ("@cssAtDecls", make_at_decls, false),
        ("@cssKeyframes", make_keyframes, false),
        ("@cssAtStmt", make_at_stmt, true),
    ];
    for (name, build, end) in at_rules {
        parser.action_with_context(name, move |rule, ctx| -> Outcome {
            let mut node = rule.o0().map(build).unwrap_or_else(object);
            if let Some(at) = with_pos(rule, ctx, position, end) {
                put(&mut node, "position", at);
            }
            install_node(rule, ctx, node);
            Ok(())
        });
    }

    // Field setters: they change the node the rule inherited.
    for (name, field) in [("@cssSelector", "selectors"), ("@cssKfValue", "values")] {
        parser.action_with_context(name, move |rule, ctx| -> Outcome {
            let value = Value::String(o0_text(rule));
            with_node(rule, ctx, |record| push_to(record, field, value));
            Ok(())
        });
    }
    parser.action_with_context("@cssDeclVal", move |rule, ctx| -> Outcome {
        let value = Value::String(o0_text(rule));
        with_node(rule, ctx, |record| put(record, "value", value));
        if position {
            if let Some(token) = rule.o0() {
                let end = end_pos(ctx, token);
                set_end(rule, ctx, end);
            }
        }
        Ok(())
    });

    // The closing brace, or the end of the source for the stylesheet: the
    // end of the node the block belongs to. A close phase, so the matched
    // token is the rule's first CLOSE token.
    parser.action_with_context("@cssEnd", move |rule, ctx| -> Outcome {
        if position {
            if let Some(token) = rule.c0() {
                let end = end_pos(ctx, token);
                set_end(rule, ctx, end);
            }
        }
        Ok(())
    });

    // List pushers: a completed child node into its parent's list.
    for (name, field) in [
        ("@cssPushRule", "rules"),
        ("@cssPushDecl", "declarations"),
        ("@cssPushKf", "keyframes"),
    ] {
        parser.action_with_context(name, move |rule, ctx| -> Outcome {
            push_child(rule, ctx, field);
            Ok(())
        });
    }

    // After the stylesheet closes, and before the engine takes the root
    // rule's node as the result: an arena parse hands back the whole list,
    // for `Value::from_arena` to assemble. A tree parse's node is already the
    // stylesheet.
    parser.state_action_ref("@stylesheet-ac", |rule, ctx| -> Outcome {
        if arena(ctx) {
            let records = ctx.u.shift_remove(NODES).unwrap_or_else(list);
            rule.node = Rc::new(RefCell::new(records));
        }
        Ok(())
    });
}

/// Install the plugin on `parser`. See [`plugin`](crate::plugin).
///
/// Safe to run more than once on one instance, and it is: `use_plugin` runs
/// it again when the same plugin is used again with other options, and
/// [`Tabnas::derive`] runs it on every derived instance. A second run
/// replaces the matcher and the actions (which capture the options) and
/// installs the rules afresh; the lex subscriber, which captures nothing, is
/// added only by the first, since subscribers are not named and would
/// otherwise run twice.
pub(crate) fn install(parser: &mut Tabnas, options: &Options) -> Result<(), PluginError> {
    let specs = specs().map_err(PluginError)?;
    let first = parser
        .rule_specs()
        .iter()
        .all(|spec| "stylesheet" != spec.name);

    let tins = Tins {
        cc: parser.token("#CC"),
        atr: parser.token("#ATR"),
        atd: parser.token("#ATD"),
        atk: parser.token("#ATK"),
        ats: parser.token("#ATS"),
        gc: parser.token("#GC"),
    };
    let lower = options.lowercase_properties;
    parser.imperative_lex_match_ref("@css-token", move |lexer, rule, ctx| {
        lex::css_token(lexer, rule, ctx, lower, tins)
    });
    register_actions(parser, options.position);

    // The plugin's per-parse state starts empty on every parse, whatever
    // the caller seeded `ctx.u` with.
    parser.parse_prepare_ref("@css-prepare", |ctx| {
        ctx.u.shift_remove(NODES);
        ctx.u.shift_remove(ASTRAL);
        ctx.u.shift_remove(OVERSHOOT);
    });
    if first {
        parser.subscribe_lex(lex::lex_subscriber);
    }

    // Named `depth`, the name jsonic's own guard has, so this one replaces
    // it: jsonic's counts jsonic's containers, and none is reachable here.
    // The count comes first because it is the cheap test.
    parser.parse_guard("depth", |ctx| {
        ctx.rule_stack.len() <= TREE_RULE_DEPTH || arena(ctx)
    });

    if !first {
        for name in RULES {
            parser.remove_rule(name);
        }
    }
    parser
        .grammar(&specs.options)
        .map_err(|e| PluginError(e.0))?;
    parser
        .grammar_with_setting(&specs.rules, &GrammarSetting::groups("css"))
        .map_err(|e| PluginError(e.0))?;
    Ok(())
}
