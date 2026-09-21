/* Copyright (c) 2025 Richard Rodger, MIT License */

//! The rule machine: it runs the rule table read out of
//! [`grammar`](crate::grammar) over the tokens [`lex`] produces,
//! and the grammar-local actions build the AST as it goes.
//!
//! The canonical TypeScript port and the Go port hand this job to the tabnas
//! engine. There is no Rust engine, so this module is the one piece of the
//! port with no line-for-line counterpart — but it is not a new design: it
//! implements the same rule/alt contract those alts are written against, so
//! `css-grammar.jsonic` stays the single source of truth for all three ports.
//!
//! A rule runs in two phases. In the OPEN phase its `open` alts are tried in
//! order; in the CLOSE phase its `close` alts are. An alt matches when the
//! next tokens are its `s` sequence (no `s` matches unconditionally), and then
//!
//! - its `a` actions run, against this rule's matched tokens and node;
//! - `b` of the matched tokens are pushed back — matched and recorded, but
//!   left in the buffer for whatever reads next;
//! - `p` pushes a child rule (this rule moves to its CLOSE phase and resumes
//!   when the child pops), `r` replaces this rule with a fresh one, and
//!   neither means: open → close, or close → pop.
//!
//! Tokens are lexed LAZILY — only as many as the alt being tried needs, under
//! the rule trying it. That is what decides whether a comment becomes a node,
//! so it is behaviour, not an optimisation: see [`crate::lex`].
//!
//! ## Node ownership
//!
//! In the canonical ports a child rule inherits its parent's node BY
//! REFERENCE, so a selector pushed three rules down lands in the parent's
//! node. Rust has no such aliasing without interior mutability, so the node
//! MOVES down the stack instead: a pushed child takes the node, a constructor
//! action hands it back before installing its own, and a popping rule either
//! returns it or delivers its own node to the parent as `child`. Only the top
//! frame ever runs, so the node is always where the running rule is.

use crate::grammar::{Alt, Grammar, RuleDef};
use crate::lex::{self, Error, Lex, Tin, Token};
use crate::value::{Node, Value};

/// Plugin options. Mirrors the TypeScript `CssOptions` and the Go
/// `CssOptions`, and the defaults match `Css.defaults` / `Defaults`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// Lowercase declaration property names (CSS property names are
    /// case-insensitive). Selectors, values and at-rule preludes are
    /// untouched. Off by default.
    pub lowercase_properties: bool,
    /// Attach a `position: { start: { line, column }, end: { … } }` (1-based)
    /// to every node. Off by default — positions add noise.
    pub position: bool,
}

/// Which phase a rule frame is in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    Open,
    Close,
}

/// One rule on the stack.
struct Frame<'g> {
    /// The grammar rule this frame is running.
    ///
    /// Borrowed from the grammar, so pushing a rule costs no allocation —
    /// the machine takes one step per token and then some.
    name: &'g str,
    /// Open or close.
    phase: Phase,
    /// The node being built. `None` while a child frame holds it.
    node: Option<Node>,
    /// Whether `node` belongs to the parent frame and must be handed back.
    inherited: bool,
    /// Tokens matched by the open alt (`r.o` in the canonical ports).
    o: Vec<Token>,
    /// Tokens matched by the close alt (`r.c`).
    c: Vec<Token>,
    /// The node the last child rule built (`r.child.node`).
    child: Option<Node>,
}

impl<'g> Frame<'g> {
    fn new(name: &'g str, node: Option<Node>, inherited: bool) -> Frame<'g> {
        Frame {
            name,
            phase: Phase::Open,
            node,
            inherited,
            o: Vec::new(),
            c: Vec::new(),
            child: None,
        }
    }
}

/// How many rule steps may pass without a token being consumed before the
/// machine calls it a loop.
///
/// A budget counted in total steps would have to grow with the source and
/// could still misfire on a large valid stylesheet. Non-progress cannot: every
/// loop this grammar can make — `items` → `statement` → `items`, `decls` →
/// `decl` → `decls` — consumes at least one token per turn, and the longest
/// legitimate no-consume chain is a handful of frames.
const MAX_IDLE_STEPS: usize = 64;

/// Run the grammar over `src`.
pub fn parse(grammar: &Grammar, src: &str, options: Options) -> Result<Value, Error> {
    // The engine short-circuits an exactly-empty source before the rule loop
    // runs, so the start rule never builds its node. reworkcss returns an
    // empty stylesheet for "", so declare that here rather than expect the
    // grammar to produce it — the canonical ports do the same, via
    // `lex.emptyResult`.
    if src.is_empty() {
        let mut sheet = Node::new();
        sheet.set("type", "stylesheet");
        sheet.set("rules", Value::List(Vec::new()));
        return Ok(Value::Node(sheet));
    }

    Machine {
        grammar,
        lex: Lex::new(src, options.lowercase_properties),
        options,
        stack: vec![Frame::new("stylesheet", None, false)],
        idle: 0,
    }
    .run()
}

struct Machine<'g> {
    grammar: &'g Grammar,
    lex: Lex,
    options: Options,
    stack: Vec<Frame<'g>>,
    idle: usize,
}

impl<'g> Machine<'g> {
    fn run(mut self) -> Result<Value, Error> {
        loop {
            let top = self.stack.len() - 1;
            let name: &'g str = self.stack[top].name;
            let phase = self.stack[top].phase;

            let grammar: &'g Grammar = self.grammar;
            let def: &'g RuleDef = grammar.rule(name).ok_or_else(|| Error {
                code: "unexpected".to_string(),
                message: format!("no grammar rule named '{name}'"),
                line: self.lex.point().r_i,
                column: self.lex.point().c_i,
            })?;
            let alts: &'g [Alt] = match phase {
                Phase::Open => &def.open,
                Phase::Close => &def.close,
            };

            // Try the alts in order, lexing only what each one needs — and,
            // within an alt, only as far as it still matches.
            //
            // The second half is behaviour, not thrift. `b{,/*!important`
            // reaches `decl` with a `#GC` in hand: its `#TX #CL` alt fails on
            // the FIRST token, so the unterminated comment behind it is never
            // read and the parse fails as `unexpected`. Lexing the whole `s`
            // up front would read it and fail as `unterminated_comment`
            // instead — a different error code for the same document, and
            // the fixtures compare codes exactly.
            let mut chosen: Option<&'g Alt> = None;
            for alt in alts {
                let mut matched = true;
                for (i, want) in alt.s.iter().enumerate() {
                    let seen = self.lex.peek(i + 1, name)?;
                    if want != seen[i].tin.name() {
                        matched = false;
                        break;
                    }
                }
                if matched {
                    chosen = Some(alt);
                    break;
                }
            }
            let Some(alt) = chosen else {
                return Err(self.no_alt(name, phase));
            };

            // Record the matched tokens, then consume all but the `b` pushed
            // back. `b` with no `s` is a no-op, as it is in the canonical
            // ports: there are no matched tokens to push back.
            let matched: Vec<Token> = self.lex.peek(alt.s.len(), name)?.to_vec();
            let consume = alt.s.len().saturating_sub(alt.b);
            self.lex.consume(consume);
            if 0 < consume {
                self.idle = 0;
            } else {
                self.idle += 1;
                if MAX_IDLE_STEPS < self.idle {
                    return Err(self.stuck(name));
                }
            }
            match phase {
                Phase::Open => self.stack[top].o = matched,
                Phase::Close => self.stack[top].c = matched,
            }

            for action in &alt.a {
                self.act(action);
            }

            if let Some(child) = &alt.p {
                // Descend: the child takes this frame's node, and this frame
                // resumes in its close phase once the child pops.
                let node = self.stack[top].node.take();
                let inherited = node.is_some();
                self.stack[top].phase = Phase::Close;
                let child = Frame::new(child, node, inherited);
                self.stack.push(child);
            } else if let Some(replacement) = &alt.r {
                // Replace: a fresh rule in this frame's place, carrying the
                // node and the parent relationship but no child.
                let node = self.stack[top].node.take();
                let inherited = self.stack[top].inherited;
                self.stack[top] = Frame::new(replacement, node, inherited);
            } else if Phase::Open == phase {
                self.stack[top].phase = Phase::Close;
            } else {
                // Pop. A frame that merely borrowed its parent's node hands
                // it straight back; one that built its own delivers it as the
                // parent's `child`, which is what the pushers append.
                let done = self.stack.pop().expect("stack is never empty here");
                let Some(parent) = self.stack.last_mut() else {
                    return Ok(done.node.map(Value::Node).unwrap_or(Value::Undefined));
                };
                if done.inherited {
                    parent.node = done.node;
                } else {
                    parent.child = done.node;
                }
            }
        }
    }

    /// No alt matched: report the token that stopped the parse.
    fn no_alt(&mut self, name: &str, phase: Phase) -> Error {
        let point = self.lex.point();
        let (line, column, what) = match self.lex.peek(1, name) {
            Ok(seen) => {
                let tok = &seen[0];
                let what = if Tin::Zz == tok.tin {
                    "end of input".to_string()
                } else {
                    format!("{:?}", tok.val)
                };
                (tok.r_i, tok.c_i, what)
            }
            // The lookahead itself failed; that error is the real one.
            Err(err) => return err,
        };
        Error {
            code: "unexpected".to_string(),
            message: format!(
                "unexpected character(s): {what} (no '{name}' {} alternative matches)",
                if Phase::Open == phase {
                    "open"
                } else {
                    "close"
                }
            ),
            line: if 0 < line { line } else { point.r_i },
            column: if 0 < column { column } else { point.c_i },
        }
    }

    /// The machine made no progress for [`MAX_IDLE_STEPS`] steps.
    fn stuck(&self, name: &str) -> Error {
        let point = self.lex.point();
        Error {
            code: "unexpected".to_string(),
            message: format!(
                "the grammar made no progress at rule '{name}' after \
                 {MAX_IDLE_STEPS} steps without consuming a token"
            ),
            line: point.r_i,
            column: point.c_i,
        }
    }

    // --- Grammar actions: build the AST ------------------------------------

    /// Run one `@cssXxx` action against the top frame.
    ///
    /// The constructors overwrite the frame's node; the field setters mutate
    /// it; the pushers append a finished child node to one of its arrays.
    /// An unknown reference is ignored, as an unresolved `@ref` is in the
    /// canonical ports.
    fn act(&mut self, action: &str) {
        match action {
            // Node constructors.
            "@cssSheet" => {
                let mut node = Node::new();
                node.set("type", "stylesheet");
                node.set("rules", Value::List(Vec::new()));
                if self.options.position {
                    node.set("position", position(Some((1, 1)), None));
                }
                self.set_node(node);
            }
            "@cssRule" => {
                let mut node = Node::new();
                node.set("type", "rule");
                node.set("selectors", Value::List(Vec::new()));
                node.set("declarations", Value::List(Vec::new()));
                self.with_pos(node, false);
            }
            "@cssDecl" => {
                let property = self.open_val();
                let mut node = Node::new();
                node.set("type", "declaration");
                node.set("property", property);
                node.set("value", "");
                self.with_pos(node, false);
            }
            "@cssComment" => {
                let comment = self.open_val();
                let mut node = Node::new();
                node.set("type", "comment");
                node.set("comment", comment);
                self.with_pos(node, true);
            }
            "@cssKeyframe" => {
                let mut node = Node::new();
                node.set("type", "keyframe");
                node.set("values", Value::List(Vec::new()));
                node.set("declarations", Value::List(Vec::new()));
                self.with_pos(node, false);
            }
            "@cssAtRules" => {
                let node = self.open_token().map(make_at_rules).unwrap_or_default();
                self.with_pos(node, false);
            }
            "@cssAtDecls" => {
                let node = self.open_token().map(make_at_decls).unwrap_or_default();
                self.with_pos(node, false);
            }
            "@cssKeyframes" => {
                let node = self.open_token().map(make_keyframes).unwrap_or_default();
                self.with_pos(node, false);
            }
            "@cssAtStmt" => {
                let node = self.open_token().map(make_at_stmt).unwrap_or_default();
                self.with_pos(node, true);
            }

            // Field setters (mutate the node this rule inherited).
            "@cssSelector" => {
                let v = self.open_val();
                if let Some(node) = self.node_mut() {
                    node.push_to("selectors", Value::Str(v));
                }
            }
            "@cssKfValue" => {
                let v = self.open_val();
                if let Some(node) = self.node_mut() {
                    node.push_to("values", Value::Str(v));
                }
            }
            "@cssDeclVal" => {
                let v = self.open_val();
                let end = self
                    .options
                    .position
                    .then(|| self.open_token().map(lex::end_pos))
                    .flatten();
                if let Some(node) = self.node_mut() {
                    node.set("value", Value::Str(v));
                    if let Some(end) = end {
                        set_end(node, end);
                    }
                }
            }

            // The closing-brace / end-of-input end position, recorded on the
            // node the block belongs to. Runs in a close phase, so the
            // matched `}`/end token is the frame's first close token.
            "@cssEnd" => {
                if !self.options.position {
                    return;
                }
                let end = self.close_token().map(lex::end_pos);
                if let (Some(end), Some(node)) = (end, self.node_mut()) {
                    set_end(node, end);
                }
            }

            // Array pushers.
            "@cssPushRule" => self.push_child("rules"),
            "@cssPushDecl" => self.push_child("declarations"),
            "@cssPushKf" => self.push_child("keyframes"),

            _ => {}
        }
    }

    fn top(&mut self) -> &mut Frame<'g> {
        self.stack.last_mut().expect("stack is never empty")
    }

    fn open_token(&self) -> Option<&Token> {
        self.stack.last().and_then(|f| f.o.first())
    }

    fn close_token(&self) -> Option<&Token> {
        self.stack.last().and_then(|f| f.c.first())
    }

    /// The value of the open alt's first matched token (`r.o[0].val`).
    fn open_val(&self) -> String {
        self.open_token().map(|t| t.val.clone()).unwrap_or_default()
    }

    fn node_mut(&mut self) -> Option<&mut Node> {
        self.top().node.as_mut()
    }

    /// Install a freshly built node, handing any inherited one back to the
    /// parent first (the canonical ports simply reassign `r.node`, which
    /// drops this rule's reference and leaves the parent's intact).
    fn set_node(&mut self, node: Node) {
        let top = self.stack.len() - 1;
        if self.stack[top].inherited {
            let borrowed = self.stack[top].node.take();
            self.stack[top].inherited = false;
            if 0 < top {
                self.stack[top - 1].node = borrowed;
            }
        }
        self.stack[top].node = Some(node);
    }

    /// Install a node, recording its start position from the open token —
    /// and its end too when `end` is set, for a node that is one token wide.
    fn with_pos(&mut self, mut node: Node, end: bool) {
        if self.options.position {
            if let Some(tok) = self.open_token() {
                let start = lex::start_pos(tok);
                let stop = end.then(|| lex::end_pos(tok));
                node.set("position", position(Some(start), stop));
            }
        }
        self.set_node(node);
    }

    /// Append the child node this frame's last child built.
    fn push_child(&mut self, field: &str) {
        let top = self.stack.len() - 1;
        let Some(child) = self.stack[top].child.take() else {
            return;
        };
        if let Some(node) = self.stack[top].node.as_mut() {
            node.push_to(field, Value::Node(child));
        }
    }
}

/// A `position` value. `end` is [`Value::Undefined`] when unknown: the key
/// exists, in insertion order, but does not serialise — which is what the
/// canonical port's `end: undefined` does.
fn position(start: Option<(usize, usize)>, end: Option<(usize, usize)>) -> Value {
    let mut pos = Node::new();
    if let Some((line, column)) = start {
        pos.set("start", point(line, column));
    }
    match end {
        Some((line, column)) => pos.set("end", point(line, column)),
        None => pos.set("end", Value::Undefined),
    }
    Value::Node(pos)
}

fn point(line: usize, column: usize) -> Value {
    let mut p = Node::new();
    p.set("line", Value::Num(line as f64));
    p.set("column", Value::Num(column as f64));
    Value::Node(p)
}

/// Record `end` on a node's existing position, leaving a node without one
/// alone (positions are off by default, and then there is nothing to record).
fn set_end(node: &mut Node, (line, column): (usize, usize)) {
    if let Some(Value::Node(pos)) = node.get_mut("position") {
        pos.set("end", point(line, column));
    }
}

/// Build a block at-rule node whose body is a list of rules (`@media`,
/// `@supports`, `@document`, `@host`, and generic block at-rules).
fn make_at_rules(tok: &Token) -> Node {
    let kw = tok.val.as_str();
    let prelude = tok.use_text.as_str();
    let mut node = Node::new();
    match kw {
        "media" => {
            node.set("type", "media");
            node.set("media", prelude);
        }
        "supports" => {
            node.set("type", "supports");
            node.set("supports", prelude);
        }
        "host" => {
            node.set("type", "host");
        }
        _ if "document" == kw || kw.ends_with("-document") => {
            // `vendor` is always present on a document node (the empty string
            // when the at-keyword carries no prefix), as in the reworkcss
            // model.
            node.set("type", "document");
            node.set("document", prelude);
            node.set("vendor", lex::vendor_prefix(kw).unwrap_or(""));
        }
        // A generic block at-rule with a rules body (@container, @layer,
        // @scope, …): the keyword is both the node type and the field
        // carrying its prelude.
        _ => {
            node.set("type", kw);
            node.set(kw, prelude);
        }
    }
    node.set("rules", Value::List(Vec::new()));
    node
}

/// Build a block at-rule node whose body is declarations (`@font-face`,
/// `@page`, and generic declaration at-rules).
fn make_at_decls(tok: &Token) -> Node {
    let kw = tok.val.as_str();
    let mut node = Node::new();
    node.set("type", kw);
    if "page" == kw {
        // A @page prelude is a selector group: `@page toc, index:blank`.
        let selectors = lex::split_selectors(&tok.use_text)
            .into_iter()
            .map(Value::Str)
            .collect();
        node.set("selectors", Value::List(selectors));
    }
    node.set("declarations", Value::List(Vec::new()));
    node
}

/// Build a `@keyframes` node, possibly vendor-prefixed.
fn make_keyframes(tok: &Token) -> Node {
    let kw = tok.val.as_str();
    let mut node = Node::new();
    node.set("type", "keyframes");
    node.set("name", tok.use_text.as_str());
    if let Some(vendor) = lex::vendor_prefix(kw) {
        node.set("vendor", vendor);
    }
    node.set("keyframes", Value::List(Vec::new()));
    node
}

/// Build a statement at-rule node (`@import`, `@charset`, `@namespace`, …):
/// the at-keyword is the node type and the field carrying its params.
fn make_at_stmt(tok: &Token) -> Node {
    let kw = tok.val.as_str();
    let params = tok.use_text.as_str();
    let mut node = Node::new();
    // `@custom-media --name <media query>` splits its params into a name and
    // a media query, as in the reworkcss model.
    if "custom-media" == kw {
        if let Some((name, media)) = split_custom_media(params) {
            node.set("type", "custom-media");
            node.set("name", name);
            node.set("media", media);
            return node;
        }
    }
    node.set("type", kw);
    node.set(kw, params);
    node
}

/// `^(--\S+)\s*([\s\S]*)$` — the `@custom-media` params split, without a
/// regex engine.
///
/// The boundary is ECMAScript whitespace, not Rust's: a JavaScript regex `\s`
/// matches the same set `String.prototype.trim` strips, so `--n\u{85}(x)` is
/// ONE `\S+` run there and must be one name here. See
/// [`lex::es_is_whitespace`].
fn split_custom_media(params: &str) -> Option<(&str, &str)> {
    if !params.starts_with("--") {
        return None;
    }
    let name_end = params.find(lex::es_is_whitespace).unwrap_or(params.len());
    if name_end <= 2 {
        return None;
    }
    Some((&params[..name_end], lex::es_trim(&params[name_end..])))
}
