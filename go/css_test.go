/* Copyright (c) 2025 Richard Rodger and other contributors, MIT License */

package tabnascss

import (
	"reflect"
	"testing"
)

func parse(t *testing.T, src string, opts ...CssOptions) any {
	t.Helper()
	out, err := Parse(src, opts...)
	if err != nil {
		t.Fatalf("parse(%q) error: %v", src, err)
	}
	return out
}

func eq(t *testing.T, got, want any) {
	t.Helper()
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("\n got:  %#v\n want: %#v", got, want)
	}
}

type m = map[string]any

func sheet(rules ...any) m { return m{"type": "stylesheet", "rules": list(rules...)} }
func list(xs ...any) []any {
	if xs == nil {
		return []any{}
	}
	return xs
}
func ruleNode(sels []any, decls ...any) m {
	return m{"type": "rule", "selectors": sels, "declarations": list(decls...)}
}
func decl(prop, val string) m {
	return m{"type": "declaration", "property": prop, "value": val}
}
func comment(c string) m { return m{"type": "comment", "comment": c} }

func TestEmptyStylesheet(t *testing.T) {
	eq(t, parse(t, "   \n  "), sheet())
	eq(t, parse(t, "/* only */"), sheet(comment(" only ")))
}

func TestSingleRule(t *testing.T) {
	eq(t, parse(t, "a { color: red; }"),
		sheet(ruleNode([]any{"a"}, decl("color", "red"))))
}

func TestDeclOrderAndDuplicates(t *testing.T) {
	eq(t, parse(t, "a { color: red; color: blue }"),
		sheet(ruleNode([]any{"a"}, decl("color", "red"), decl("color", "blue"))))
}

func TestNoTrailingSemicolon(t *testing.T) {
	eq(t, parse(t, "a { color: red }"),
		sheet(ruleNode([]any{"a"}, decl("color", "red"))))
}

func TestEmptyRuleBlock(t *testing.T) {
	eq(t, parse(t, "a {}"), sheet(ruleNode([]any{"a"})))
}

func TestMultipleRulesKeepOrder(t *testing.T) {
	eq(t, parse(t, "a { x: 1 } b { y: 2 }"),
		sheet(ruleNode([]any{"a"}, decl("x", "1")), ruleNode([]any{"b"}, decl("y", "2"))))
}

func TestSelectorGroupIsList(t *testing.T) {
	eq(t, parse(t, "h1, h2 { margin: 0 }"),
		sheet(ruleNode([]any{"h1", "h2"}, decl("margin", "0"))))
}

func TestCommaInsideNot(t *testing.T) {
	eq(t, parse(t, "a:not(.x, .y), b { top: 0 }"),
		sheet(ruleNode([]any{"a:not(.x, .y)", "b"}, decl("top", "0"))))
}

func TestCompoundValues(t *testing.T) {
	eq(t, parse(t, "p { border: 1px solid #fff; color: rgb(1, 2, 3) }"),
		sheet(ruleNode([]any{"p"}, decl("border", "1px solid #fff"), decl("color", "rgb(1, 2, 3)"))))
}

func TestImportant(t *testing.T) {
	eq(t, parse(t, "a { color: red !important }"),
		sheet(ruleNode([]any{"a"}, decl("color", "red !important"))))
}

func TestCommentNodes(t *testing.T) {
	eq(t, parse(t, "/* head */ a { /* c1 */ color: red; /* c2 */ }"),
		sheet(comment(" head "), ruleNode([]any{"a"}, comment(" c1 "), decl("color", "red"), comment(" c2 "))))
}

func TestMidConstructCommentsSkipped(t *testing.T) {
	eq(t, parse(t, "a /* x */ { color /* y */ : red }"),
		sheet(ruleNode([]any{"a"}, decl("color", "red"))))
}

func TestMedia(t *testing.T) {
	eq(t, parse(t, "@media screen { a { color: blue } }"),
		sheet(m{"type": "media", "media": "screen", "rules": list(ruleNode([]any{"a"}, decl("color", "blue")))}))
}

func TestSupports(t *testing.T) {
	eq(t, parse(t, "@supports (display: grid) { a { x: 1 } }"),
		sheet(m{"type": "supports", "supports": "(display: grid)", "rules": list(ruleNode([]any{"a"}, decl("x", "1")))}))
}

func TestFontFace(t *testing.T) {
	eq(t, parse(t, `@font-face { font-family: "A"; src: url(a.woff) }`),
		sheet(m{"type": "font-face", "declarations": list(decl("font-family", `"A"`), decl("src", "url(a.woff)"))}))
}

func TestImport(t *testing.T) {
	eq(t, parse(t, `@import "base.css";`),
		sheet(m{"type": "import", "import": `"base.css"`}))
}

func TestCharsetThenRule(t *testing.T) {
	eq(t, parse(t, `@charset "utf-8"; a { x: 1 }`),
		sheet(m{"type": "charset", "charset": `"utf-8"`}, ruleNode([]any{"a"}, decl("x", "1"))))
}

func TestKeyframes(t *testing.T) {
	eq(t, parse(t, "@keyframes slide { from { left: 0 } 50%, 100% { left: 10px } }"),
		sheet(m{"type": "keyframes", "name": "slide", "keyframes": list(
			m{"type": "keyframe", "values": []any{"from"}, "declarations": list(decl("left", "0"))},
			m{"type": "keyframe", "values": []any{"50%", "100%"}, "declarations": list(decl("left", "10px"))},
		)}))
}

func TestVendorKeyframes(t *testing.T) {
	eq(t, parse(t, "@-webkit-keyframes x { to { opacity: 1 } }"),
		sheet(m{"type": "keyframes", "name": "x", "vendor": "-webkit-", "keyframes": list(
			m{"type": "keyframe", "values": []any{"to"}, "declarations": list(decl("opacity", "1"))},
		)}))
}

func TestPage(t *testing.T) {
	eq(t, parse(t, "@page :first { margin: 1in }"),
		sheet(m{"type": "page", "selectors": []any{":first"}, "declarations": list(decl("margin", "1in"))}))
}

func TestNamespaceStatement(t *testing.T) {
	eq(t, parse(t, "@namespace svg url(http://x);"),
		sheet(m{"type": "namespace", "namespace": "svg url(http://x)"}))
}

func TestVendorDocument(t *testing.T) {
	eq(t, parse(t, "@-moz-document url(x) { a { c: 1 } }"),
		sheet(m{"type": "document", "document": "url(x)", "vendor": "-moz-",
			"rules": list(ruleNode([]any{"a"}, decl("c", "1")))}))
}

func TestGenericBlockAtRule(t *testing.T) {
	eq(t, parse(t, "@layer base { a { c: 1 } }"),
		sheet(m{"type": "layer", "layer": "base", "rules": list(ruleNode([]any{"a"}, decl("c", "1")))}))
}

func TestCommentInsideMedia(t *testing.T) {
	eq(t, parse(t, "@media x { /* c */ a { b: 1 } }"),
		sheet(m{"type": "media", "media": "x",
			"rules": list(comment(" c "), ruleNode([]any{"a"}, decl("b", "1")))}))
}

func TestSemicolonInsideUrlString(t *testing.T) {
	eq(t, parse(t, `a { background: url("a;b.png") }`),
		sheet(ruleNode([]any{"a"}, decl("background", `url("a;b.png")`))))
}

func TestLowercaseProperties(t *testing.T) {
	tru := true
	eq(t, parse(t, "A { COLOR: Red }", CssOptions{LowercaseProperties: &tru}),
		sheet(ruleNode([]any{"A"}, decl("color", "Red"))))
}
