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

use std::fmt::Write as _;

/// A value held by an AST node field.
///
/// Dropping a value and writing it as JSON are ITERATIVE, so an AST as deep
/// as its source cannot overflow the stack on either. Cloning and comparing
/// still walk the tree recursively, as the derived implementations do; that
/// is fine for any stylesheet a person wrote and worth knowing before
/// cloning one an attacker did.
#[derive(Clone, Debug, PartialEq)]
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
#[derive(Clone, Debug, Default, PartialEq)]
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
        write_steps(vec![Emit::Node(self)], &mut out);
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

/// One step of the iterative JSON writer.
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
    write_steps(vec![Emit::Value(root)], out)
}

fn write_steps(mut steps: Vec<Emit<'_>>, out: &mut String) {
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
                    .filter(|(_, v)| !matches!(v, Value::Undefined))
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
            Emit::Value(Value::Undefined) | Emit::Value(Value::Null) => out.push_str("null"),
            Emit::Value(Value::Bool(b)) => out.push_str(if *b { "true" } else { "false" }),
            Emit::Value(Value::Num(n)) => write_num(*n, out),
            Emit::Value(Value::Str(s)) => write_json_string(s, out),
        }
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
