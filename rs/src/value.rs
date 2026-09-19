/* Copyright (c) 2025 Richard Rodger, MIT License */

//! The AST value model: [`Node`], an insertion-ordered map with a `type`
//! discriminator, and [`Value`], what a node field can hold.
//!
//! The TypeScript port returns plain JavaScript objects and the Go port
//! returns `map[string]any`. Rust has no such universal value, so this module
//! supplies one. Insertion order is preserved because the AST is usually read
//! back as JSON and `{"type": …}` first reads better than a sorted map — the
//! conformance runners compare key sets, not key order, so nothing depends on
//! it.
//!
//! [`Value::Undefined`] is the one non-obvious variant. It is JavaScript's
//! `undefined`: a key that EXISTS in insertion order but is absent from the
//! serialised JSON. The canonical port writes `node.position = { start, end:
//! undefined }` at construction and fills `end` in later, so a node whose end
//! is never recorded (a declaration with an empty value, say) serialises as
//! `{"start":…}` with no `end` at all. Emitting `null` there instead would be
//! a different AST.

use std::fmt;
use std::fmt::Write as _;

/// A value held by an AST node field.
///
/// Every traversal of a value is ITERATIVE: dropping, cloning, comparing,
/// writing it as JSON and formatting it for [`std::fmt::Debug`]. An AST is as
/// deep as the CSS that produced it, so a derived implementation of any of
/// them would recurse once per level and abort the process on nothing worse
/// than untrusted input. None of them is derived, for that reason.
pub enum Value {
    /// A key that exists but serialises to nothing (JavaScript `undefined`).
    Undefined,
    /// JSON `null`.
    Null,
    /// JSON `true` / `false`.
    Bool(bool),
    /// A JSON number. Integral values serialise without a decimal point.
    Num(f64),
    /// A JSON string.
    Str(String),
    /// A JSON array.
    List(Vec<Value>),
    /// A nested AST node (a JSON object).
    Node(Node),
}

impl Value {
    /// The string held here, if this is a [`Value::Str`].
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    /// The node held here, if this is a [`Value::Node`].
    pub fn as_node(&self) -> Option<&Node> {
        match self {
            Value::Node(n) => Some(n),
            _ => None,
        }
    }

    /// The list held here, if this is a [`Value::List`].
    pub fn as_list(&self) -> Option<&[Value]> {
        match self {
            Value::List(l) => Some(l),
            _ => None,
        }
    }

    /// This value as JSON. `Undefined` renders as `null` at the top level and
    /// inside an array — only an object KEY can be dropped entirely, which is
    /// what JavaScript's `JSON.stringify` does too.
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        write_json(self, &mut out);
        out
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::Str(s.to_string())
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::Str(s)
    }
}

impl From<Node> for Value {
    fn from(n: Node) -> Self {
        Value::Node(n)
    }
}

/// An AST node: an insertion-ordered string-keyed map.
///
/// Like [`Value`], every traversal of it is iterative rather than derived.
#[derive(Default)]
pub struct Node {
    entries: Vec<(String, Value)>,
}

impl Node {
    /// An empty node.
    pub fn new() -> Self {
        Node {
            entries: Vec::new(),
        }
    }

    /// The node's `type` discriminator, if it has one.
    pub fn node_type(&self) -> Option<&str> {
        self.get("type").and_then(Value::as_str)
    }

    /// The value at `key`, if the key is present.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.entries.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    /// A mutable reference to the value at `key`, if the key is present.
    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value> {
        self.entries
            .iter_mut()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }

    /// Set `key`, keeping its existing position when it is already present
    /// (as assigning to a JavaScript object property does).
    pub fn set(&mut self, key: impl Into<String>, value: impl Into<Value>) {
        let key = key.into();
        let value = value.into();
        match self.get_mut(&key) {
            Some(slot) => *slot = value,
            None => self.entries.push((key, value)),
        }
    }

    /// Append to the list at `key`, creating the list if it is absent.
    pub fn push_to(&mut self, key: &str, value: Value) {
        match self.get_mut(key) {
            Some(Value::List(list)) => list.push(value),
            Some(slot) => *slot = Value::List(vec![value]),
            None => self
                .entries
                .push((key.to_string(), Value::List(vec![value]))),
        }
    }

    /// The entries, in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// How many keys the node holds, `Undefined` ones included.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the node holds no keys at all.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// This node as JSON. Keys holding [`Value::Undefined`] are omitted.
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        write_steps(vec![Emit::Node(self)], &mut out, Mode::Json);
        out
    }
}

/// Dismantle a node iteratively when it is dropped.
///
/// An AST is as deep as the CSS that produced it, and a derived drop would
/// recurse once per level — so a stylesheet nested a few thousand rules deep
/// would abort the process, on nothing worse than untrusted input, at the
/// moment the tree went out of scope. The rule machine keeps its own stack
/// for the same reason; this is the other half of that promise.
impl Drop for Node {
    fn drop(&mut self) {
        let mut pending: Vec<Value> = self.entries.drain(..).map(|(_, v)| v).collect();
        while let Some(value) = pending.pop() {
            match value {
                Value::List(items) => pending.extend(items),
                // `node` is emptied here, so its own drop finds nothing left
                // to do and does not recurse.
                Value::Node(mut node) => {
                    pending.extend(node.entries.drain(..).map(|(_, v)| v));
                }
                _ => {}
            }
        }
    }
}

/// What [`write_steps`] is producing.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// JSON: an `Undefined` key is dropped, as `JSON.stringify` drops a key
    /// whose value is `undefined`.
    Json,
    /// A debug view: an `Undefined` key is kept and shown, because the point
    /// of the debug view is to show what is there.
    Debug,
}

/// One step of the iterative writer.
enum Emit<'a> {
    /// Write this value (pushing more steps for a node or a list).
    Value(&'a Value),
    /// Write this node — the same work as `Value(Value::Node(..))`, for a
    /// node that is not wrapped in a [`Value`].
    Node(&'a Node),
    /// Write this object key and the `:` after it.
    Key(&'a str),
    /// Write this punctuation verbatim.
    Raw(&'static str),
}

/// Write a value as JSON, iteratively.
///
/// Iterative for the same reason [`Node`]'s drop is: recursing once per
/// nesting level turns a deeply nested stylesheet into a stack overflow, and
/// serialising the AST is the most ordinary thing a caller does with it.
fn write_json(root: &Value, out: &mut String) {
    write_steps(vec![Emit::Value(root)], out, Mode::Json)
}

fn write_steps(mut steps: Vec<Emit<'_>>, out: &mut String, mode: Mode) {
    while let Some(step) = steps.pop() {
        match step {
            Emit::Raw(text) => out.push_str(text),
            Emit::Key(key) => {
                write_json_string(key, out);
                out.push(':');
            }
            Emit::Value(Value::Node(node)) | Emit::Node(node) => {
                out.push('{');
                steps.push(Emit::Raw("}"));
                let entries: Vec<(&str, &Value)> = node
                    .iter()
                    .filter(|(_, v)| Mode::Debug == mode || !matches!(v, Value::Undefined))
                    .collect();
                // Pushed in reverse so they come back off the stack in order.
                for (i, (key, value)) in entries.iter().enumerate().rev() {
                    steps.push(Emit::Value(value));
                    steps.push(Emit::Key(key));
                    if 0 < i {
                        steps.push(Emit::Raw(","));
                    }
                }
            }
            Emit::Value(Value::List(items)) => {
                out.push('[');
                steps.push(Emit::Raw("]"));
                for (i, item) in items.iter().enumerate().rev() {
                    steps.push(Emit::Value(item));
                    if 0 < i {
                        steps.push(Emit::Raw(","));
                    }
                }
            }
            Emit::Value(Value::Undefined) => out.push_str(match mode {
                Mode::Json => "null",
                Mode::Debug => "undefined",
            }),
            Emit::Value(Value::Null) => out.push_str("null"),
            Emit::Value(Value::Bool(b)) => out.push_str(if *b { "true" } else { "false" }),
            Emit::Value(Value::Num(n)) => write_num(*n, out),
            Emit::Value(Value::Str(s)) => write_json_string(s, out),
        }
    }
}

/// The debug view is the JSON, with `Undefined` keys shown rather than
/// dropped.
///
/// Not derived, and not a struct-shaped `{:#?}` dump. A derived formatter
/// recurses once per nesting level, so `format!("{ast:?}")` on a stylesheet a
/// few thousand rules deep aborts the process — on an AST whose parse, drop
/// and serialisation are all depth-safe, and in the one operation a caller
/// reaches for while debugging. This shares the iterative writer instead.
impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = String::new();
        write_steps(vec![Emit::Value(self)], &mut out, Mode::Debug);
        f.write_str(&out)
    }
}

impl fmt::Debug for Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = String::new();
        write_steps(vec![Emit::Node(self)], &mut out, Mode::Debug);
        f.write_str(&out)
    }
}

/// One step of the iterative deep clone.
enum CloneStep<'a> {
    /// Clone this value, pushing more steps for a node or a list.
    Enter(&'a Value),
    /// Take the last `n` cloned values off the output and make a node of
    /// them, under this node's keys.
    CloseNode(&'a Node),
    /// Take the last `n` cloned values off the output and make a list.
    CloseList(usize),
}

/// A deep clone, iteratively.
///
/// A derived `Clone` recurses once per nesting level. Cloning a parsed AST is
/// an ordinary thing to do, so it is depth-safe here like everything else.
fn clone_value(root: &Value) -> Value {
    let mut steps = vec![CloneStep::Enter(root)];
    let mut done: Vec<Value> = Vec::new();
    while let Some(step) = steps.pop() {
        match step {
            // Children are pushed after the close step, so they pop first and
            // land on `done` in order, before the close reads them back.
            CloneStep::Enter(Value::Node(node)) => {
                steps.push(CloneStep::CloseNode(node));
                for (_, value) in node.entries.iter().rev() {
                    steps.push(CloneStep::Enter(value));
                }
            }
            CloneStep::Enter(Value::List(items)) => {
                steps.push(CloneStep::CloseList(items.len()));
                for item in items.iter().rev() {
                    steps.push(CloneStep::Enter(item));
                }
            }
            CloneStep::Enter(leaf) => done.push(match leaf {
                Value::Undefined => Value::Undefined,
                Value::Null => Value::Null,
                Value::Bool(b) => Value::Bool(*b),
                Value::Num(n) => Value::Num(*n),
                Value::Str(s) => Value::Str(s.clone()),
                // Handled by the arms above.
                Value::List(_) | Value::Node(_) => unreachable!(),
            }),
            CloneStep::CloseNode(node) => {
                let values = done.split_off(done.len() - node.entries.len());
                let entries = node
                    .entries
                    .iter()
                    .map(|(k, _)| k.clone())
                    .zip(values)
                    .collect();
                done.push(Value::Node(Node { entries }));
            }
            CloneStep::CloseList(n) => {
                let items = done.split_off(done.len() - n);
                done.push(Value::List(items));
            }
        }
    }
    done.pop().expect("the clone produced no value")
}

impl Clone for Value {
    fn clone(&self) -> Value {
        clone_value(self)
    }
}

impl Clone for Node {
    fn clone(&self) -> Node {
        let mut out = Node::new();
        // One level by hand, then the iterative walk for each field, so that
        // cloning a node never wraps it in a temporary Value.
        for (key, value) in &self.entries {
            out.entries.push((key.clone(), clone_value(value)));
        }
        out
    }
}

/// An equality walk, iteratively, for the same reason as the rest.
fn values_eq(a: &Value, b: &Value) -> bool {
    let mut pending: Vec<(&Value, &Value)> = vec![(a, b)];
    while let Some((a, b)) = pending.pop() {
        match (a, b) {
            (Value::Undefined, Value::Undefined) | (Value::Null, Value::Null) => {}
            (Value::Bool(a), Value::Bool(b)) if a == b => {}
            (Value::Num(a), Value::Num(b)) if a == b => {}
            (Value::Str(a), Value::Str(b)) if a == b => {}
            (Value::List(a), Value::List(b)) if a.len() == b.len() => {
                pending.extend(a.iter().zip(b.iter()));
            }
            (Value::Node(a), Value::Node(b)) if a.entries.len() == b.entries.len() => {
                for ((ak, av), (bk, bv)) in a.entries.iter().zip(b.entries.iter()) {
                    if ak != bk {
                        return false;
                    }
                    pending.push((av, bv));
                }
            }
            _ => return false,
        }
    }
    true
}

impl PartialEq for Value {
    fn eq(&self, other: &Value) -> bool {
        values_eq(self, other)
    }
}

impl PartialEq for Node {
    fn eq(&self, other: &Node) -> bool {
        self.entries.len() == other.entries.len()
            && self
                .entries
                .iter()
                .zip(other.entries.iter())
                .all(|((ak, _), (bk, _))| ak == bk)
            && self
                .entries
                .iter()
                .zip(other.entries.iter())
                .all(|((_, av), (_, bv))| values_eq(av, bv))
    }
}

/// Write a JSON number. Integral values lose the `.0` Rust would print, so
/// a line number reads as `2` and not `2.0`.
fn write_num(n: f64, out: &mut String) {
    if n.is_finite() && n == n.trunc() && n.abs() < 1e15 {
        let _ = write!(out, "{}", n as i64);
    } else if n.is_finite() {
        let _ = write!(out, "{}", n);
    } else {
        // JSON has no infinity or NaN; JavaScript's JSON.stringify writes
        // null for both.
        out.push_str("null");
    }
}

/// Write a JSON string literal, escaping exactly what `JSON.stringify` does.
fn write_json_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}
