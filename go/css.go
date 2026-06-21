/* Copyright (c) 2025 Richard Rodger, MIT License */

// Package css is a jsonic plugin that parses CSS (Cascading Style Sheets)
// into a reworkcss-style abstract syntax tree: ordered, typed nodes that
// preserve declaration order, duplicate properties, rule types and comments.
//
//	{ "type": "stylesheet", "rules": [ ...nodes ] }
//
// where each node is a map[string]any with a "type" discriminator: "rule"
// (selectors []any, declarations []any), "declaration" (property, value),
// "comment" (comment), at-rule nodes (media/supports/import/keyframes/...).
//
// Example:
//
//	a { color: red; color: blue } /* note */
//
// parses to:
//
//	map[string]any{"type": "stylesheet", "rules": []any{
//	  map[string]any{"type": "rule", "selectors": []any{"a"}, "declarations": []any{
//	    map[string]any{"type": "declaration", "property": "color", "value": "red"},
//	    map[string]any{"type": "declaration", "property": "color", "value": "blue"}}},
//	  map[string]any{"type": "comment", "comment": " note "}}}
package tabnascss

import (
	"fmt"
	"regexp"
	"strings"
	"sync"

	jsonic "github.com/tabnas/jsonic/go"
)

const Version = "0.1.0"

// --- BEGIN EMBEDDED css-grammar.jsonic ---
const grammarText = `
# CSS Grammar Definition (AST)
# Parses CSS into a reworkcss-style abstract syntax tree: ordered, typed
# nodes that preserve declaration order, duplicate properties, rule types
# and comments.
#
#   { type: 'stylesheet', rules: [ Node, ... ] }
#   rule        { type:'rule',        selectors:[string], declarations:[Node] }
#   declaration { type:'declaration', property:string, value:string }
#   comment     { type:'comment',     comment:string }
#   at-rules (media/supports/import/keyframes/font-face/...) — see below
#
# Example:
#   a { color: red; color: blue } /* note */
# parses to:
#   { type:'stylesheet', rules: [
#     { type:'rule', selectors:['a'], declarations:[
#       { type:'declaration', property:'color', value:'red' },
#       { type:'declaration', property:'color', value:'blue' } ] },
#     { type:'comment', comment:' note ' } ] }
#
# The cssToken lex matcher emits: #TX (one selector or a property name),
# #GC (a top-level selector-group comma), #VL (a declaration value),
# #CC (a comment, at a statement position), and the at-rule tokens
# #ATR/#ATD/#ATK/#ATS (carrying the keyword in val and the prelude in
# use). Fixed "{" "}" ":" lex as #OB #CB #CL; ";" is remapped to #CA.
#
# Every alt is tagged with the 'css' group via { rule: { alt: { g: 'css' } } }.

{
  rule: {
    # The top-level stylesheet node. "items" fills its rules[].
    stylesheet: {
      open: [
        { a: '@cssSheet' p: items g: 'css,sheet' }
      ]
      close: [
        { s: '#ZZ' a: '@cssEnd' g: 'css,sheet,end' }
      ]
    }

    # A statement list (the stylesheet body or an at-rule's rules body). Reads
    # rule / at-rule / comment nodes into the enclosing node's rules[]. Each
    # iteration pushes one "statement" child, then @cssPushRule appends it.
    items: {
      open: [
        { s: '#ZZ' b: 1 g: 'css,items,end' }
        { s: '#CB' b: 1 g: 'css,items,endblock' }
        { p: statement g: 'css,items' }
      ]
      close: [
        { s: '#ZZ' b: 1 a: '@cssPushRule' g: 'css,items,end' }
        { s: '#CB' b: 1 a: '@cssPushRule' g: 'css,items,endblock' }
        { a: '@cssPushRule' r: items g: 'css,items,next' }
      ]
    }

    # Builds ONE statement node (resetting from the inherited list node).
    statement: {
      open: [
        # A comment node.
        { s: '#CC' a: '@cssComment' g: 'css,comment' }
        # A block at-rule whose body is rules (e.g. @media): push items.
        { s: '#ATR' a: '@cssAtRules' p: rulesbody g: 'css,atrules' }
        # A block at-rule whose body is declarations (e.g. @font-face).
        { s: '#ATD' a: '@cssAtDecls' p: declbody g: 'css,atdecls' }
        # @keyframes: a body of keyframe blocks.
        { s: '#ATK' a: '@cssKeyframes' p: kfbody g: 'css,keyframes' }
        # A statement at-rule (e.g. @import "x"): a leaf node.
        { s: '#ATS' a: '@cssAtStmt' g: 'css,atstmt' }
        # A style rule: selectors + declarations.
        { s: '#TX' b: 1 a: '@cssRule' p: sel g: 'css,rule' }
      ]
      close: [
        { b: 1 g: 'css,statement,end' }
      ]
    }

    # Reads a selector group into the (rule/page) node's selectors[], then the
    # declaration block. Each selector is one #TX; #GC separates a group.
    sel: {
      open: [
        { s: '#TX #GC' a: '@cssSelector' r: sel g: 'css,sel,group' }
        { s: '#TX #OB' b: 1 a: '@cssSelector' p: declbody g: 'css,sel,last' }
      ]
      close: [
        { b: 1 g: 'css,sel,end' }
      ]
    }

    # The "{ ... }" wrapper around a declaration list; fills the parent node's
    # declarations[].
    declbody: {
      open: [
        { s: '#OB #CB' b: 1 g: 'css,declbody,empty' }
        { s: '#OB' p: decls g: 'css,declbody' }
      ]
      close: [
        { s: '#CB' a: '@cssEnd' g: 'css,declbody,end' }
      ]
    }

    # A declaration list: declaration / comment nodes, ";"-separated.
    decls: {
      open: [
        { s: '#CB' b: 1 g: 'css,decls,empty' }
        { p: decl g: 'css,decls' }
      ]
      close: [
        { s: '#CA #CB' b: 1 a: '@cssPushDecl' g: 'css,decls,trailing' }
        { s: '#CB' b: 1 a: '@cssPushDecl' g: 'css,decls,end' }
        { s: '#CA' a: '@cssPushDecl' r: decls g: 'css,decls,next' }
        { a: '@cssPushDecl' r: decls g: 'css,decls,comment' }
      ]
    }

    # Builds ONE block member: a declaration, a comment, or — for CSS
    # Nesting — a nested style rule or at-rule. Disambiguated by the token
    # after the key: #TX #CL is a declaration; #TX followed by "{"/"," is a
    # nested rule; #ATR/#ATD/#ATK/#ATS is a nested at-rule.
    decl: {
      open: [
        { s: '#CC' a: '@cssComment' g: 'css,comment' }
        { s: '#TX #CL' a: '@cssDecl' p: declval g: 'css,decl' }
        # Nested at-rules (CSS Nesting).
        { s: '#ATR' a: '@cssAtRules' p: rulesbody g: 'css,nest,atrules' }
        { s: '#ATD' a: '@cssAtDecls' p: declbody g: 'css,nest,atdecls' }
        { s: '#ATK' a: '@cssKeyframes' p: kfbody g: 'css,nest,keyframes' }
        { s: '#ATS' a: '@cssAtStmt' g: 'css,nest,atstmt' }
        # A nested style rule (selector first, no ":").
        { s: '#TX' b: 1 a: '@cssRule' p: sel g: 'css,nest,rule' }
      ]
      close: [
        { b: 1 g: 'css,decl,end' }
      ]
    }

    # The value of a declaration (a single #VL run).
    declval: {
      open: [
        { s: '#VL' a: '@cssDeclVal' g: 'css,declval' }
        { b: 1 g: 'css,declval,empty' }
      ]
      close: [
        { b: 1 g: 'css,declval,end' }
      ]
    }

    # The "{ ... }" rules body of a block at-rule (@media/@supports/...).
    rulesbody: {
      open: [
        { s: '#OB #CB' b: 1 g: 'css,rulesbody,empty' }
        { s: '#OB' p: items g: 'css,rulesbody' }
      ]
      close: [
        { s: '#CB' a: '@cssEnd' g: 'css,rulesbody,end' }
      ]
    }

    # The "{ ... }" body of @keyframes: a list of keyframe blocks.
    kfbody: {
      open: [
        { s: '#OB #CB' b: 1 g: 'css,kfbody,empty' }
        { s: '#OB' p: kfitems g: 'css,kfbody' }
      ]
      close: [
        { s: '#CB' a: '@cssEnd' g: 'css,kfbody,end' }
      ]
    }

    # A list of keyframe blocks (and comments) -> the keyframes node's
    # keyframes[].
    kfitems: {
      open: [
        { s: '#CB' b: 1 g: 'css,kfitems,empty' }
        { p: keyframe g: 'css,kfitems' }
      ]
      close: [
        { s: '#CB' b: 1 a: '@cssPushKf' g: 'css,kfitems,end' }
        { a: '@cssPushKf' r: kfitems g: 'css,kfitems,next' }
      ]
    }

    # One keyframe block: values (0%, 50%, from, to) + declarations. Mirrors
    # "statement"+"sel" but builds a 'keyframe' node with values[].
    keyframe: {
      open: [
        { s: '#CC' a: '@cssComment' g: 'css,comment' }
        { s: '#TX' b: 1 a: '@cssKeyframe' p: kfsel g: 'css,keyframe' }
      ]
      close: [
        { b: 1 g: 'css,keyframe,end' }
      ]
    }

    kfsel: {
      open: [
        { s: '#TX #GC' a: '@cssKfValue' r: kfsel g: 'css,kfsel,group' }
        { s: '#TX #OB' b: 1 a: '@cssKfValue' p: declbody g: 'css,kfsel,last' }
      ]
      close: [
        { b: 1 g: 'css,kfsel,end' }
      ]
    }
  }
}
`
// --- END EMBEDDED css-grammar.jsonic ---

// Css is a jsonic plugin that adds CSS parsing support.
// Options are pre-merged with Defaults by jsonic.UseDefaults.
func Css(j *jsonic.Jsonic, options map[string]any) error {
	// Guard against re-invocation: SetOptions triggers plugin re-application.
	if j.Decoration("css-init") != nil {
		return nil
	}
	j.Decorate("css-init", true)

	lowercaseProperties := toBool(options["lowercaseProperties"])
	position := toBool(options["position"])

	// Resolve tins for the custom tokens on this instance, so the matcher
	// emits the same tins the grammar's alts resolve to.
	tins := cssTins{
		cc:  j.Token("#CC"),
		gc:  j.Token("#GC"),
		atr: j.Token("#ATR"),
		atd: j.Token("#ATD"),
		atk: j.Token("#ATK"),
		ats: j.Token("#ATS"),
	}

	// The grammar builds the typed AST entirely from these grammar-local
	// actions (node constructors, field setters, and array pushers).
	gs, err := parseGrammarText(grammarText, makeActions(lowercaseProperties, position))
	if err != nil {
		return err
	}

	// All jsonic option overrides live on the grammar object so the plugin
	// applies them atomically alongside its rule alts.
	semi := ";"
	gs.Options = &jsonic.Options{
		Rule: &jsonic.RuleOptions{
			Exclude: "jsonic,imp",
			Start:   "stylesheet",
		},
		Fixed: &jsonic.FixedOptions{
			Token: map[string]*string{
				"#CA": &semi,
				"#OS": nil,
				"#CS": nil,
			},
		},
		TokenSet: map[string][]string{
			"KEY": {"#TX"},
		},
		String: &jsonic.StringOptions{Chars: ""},
		Number: &jsonic.NumberOptions{Lex: boolPtr(false)},
		Text:   &jsonic.TextOptions{Lex: boolPtr(false)},
		Value:  &jsonic.ValueOptions{Lex: boolPtr(false)},
		Comment: &jsonic.CommentOptions{
			Lex: boolPtr(true),
			Def: map[string]*jsonic.CommentDef{
				"hash":  {Line: true, Start: "#", Lex: boolPtr(false)},
				"slash": {Line: true, Start: "//", Lex: boolPtr(false)},
				"multi": {Line: false, Start: "/*", End: "*/", Lex: boolPtr(true)},
			},
		},
		Lex: &jsonic.LexOptions{
			Match: map[string]*jsonic.MatchSpec{
				"cssToken": {Order: 100000, Make: buildCssTokenMatcher(lowercaseProperties, tins)},
			},
		},
	}

	setting := &jsonic.GrammarSetting{
		Rule: &jsonic.GrammarSettingRule{Alt: &jsonic.GrammarSettingAlt{G: "css"}},
	}
	if err := j.Grammar(gs, setting); err != nil {
		return fmt.Errorf("css: failed to apply grammar: %w", err)
	}
	return nil
}

// Defaults matches the TS Css.defaults. Used with jsonic.UseDefaults.
var Defaults = map[string]any{
	"lowercaseProperties": false,
	"position":            false,
}

// CssOptions is a typed wrapper for the plugin options.
type CssOptions struct {
	// LowercaseProperties, when true, lowercases declaration property names
	// (CSS property names are case-insensitive). Selectors, values and at-rule
	// preludes are untouched.
	LowercaseProperties *bool
	// Position, when true, attaches a position {start{line,column},end{...}}
	// (1-based) to every node. Off by default.
	Position *bool
}

func (o CssOptions) toMap() map[string]any {
	m := map[string]any{}
	if o.LowercaseProperties != nil {
		m["lowercaseProperties"] = *o.LowercaseProperties
	}
	if o.Position != nil {
		m["position"] = *o.Position
	}
	return m
}

// MakeJsonic returns a reusable Jsonic instance configured for CSS parsing.
func MakeJsonic(opts ...CssOptions) *jsonic.Jsonic {
	j := jsonic.Make()
	var m map[string]any
	if len(opts) > 0 {
		m = opts[0].toMap()
	}
	if err := j.UseDefaults(Css, Defaults, m); err != nil {
		panic(fmt.Sprintf("css: plugin initialisation failed: %v", err))
	}
	return j
}

var (
	defaultOnce   sync.Once
	defaultParser *jsonic.Jsonic
)

// Parse parses a CSS string and returns its AST. The no-options path reuses a
// cached instance (safe for concurrent use); option-taking calls build a
// dedicated instance.
func Parse(src string, opts ...CssOptions) (any, error) {
	if len(opts) == 0 {
		defaultOnce.Do(func() { defaultParser = MakeJsonic() })
		return defaultParser.Parse(src)
	}
	return MakeJsonic(opts...).Parse(src)
}

// --- Grammar actions: build the AST ---------------------------------------

func node(r *jsonic.Rule) map[string]any {
	m, _ := r.Node.(map[string]any)
	return m
}

func tokenVal(r *jsonic.Rule) any {
	if r.O0 == nil {
		return nil
	}
	return r.O0.Val
}

func childMap(r *jsonic.Rule) (map[string]any, bool) {
	if r.Child == nil {
		return nil, false
	}
	m, ok := r.Child.Node.(map[string]any)
	return m, ok
}

func appendField(r *jsonic.Rule, field string, v any) {
	m := node(r)
	if m == nil {
		return
	}
	arr, _ := m[field].([]any)
	m[field] = append(arr, v)
}

// makeActions returns all grammar-local actions keyed by their @name ref. The
// node constructors overwrite r.Node; the field setters mutate it; the pushers
// append a finished child node to a parent array. When position is on, the
// constructors record node["position"]["start"] (and single-token nodes their
// end); @cssEnd records the closing-brace end.
func makeActions(_lowercaseProperties bool, position bool) map[jsonic.FuncRef]any {
	mk := func(f func(*jsonic.Rule)) jsonic.AltAction {
		return jsonic.AltAction(func(r *jsonic.Rule, _ *jsonic.Context) { f(r) })
	}
	// withPos records a node's start (and optionally end) from a token.
	withPos := func(n map[string]any, tok *jsonic.Token, end bool) map[string]any {
		if position && tok != nil {
			p := map[string]any{"start": startPos(tok)}
			if end {
				p["end"] = endPos(tok)
			} else {
				p["end"] = nil
			}
			n["position"] = p
		}
		return n
	}
	return map[jsonic.FuncRef]any{
		// Node constructors.
		"@cssSheet": mk(func(r *jsonic.Rule) {
			r.Node = map[string]any{"type": "stylesheet", "rules": []any{}}
			if position {
				r.Node.(map[string]any)["position"] = map[string]any{
					"start": map[string]any{"line": 1, "column": 1}, "end": nil}
			}
		}),
		"@cssRule": mk(func(r *jsonic.Rule) {
			r.Node = withPos(map[string]any{"type": "rule", "selectors": []any{}, "declarations": []any{}}, r.O0, false)
		}),
		"@cssDecl": mk(func(r *jsonic.Rule) {
			r.Node = withPos(map[string]any{"type": "declaration", "property": tokenVal(r), "value": ""}, r.O0, false)
		}),
		"@cssComment": mk(func(r *jsonic.Rule) {
			r.Node = withPos(map[string]any{"type": "comment", "comment": tokenVal(r)}, r.O0, true)
		}),
		"@cssKeyframe": mk(func(r *jsonic.Rule) {
			r.Node = withPos(map[string]any{"type": "keyframe", "values": []any{}, "declarations": []any{}}, r.O0, false)
		}),
		"@cssAtRules":   mk(func(r *jsonic.Rule) { r.Node = withPos(makeAtRules(r.O0), r.O0, false) }),
		"@cssAtDecls":   mk(func(r *jsonic.Rule) { r.Node = withPos(makeAtDecls(r.O0), r.O0, false) }),
		"@cssKeyframes": mk(func(r *jsonic.Rule) { r.Node = withPos(makeKeyframes(r.O0), r.O0, false) }),
		"@cssAtStmt":    mk(func(r *jsonic.Rule) { r.Node = withPos(makeAtStmt(r.O0), r.O0, true) }),

		// Field setters.
		"@cssSelector": mk(func(r *jsonic.Rule) { appendField(r, "selectors", tokenVal(r)) }),
		"@cssKfValue":  mk(func(r *jsonic.Rule) { appendField(r, "values", tokenVal(r)) }),
		"@cssDeclVal": mk(func(r *jsonic.Rule) {
			if m := node(r); m != nil {
				m["value"] = tokenVal(r)
				if position {
					if p, ok := m["position"].(map[string]any); ok {
						p["end"] = endPos(r.O0)
					}
				}
			}
		}),

		// Record the closing-brace / end-of-input end position. Runs in a
		// close phase, so the matched }/end token is in r.C0.
		"@cssEnd": mk(func(r *jsonic.Rule) {
			if !position {
				return
			}
			if m := node(r); m != nil && r.C0 != nil {
				if p, ok := m["position"].(map[string]any); ok {
					p["end"] = endPos(r.C0)
				}
			}
		}),

		// Array pushers.
		"@cssPushRule": mk(func(r *jsonic.Rule) {
			if c, ok := childMap(r); ok {
				appendField(r, "rules", c)
			}
		}),
		"@cssPushDecl": mk(func(r *jsonic.Rule) {
			if c, ok := childMap(r); ok {
				appendField(r, "declarations", c)
			}
		}),
		"@cssPushKf": mk(func(r *jsonic.Rule) {
			if c, ok := childMap(r); ok {
				appendField(r, "keyframes", c)
			}
		}),
	}
}

func atRuleVal(tok *jsonic.Token) (kw, prelude string) {
	if tok == nil {
		return "", ""
	}
	kw, _ = tok.Val.(string)
	if tok.Use != nil {
		if p, ok := tok.Use["prelude"].(string); ok {
			prelude = p
		}
	}
	return kw, prelude
}

func makeAtRules(tok *jsonic.Token) map[string]any {
	kw, prelude := atRuleVal(tok)
	switch {
	case kw == "media":
		return map[string]any{"type": "media", "media": prelude, "rules": []any{}}
	case kw == "supports":
		return map[string]any{"type": "supports", "supports": prelude, "rules": []any{}}
	case kw == "host":
		return map[string]any{"type": "host", "rules": []any{}}
	case kw == "document" || strings.HasSuffix(kw, "-document"):
		n := map[string]any{"type": "document", "document": prelude, "rules": []any{}}
		if v := vendorPrefix(kw); v != "" {
			n["vendor"] = v
		}
		return n
	default:
		return map[string]any{"type": kw, kw: prelude, "rules": []any{}}
	}
}

func makeAtDecls(tok *jsonic.Token) map[string]any {
	kw, prelude := atRuleVal(tok)
	switch kw {
	case "font-face":
		return map[string]any{"type": "font-face", "declarations": []any{}}
	case "page":
		sels := []any{}
		if prelude != "" {
			sels = []any{prelude}
		}
		return map[string]any{"type": "page", "selectors": sels, "declarations": []any{}}
	default:
		return map[string]any{"type": kw, "declarations": []any{}}
	}
}

func makeKeyframes(tok *jsonic.Token) map[string]any {
	kw, name := atRuleVal(tok)
	n := map[string]any{"type": "keyframes", "name": name}
	if v := vendorPrefix(kw); v != "" {
		n["vendor"] = v
	}
	n["keyframes"] = []any{}
	return n
}

func makeAtStmt(tok *jsonic.Token) map[string]any {
	kw := ""
	params := ""
	if tok != nil {
		kw, _ = tok.Val.(string)
		if tok.Use != nil {
			if p, ok := tok.Use["params"].(string); ok {
				params = p
			}
		}
	}
	return map[string]any{"type": kw, kw: params}
}

var vendorRe = regexp.MustCompile(`^(-[a-z]+-)`)

func vendorPrefix(kw string) string {
	return vendorRe.FindString(kw)
}

// --- Lexer ----------------------------------------------------------------

type cssTins struct {
	cc, gc, atr, atd, atk, ats jsonic.Tin
}

// commentNodeRules: rule names at which a comment is captured as a node (the
// statement / declaration / keyframe LIST readers and block wrappers, which
// lex the first body token). Elsewhere comments are skipped.
var commentNodeRules = map[string]bool{
	"items":     true,
	"decls":     true,
	"kfitems":   true,
	"declbody":  true,
	"rulesbody": true,
	"kfbody":    true,
}

var keyframesRe = regexp.MustCompile(`^(-[a-z]+-)?keyframes$`)

var declsKw = map[string]bool{
	"font-face":           true,
	"page":                true,
	"viewport":            true,
	"-ms-viewport":        true,
	"counter-style":       true,
	"property":            true,
	"font-palette-values": true,
}

// buildCssTokenMatcher builds the single lex matcher. See the TypeScript
// plugin (src/css.ts) for the canonical commentary; the two are kept in step.
func buildCssTokenMatcher(lowercaseProperties bool, tins cssTins) jsonic.MakeLexMatcher {
	return func(_ *jsonic.LexConfig, _ *jsonic.Options) jsonic.LexMatcher {
		return func(lex *jsonic.Lex, rule *jsonic.Rule) *jsonic.Token {
			pnt := lex.Cursor()
			src := lex.Src
			sI := pnt.SI
			if sI >= len(src) {
				return nil
			}
			c := src[sI]
			name := rule.Name

			// Defer whitespace.
			if c == ' ' || c == '\t' || c == '\r' || c == '\n' {
				return nil
			}

			// Comments: a node at a list position, otherwise deferred (skipped).
			if c == '/' && sI+1 < len(src) && src[sI+1] == '*' {
				if !commentNodeRules[name] {
					return nil
				}
				e := strings.Index(src[sI+2:], "*/")
				contentEnd := len(src)
				end := len(src)
				if e >= 0 {
					contentEnd = sI + 2 + e
					end = contentEnd + 2
				}
				tkn := lex.Token("#CC", tins.cc, src[sI+2:contentEnd], src[sI:end])
				advance(pnt, src, sI, end)
				return tkn
			}

			// Value position.
			if name == "declval" {
				if c == '{' || c == '}' || c == ';' || c == ':' {
					return nil
				}
				endI := scanValueEnd(src, sI)
				val := strings.TrimSpace(stripComments(src[sI:endI]))
				tkn := lex.Token("#VL", jsonic.TinVL, val, src[sI:endI])
				advance(pnt, src, sI, endI)
				return tkn
			}

			// A top-level selector-group comma.
			if c == ',' {
				tkn := lex.Token("#GC", tins.gc, ",", ",")
				advance(pnt, src, sI, sI+1)
				return tkn
			}

			// An at-rule.
			if c == '@' {
				return matchAtRule(lex, src, sI, tins)
			}

			// Other fixed punctuation belongs to the grammar.
			if c == '{' || c == '}' || c == ';' {
				return nil
			}

			// A selector or a property name.
			kind, _ := scanToBraceOrEnd(src, sI)
			if kind == selectorKind {
				end := scanSelectorEnd(src, sI)
				sel := strings.TrimSpace(stripComments(src[sI:end]))
				tkn := lex.Token("#TX", jsonic.TinTX, sel, src[sI:end])
				advance(pnt, src, sI, end)
				return tkn
			}
			eI := sI
			for eI < len(src) && isPropChar(src[eI]) {
				eI++
			}
			if eI == sI {
				return nil
			}
			prop := src[sI:eI]
			if lowercaseProperties {
				prop = strings.ToLower(prop)
			}
			tkn := lex.Token("#TX", jsonic.TinTX, prop, src[sI:eI])
			advance(pnt, src, sI, eI)
			return tkn
		}
	}
}

func matchAtRule(lex *jsonic.Lex, src string, sI int, tins cssTins) *jsonic.Token {
	pnt := lex.Cursor()
	kEnd := sI + 1
	for kEnd < len(src) && isAtChar(src[kEnd]) {
		kEnd++
	}
	kw := src[sI+1 : kEnd]

	kind, idx := scanToBraceOrEnd(src, sI)
	if kind == selectorKind {
		prelude := strings.TrimSpace(stripComments(src[kEnd:idx]))
		var tinName string
		var tin jsonic.Tin
		switch {
		case keyframesRe.MatchString(kw):
			tinName, tin = "#ATK", tins.atk
		case declsKw[kw]:
			tinName, tin = "#ATD", tins.atd
		default:
			tinName, tin = "#ATR", tins.atr
		}
		tkn := lex.Token(tinName, tin, kw, src[sI:idx])
		tkn.Use = map[string]any{"prelude": prelude}
		advance(pnt, src, sI, idx)
		return tkn
	}
	pEnd := scanValueEnd(src, kEnd)
	params := strings.TrimSpace(stripComments(src[kEnd:pEnd]))
	end := pEnd
	if pEnd < len(src) && src[pEnd] == ';' {
		end = pEnd + 1
	}
	tkn := lex.Token("#ATS", tins.ats, kw, src[sI:end])
	tkn.Use = map[string]any{"params": params}
	advance(pnt, src, sI, end)
	return tkn
}

func advance(pnt *jsonic.Point, src string, sI, end int) {
	rows := 0
	lastNL := -1
	for i := sI; i < end; i++ {
		if src[i] == '\n' {
			rows++
			lastNL = i
		}
	}
	if rows > 0 {
		pnt.RI += rows
		pnt.CI = end - lastNL
	} else {
		pnt.CI += end - sI
	}
	pnt.SI = end
}

// startPos is a token's first-character position (1-based line/column).
func startPos(tok *jsonic.Token) map[string]any {
	return map[string]any{"line": tok.RI, "column": tok.CI}
}

// endPos is the position just after a token's last character.
func endPos(tok *jsonic.Token) map[string]any {
	s := tok.Src
	rows := 0
	lastNL := -1
	for i := 0; i < len(s); i++ {
		if s[i] == '\n' {
			rows++
			lastNL = i
		}
	}
	if rows > 0 {
		return map[string]any{"line": tok.RI + rows, "column": len(s) - lastNL}
	}
	return map[string]any{"line": tok.RI, "column": tok.CI + len(s)}
}

const (
	selectorKind = 0
	declKind     = 1
)

func scanToBraceOrEnd(src string, i int) (int, int) {
	depth := 0
	for i < len(src) {
		c := src[i]
		if c == '"' || c == '\'' {
			i = skipString(src, i)
			continue
		}
		if c == '/' && i+1 < len(src) && src[i+1] == '*' {
			i = skipComment(src, i)
			continue
		}
		if c == '(' || c == '[' {
			depth++
		} else if c == ')' || c == ']' {
			if depth > 0 {
				depth--
			}
		} else if depth == 0 {
			if c == '{' {
				return selectorKind, i
			}
			if c == ';' || c == '}' {
				return declKind, i
			}
		}
		i++
	}
	return declKind, i
}

func scanSelectorEnd(src string, i int) int {
	depth := 0
	for i < len(src) {
		c := src[i]
		if c == '"' || c == '\'' {
			i = skipString(src, i)
			continue
		}
		if c == '/' && i+1 < len(src) && src[i+1] == '*' {
			i = skipComment(src, i)
			continue
		}
		if c == '(' || c == '[' {
			depth++
		} else if c == ')' || c == ']' {
			if depth > 0 {
				depth--
			}
		} else if depth == 0 && (c == ',' || c == '{') {
			return i
		}
		i++
	}
	return i
}

func scanValueEnd(src string, i int) int {
	depth := 0
	for i < len(src) {
		c := src[i]
		if c == '"' || c == '\'' {
			i = skipString(src, i)
			continue
		}
		if c == '/' && i+1 < len(src) && src[i+1] == '*' {
			i = skipComment(src, i)
			continue
		}
		if c == '(' || c == '[' {
			depth++
		} else if c == ')' || c == ']' {
			if depth > 0 {
				depth--
			}
		} else if depth == 0 && (c == ';' || c == '}') {
			return i
		}
		i++
	}
	return i
}

func skipString(src string, i int) int {
	q := src[i]
	i++
	for i < len(src) {
		if src[i] == '\\' {
			i += 2
			continue
		}
		if src[i] == q {
			return i + 1
		}
		i++
	}
	return i
}

func skipComment(src string, i int) int {
	i += 2
	for i+1 < len(src) && !(src[i] == '*' && src[i+1] == '/') {
		i++
	}
	return i + 2
}

// stripComments removes `/* ... */` comments from a selector / value run,
// leaving quoted strings untouched.
func stripComments(s string) string {
	if !strings.Contains(s, "/*") {
		return s
	}
	var b strings.Builder
	i := 0
	for i < len(s) {
		c := s[i]
		if c == '"' || c == '\'' {
			j := skipString(s, i)
			b.WriteString(s[i:j])
			i = j
			continue
		}
		if c == '/' && i+1 < len(s) && s[i+1] == '*' {
			i = skipComment(s, i)
			continue
		}
		b.WriteByte(c)
		i++
	}
	return b.String()
}

func isPropChar(c byte) bool {
	return (c >= '0' && c <= '9') ||
		(c >= 'A' && c <= 'Z') ||
		(c >= 'a' && c <= 'z') ||
		c == '-' || c == '_'
}

func isAtChar(c byte) bool {
	return (c >= '0' && c <= '9') ||
		(c >= 'A' && c <= 'Z') ||
		(c >= 'a' && c <= 'z') ||
		c == '-'
}

// --- Grammar text -> GrammarSpec ------------------------------------------

func parseGrammarText(text string, refs map[jsonic.FuncRef]any) (*jsonic.GrammarSpec, error) {
	parsed, err := jsonic.Make().Parse(text)
	if err != nil {
		return nil, fmt.Errorf("css: failed to parse grammar text: %w", err)
	}
	parsedMap, ok := parsed.(map[string]any)
	if !ok {
		return nil, fmt.Errorf("css: grammar text did not parse to a map")
	}
	gs := &jsonic.GrammarSpec{Ref: refs}
	ruleMap, ok := parsedMap["rule"].(map[string]any)
	if !ok {
		return gs, nil
	}
	gs.Rule = make(map[string]*jsonic.GrammarRuleSpec, len(ruleMap))
	for name, rDef := range ruleMap {
		rd, ok := rDef.(map[string]any)
		if !ok {
			continue
		}
		grs := &jsonic.GrammarRuleSpec{}
		if openDef, ok := rd["open"]; ok {
			grs.Open = buildGrammarAlts(openDef)
		}
		if closeDef, ok := rd["close"]; ok {
			grs.Close = buildGrammarAlts(closeDef)
		}
		gs.Rule[name] = grs
	}
	return gs, nil
}

func buildGrammarAlts(def any) []*jsonic.GrammarAltSpec {
	arr, ok := def.([]any)
	if !ok {
		return nil
	}
	alts := make([]*jsonic.GrammarAltSpec, 0, len(arr))
	for _, item := range arr {
		m, ok := item.(map[string]any)
		if !ok {
			alts = append(alts, &jsonic.GrammarAltSpec{})
			continue
		}
		ga := &jsonic.GrammarAltSpec{}
		if s, ok := m["s"]; ok {
			switch sv := s.(type) {
			case string:
				ga.S = sv
			case []any:
				strs := make([]string, len(sv))
				for i, v := range sv {
					strs[i], _ = v.(string)
				}
				ga.S = strs
			}
		}
		if b, ok := m["b"]; ok {
			switch bv := b.(type) {
			case float64:
				ga.B = int(bv)
			case int:
				ga.B = bv
			}
		}
		if p, ok := m["p"].(string); ok {
			ga.P = p
		}
		if r, ok := m["r"].(string); ok {
			ga.R = r
		}
		if a, ok := m["a"]; ok {
			switch av := a.(type) {
			case string:
				ga.A = jsonic.FuncRef(av)
			case []any:
				refs := make([]any, len(av))
				for i, v := range av {
					if s, ok := v.(string); ok {
						refs[i] = jsonic.FuncRef(s)
					} else {
						refs[i] = v
					}
				}
				ga.A = refs
			}
		}
		if g, ok := m["g"].(string); ok {
			ga.G = g
		}
		alts = append(alts, ga)
	}
	return alts
}

func toBool(v any) bool {
	b, _ := v.(bool)
	return b
}

func boolPtr(b bool) *bool {
	return &b
}
