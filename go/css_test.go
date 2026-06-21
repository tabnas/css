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
		t.Fatalf("got %#v, want %#v", got, want)
	}
}

// m is a shorthand for building expected map[string]any values.
type m = map[string]any

func TestEmptyStylesheet(t *testing.T) {
	// A zero-length source runs no rules (an engine convention) and yields
	// nil; any non-empty source (even whitespace or comments) yields an empty
	// stylesheet map.
	eq(t, parse(t, ""), nil)
	eq(t, parse(t, "   \n  "), m{})
	eq(t, parse(t, "/* only a comment */"), m{})
}

func TestSingleRuleSingleDeclaration(t *testing.T) {
	eq(t, parse(t, "a { color: red; }"), m{"a": m{"color": "red"}})
}

func TestDeclarationWithoutTrailingSemicolon(t *testing.T) {
	eq(t, parse(t, "a { color: red }"), m{"a": m{"color": "red"}})
}

func TestMultipleDeclarations(t *testing.T) {
	eq(t, parse(t, "a { color: red; font-size: 12px; }"),
		m{"a": m{"color": "red", "font-size": "12px"}})
}

func TestEmptyRuleBlock(t *testing.T) {
	eq(t, parse(t, "a {}"), m{"a": m{}})
}

func TestMultipleRules(t *testing.T) {
	eq(t, parse(t, "a { color: red } b { color: blue }"),
		m{"a": m{"color": "red"}, "b": m{"color": "blue"}})
}

func TestCompoundValue(t *testing.T) {
	eq(t, parse(t, "p { border: 1px solid #fff; }"),
		m{"p": m{"border": "1px solid #fff"}})
}

func TestSelectorGrouping(t *testing.T) {
	eq(t, parse(t, "h1, h2 { margin: 0 }"), m{"h1, h2": m{"margin": "0"}})
}

func TestCombinatorAndClassSelectors(t *testing.T) {
	eq(t, parse(t, ".foo > .bar { top: 0 }"), m{".foo > .bar": m{"top": "0"}})
}

func TestPseudoClassSelector(t *testing.T) {
	eq(t, parse(t, "a:hover { color: red }"), m{"a:hover": m{"color": "red"}})
}

func TestPseudoElementSelector(t *testing.T) {
	eq(t, parse(t, "a::before { content: \"x\" }"),
		m{"a::before": m{"content": "\"x\""}})
}

func TestAttributeSelector(t *testing.T) {
	eq(t, parse(t, "input[type=text] { border: 0 }"),
		m{"input[type=text]": m{"border": "0"}})
}

func TestValueContainingColon(t *testing.T) {
	eq(t, parse(t, "a { background: url(http://x/y.png) }"),
		m{"a": m{"background": "url(http://x/y.png)"}})
}

func TestFunctionValueWithCommas(t *testing.T) {
	eq(t, parse(t, "a { color: rgb(1, 2, 3); top: 0 }"),
		m{"a": m{"color": "rgb(1, 2, 3)", "top": "0"}})
}

func TestBlockCommentsIgnored(t *testing.T) {
	src := `/* header */ a {
      color: red; /* the colour */
      /* a gap */
      top: 0;
    }`
	eq(t, parse(t, src), m{"a": m{"color": "red", "top": "0"}})
}

func TestNestedAtRule(t *testing.T) {
	eq(t, parse(t, "@media screen { a { color: blue } }"),
		m{"@media screen": m{"a": m{"color": "blue"}}})
}

func TestAtRulePreludeWithParens(t *testing.T) {
	eq(t, parse(t, "@media (max-width: 600px) { a { color: red } }"),
		m{"@media (max-width: 600px)": m{"a": m{"color": "red"}}})
}

func TestStatementAtRule(t *testing.T) {
	eq(t, parse(t, "@import \"base.css\";"), m{"@import": "\"base.css\""})
}

func TestStatementAtRuleThenRule(t *testing.T) {
	eq(t, parse(t, "@charset \"utf-8\"; a { color: red }"),
		m{"@charset": "\"utf-8\"", "a": m{"color": "red"}})
}

func TestImportantIsPartOfValue(t *testing.T) {
	eq(t, parse(t, "a { color: red !important }"),
		m{"a": m{"color": "red !important"}})
}

func TestRealisticStylesheet(t *testing.T) {
	src := `
      body {
        margin: 0;
        font-family: "Helvetica Neue", Arial, sans-serif;
      }
      .nav > li {
        display: inline-block;
        padding: 0 10px;
      }
      @media (min-width: 768px) {
        .nav > li { padding: 0 20px; }
      }
    `
	eq(t, parse(t, src), m{
		"body": m{
			"margin":      "0",
			"font-family": "\"Helvetica Neue\", Arial, sans-serif",
		},
		".nav > li": m{
			"display": "inline-block",
			"padding": "0 10px",
		},
		"@media (min-width: 768px)": m{
			".nav > li": m{"padding": "0 20px"},
		},
	})
}

func TestLowercasePropertiesOption(t *testing.T) {
	tru := true
	eq(t, parse(t, "A { COLOR: Red }", CssOptions{LowercaseProperties: &tru}),
		m{"A": m{"color": "Red"}})
}

func TestLowercaseValuesOption(t *testing.T) {
	tru := true
	eq(t, parse(t, "a { color: RED }", CssOptions{LowercaseValues: &tru}),
		m{"a": m{"color": "red"}})
}
