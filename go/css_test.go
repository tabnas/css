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

func TestNestingStyleRule(t *testing.T) {
	eq(t, parse(t, "a { color: red; & b { top: 0 } }"),
		sheet(ruleNode([]any{"a"},
			decl("color", "red"),
			ruleNode([]any{"& b"}, decl("top", "0")))))
}

func TestNestingRuleFirst(t *testing.T) {
	eq(t, parse(t, "a { b { x: 1 } color: red }"),
		sheet(ruleNode([]any{"a"},
			ruleNode([]any{"b"}, decl("x", "1")),
			decl("color", "red"))))
}

func TestNestingAtMedia(t *testing.T) {
	eq(t, parse(t, "a { color: red; @media x { b { y: 1 } } }"),
		sheet(ruleNode([]any{"a"},
			decl("color", "red"),
			m{"type": "media", "media": "x", "rules": list(ruleNode([]any{"b"}, decl("y", "1")))})))
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

func TestPositionOption(t *testing.T) {
	tru := true
	ast := parse(t, "a {\n  color: red;\n}", CssOptions{Position: &tru}).(map[string]any)
	eq(t, ast["position"], m{
		"start": m{"line": 1, "column": 1},
		"end":   m{"line": 3, "column": 2},
	})
	r := ast["rules"].([]any)[0].(map[string]any)
	pos := r["position"].(map[string]any)
	eq(t, pos["start"], m{"line": 1, "column": 1})
	eq(t, pos["end"], m{"line": 3, "column": 2})
	d := r["declarations"].([]any)[0].(map[string]any)
	dpos := d["position"].(map[string]any)
	eq(t, dpos["start"], m{"line": 2, "column": 3})
	eq(t, dpos["end"], m{"line": 2, "column": 13})
}

func TestNoPositionByDefault(t *testing.T) {
	ast := parse(t, "a { x: 1 }").(map[string]any)
	if _, ok := ast["position"]; ok {
		t.Fatalf("expected no position field on stylesheet")
	}
	r := ast["rules"].([]any)[0].(map[string]any)
	if _, ok := r["position"]; ok {
		t.Fatalf("expected no position field on rule")
	}
}

func TestRealWorldSmokeNormalize(t *testing.T) {
	// An excerpt of normalize.css (MIT). We assert structural sanity rather
	// than a full deep-equal: no error, a stylesheet, and the expected shape.
	src := `
/*! normalize.css v8.0.1 | MIT License */

/* Document
   ========================================================================== */

html {
  line-height: 1.15; /* 1 */
  -webkit-text-size-adjust: 100%; /* 2 */
}

/* Sections */

body {
  margin: 0;
}

h1 {
  font-size: 2em;
  margin: 0.67em 0;
}

a {
  background-color: transparent;
}

b,
strong {
  font-weight: bolder;
}

img {
  border-style: none;
}

button,
input,
optgroup,
select,
textarea {
  font-family: inherit;
  font-size: 100%;
  line-height: 1.15;
  margin: 0;
}

[type="checkbox"],
[type="radio"] {
  box-sizing: border-box;
  padding: 0;
}

::-webkit-file-upload-button {
  -webkit-appearance: button;
  font: inherit;
}

@media (min-width: 768px) {
  body {
    margin: 0 auto;
    max-width: 960px;
  }
  .hero {
    background: url("hero;cover.png") no-repeat center / cover;
  }
}
`
	ast := parse(t, src).(map[string]any)
	if ast["type"] != "stylesheet" {
		t.Fatalf("expected stylesheet, got %v", ast["type"])
	}
	top := ast["rules"].([]any)

	nodesOfType := func(typ string) []map[string]any {
		var out []map[string]any
		for _, n := range top {
			nm := n.(map[string]any)
			if nm["type"] == typ {
				out = append(out, nm)
			}
		}
		return out
	}

	if len(nodesOfType("comment")) < 3 {
		t.Fatalf("expected banner + section comments preserved")
	}

	rules := nodesOfType("rule")
	if len(rules) < 8 {
		t.Fatalf("expected at least 8 style rules, got %d", len(rules))
	}

	// The selector group splits into a list, never a joined string.
	var group map[string]any
	for _, r := range rules {
		sels := r["selectors"].([]any)
		if len(sels) == 5 && sels[0] == "button" {
			group = r
		}
	}
	if group == nil {
		t.Fatalf("button,input,... group not found as a 5-element list")
	}

	for _, r := range rules {
		sels := r["selectors"].([]any)
		if sels[0] == `[type="checkbox"]` {
			eq(t, sels, []any{`[type="checkbox"]`, `[type="radio"]`})
		}
	}

	// Trailing comments stay attached as nodes inside a rule's declarations.
	for _, r := range rules {
		if r["selectors"].([]any)[0] == "html" {
			var types []any
			for _, d := range r["declarations"].([]any) {
				types = append(types, d.(map[string]any)["type"])
			}
			eq(t, types, []any{"declaration", "comment", "declaration", "comment"})
		}
	}

	// The @media block wraps its own rules; the url() with a ';' is one value.
	media := nodesOfType("media")
	if len(media) != 1 || media[0]["media"] != "(min-width: 768px)" {
		t.Fatalf("expected one @media (min-width: 768px) block")
	}
	for _, r := range media[0]["rules"].([]any) {
		rm := r.(map[string]any)
		if rm["selectors"].([]any)[0] == ".hero" {
			eq(t, rm["declarations"].([]any)[0], decl("background",
				`url("hero;cover.png") no-repeat center / cover`))
		}
	}
}

func TestRealisticStylesheet(t *testing.T) {
	src := `
		/* base */
		body {
			margin: 0;
			font-family: "Helvetica Neue", Arial, sans-serif;
		}
		.nav > li { display: inline-block; padding: 0 10px; }
		@media (min-width: 768px) {
			.nav > li { padding: 0 20px; }
		}
	`
	eq(t, parse(t, src), sheet(
		comment(" base "),
		ruleNode([]any{"body"},
			decl("margin", "0"),
			decl("font-family", `"Helvetica Neue", Arial, sans-serif`)),
		ruleNode([]any{".nav > li"},
			decl("display", "inline-block"),
			decl("padding", "0 10px")),
		m{"type": "media", "media": "(min-width: 768px)",
			"rules": list(ruleNode([]any{".nav > li"}, decl("padding", "0 20px")))},
	))
}
