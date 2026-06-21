/* Copyright (c) 2025 Richard Rodger, MIT License */

// Package css is a jsonic plugin that parses CSS (Cascading Style Sheets)
// into a nested map of selector -> { property -> value }.
//
// Example:
//
//	a { color: red; font-size: 12px; }
//	.foo, .bar { margin: 0 }
//	@media screen { a { color: blue } }
//
// parses to:
//
//	{
//	  "a": { "color": "red", "font-size": "12px" },
//	  ".foo, .bar": { "margin": "0" },
//	  "@media screen": { "a": { "color": "blue" } }
//	}
package tabnascss

import (
	"fmt"
	"strings"
	"sync"

	jsonic "github.com/tabnas/jsonic/go"
)

const Version = "0.1.0"

// --- BEGIN EMBEDDED css-grammar.jsonic ---
const grammarText = `
# CSS Grammar Definition
# Parses CSS (Cascading Style Sheets) into a nested map of
#   selector -> { property -> value }
# Nested at-rules (e.g. @media) recurse: their block is itself a map of
# rules. Statement at-rules (e.g. @import) become property -> value pairs.
#
# Example:
#   a { color: red; font-size: 12px; }
#   .foo, .bar { margin: 0 }
#   @media screen { a { color: blue } }
# parses to:
#   {
#     "a": { "color": "red", "font-size": "12px" },
#     ".foo, .bar": { "margin": "0" },
#     "@media screen": { "a": { "color": "blue" } }
#   }
#
# The custom cssToken lex matcher is context-sensitive: it inspects the
# active rule's expected-token columns to decide what to emit.
#   - At a key position it emits #TX, peeking ahead to choose between a
#     selector (a "{" is reached first -> the whole prelude, trimmed) and a
#     property name (a ";"/"}" is reached first -> the identifier up to ":").
#   - At a value position it emits #VL: the run of text up to the next
#     top-level ";" or "}" (trimmed), so '1px solid #fff' is one value.
# The fixed tokens "{" "}" ":" lex as #OB #CB #CL; ";" is remapped to #CA
# (the member separator). Bare "[" "]" are disabled.
#
# The grammar is applied with { rule: { alt: { g: 'css' } } } so every alt
# below is automatically tagged with the 'css' group.

{
  rule: {
    # Start rule: a stylesheet is an implicit top-level map of rules, with
    # no surrounding braces. It is closed by end-of-input (#ZZ).
    stylesheet: {
      open: [
        # Empty input -> empty stylesheet.
        { s: '#ZZ' b: 1 a: '@object$' g: 'css,sheet,empty' }
        # Otherwise the first key (#TX) starts the rule list. b:1 re-feeds
        # the key to the pair rule.
        { s: '#TX' b: 1 a: '@object$' p: pair g: 'css,sheet' }
      ]
      close: [
        { s: '#ZZ' g: 'css,sheet,end' }
      ]
    }

    # An explicit "{ ... }" block: a declaration block or a nested ruleset.
    block: {
      open: [
        # Empty block: {}.
        { s: '#OB #CB' b: 1 a: '@object$' g: 'css,block,empty' }
        { s: '#OB' a: '@object$' p: pair g: 'css,block' }
      ]
      close: [
        { s: '#CB' g: 'css,block,end' }
      ]
    }

    # A member of a map. Three shapes, disambiguated by the token after the
    # key: ":" -> declaration, "{" -> nested ruleset, value -> statement
    # at-rule. @key$ captures the key for the matching @setval$.
    pair: {
      open: [
        # Declaration:  property : value
        { s: '#TX #CL' a: '@key$' p: val g: 'css,decl' }
        # Ruleset:  selector { ... }   (b:1 re-feeds "{" to the val/block).
        { s: '#TX #OB' b: 1 a: '@key$' p: val g: 'css,rule' }
        # Statement at-rule:  @import "x"   The cssToken matcher emits the
        # at-keyword (#TX) then, via a rule flag, the params as a value
        # (#VL); b:1 re-feeds the value to the val rule.
        { s: '#TX #VL' b: 1 a: '@key$' p: val g: 'css,atrule' }
      ]
      close: [
        # Trailing ";" before "}" -> end of block (re-fed to block close).
        { s: '#CA #CB' b: 1 a: '@setval$' g: 'css,decl,trailing' }
        # Trailing ";" before end-of-input -> end of stylesheet.
        { s: '#CA #ZZ' b: 1 a: '@setval$' g: 'css,decl,trailing,end' }
        # ";" -> next declaration in the same block.
        { s: '#CA' a: '@setval$' r: pair g: 'css,decl,next' }
        # "}" -> end of the enclosing block (re-fed to block close).
        { s: '#CB' b: 1 a: '@setval$' g: 'css,pair,endblock' }
        # End of input -> end of the stylesheet.
        { s: '#ZZ' b: 1 a: '@setval$' g: 'css,pair,endsheet' }
        # A new key with no separator -> next ruleset (implicit continuation).
        { s: '#TX' b: 1 a: '@setval$' r: pair g: 'css,rule,next' }
      ]
    }

    # The value side of a pair: either a nested block (map) or a value token.
    # @reset$ clears the parent-seeded node so the value does not inherit the
    # enclosing object; @value$ resolves it (a built block wins, else the
    # #VL scalar).
    val: {
      open: [
        { s: '#OB' b: 1 a: '@reset$' p: block g: 'css,val,block' }
        { s: '#VL' a: '@reset$' g: 'css,val,text' }
      ]
      close: [
        { s: '#ZZ' b: 1 a: '@value$' g: 'css,val,endsheet' }
        { b: 1 a: '@value$' g: 'css,val,more' }
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
	lowercaseValues := toBool(options["lowercaseValues"])

	// No grammar-local closures are needed; the rule alts use only builtin
	// ($) actions.
	gs, err := parseGrammarText(grammarText, map[jsonic.FuncRef]any{})
	if err != nil {
		return err
	}

	// All jsonic option overrides live on the grammar object so the plugin
	// applies them atomically alongside its rule alts.
	semi := ";"
	gs.Options = &jsonic.Options{
		Rule: &jsonic.RuleOptions{
			// Remove jsonic extensions (implicit maps/lists, top-level commas,
			// path dives). CSS structure is supplied entirely by the rules.
			Exclude: "jsonic,imp",
			Start:   "stylesheet",
		},
		Fixed: &jsonic.FixedOptions{
			Token: map[string]*string{
				// `;` is the declaration terminator — remap the member
				// separator (#CA, jsonic's comma) onto it. `:` stays #CL.
				"#CA": &semi,
				// Bare `[` `]` are not CSS structure; they only ever appear
				// inside selectors/values, consumed by cssToken as text.
				"#OS": nil,
				"#CS": nil,
			},
		},
		TokenSet: map[string][]string{
			// Keys are the text token produced by the cssToken matcher.
			"KEY": {"#TX"},
		},
		// The cssToken matcher owns all non-fixed text (selectors, property
		// names, values), so the default string/number/text matchers are off.
		String: &jsonic.StringOptions{
			Chars: "",
		},
		Number: &jsonic.NumberOptions{
			Lex: boolPtr(false),
		},
		Text: &jsonic.TextOptions{
			Lex: boolPtr(false),
		},
		Value: &jsonic.ValueOptions{
			Lex: boolPtr(false),
		},
		// Only `/* ... */` block comments in CSS.
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
				// Runs ahead of the fixed-token matcher so it owns selectors,
				// property names and values; it defers on the fixed
				// punctuation and on whitespace/comments.
				"cssToken": {Order: 100000, Make: buildCssTokenMatcher(lowercaseProperties, lowercaseValues)},
			},
		},
	}

	// Tag every alt in this grammar with the 'css' group so callers can
	// selectively exclude css alts via rule.exclude.
	setting := &jsonic.GrammarSetting{
		Rule: &jsonic.GrammarSettingRule{
			Alt: &jsonic.GrammarSettingAlt{G: "css"},
		},
	}
	if err := j.Grammar(gs, setting); err != nil {
		return fmt.Errorf("css: failed to apply grammar: %w", err)
	}

	return nil
}

// Defaults matches the TS Css.defaults. Used with jsonic.UseDefaults.
var Defaults = map[string]any{
	"lowercaseProperties": false,
	"lowercaseValues":     false,
}

// CssOptions is a typed wrapper for the plugin options.
// Fields are pointers so callers can express "omit" (nil) vs "set".
type CssOptions struct {
	// LowercaseProperties, when true, lowercases declaration property names
	// (CSS property names are case-insensitive). Selectors are untouched.
	LowercaseProperties *bool
	// LowercaseValues, when true, lowercases declaration values. Off by
	// default because parts of a value are case-sensitive.
	LowercaseValues *bool
}

func (o CssOptions) toMap() map[string]any {
	m := map[string]any{}
	if o.LowercaseProperties != nil {
		m["lowercaseProperties"] = *o.LowercaseProperties
	}
	if o.LowercaseValues != nil {
		m["lowercaseValues"] = *o.LowercaseValues
	}
	return m
}

// MakeJsonic returns a reusable Jsonic instance configured for CSS parsing.
// Use this when parsing multiple CSS strings with the same options.
func MakeJsonic(opts ...CssOptions) *jsonic.Jsonic {
	j := jsonic.Make()
	var m map[string]any
	if len(opts) > 0 {
		m = opts[0].toMap()
	}
	if err := j.UseDefaults(Css, Defaults, m); err != nil {
		// Plugin registration errors are programming errors with static
		// inputs; surface them via panic rather than silent misbehavior.
		panic(fmt.Sprintf("css: plugin initialisation failed: %v", err))
	}
	return j
}

// defaultParser is a lazily-created instance reused by the default (no-option)
// Parse path, so repeated calls don't rebuild the engine and grammar each
// time. Parsing builds a fresh context per call and only reads instance
// state, so the shared instance is safe for concurrent use.
var (
	defaultOnce   sync.Once
	defaultParser *jsonic.Jsonic
)

// Parse parses a CSS string and returns the resulting value. Convenience
// wrapper around MakeJsonic(opts...).Parse(src).
func Parse(src string, opts ...CssOptions) (any, error) {
	if len(opts) == 0 {
		defaultOnce.Do(func() { defaultParser = MakeJsonic() })
		return defaultParser.Parse(src)
	}
	return MakeJsonic(opts...).Parse(src)
}

// The single context-sensitive lex matcher. It uses the active rule to
// decide what to emit at the current source position:
//
//   - value mode -> read a declaration value up to `;`/`}` and emit #VL.
//     Selected when the val rule is open, or when the previous key was a
//     statement at-keyword (flagged in the per-parse context bag, ctx.U).
//   - key mode   -> read a selector (up to `{`) or a property / at-keyword
//     (up to `:`/whitespace), chosen by lookahead, and emit #TX.
//
// Anything else (fixed punctuation, whitespace, comments) is deferred to the
// later builtin matchers. Using only rule.Name/rule.State and a ctx.U flag
// keeps this logic identical to the TS plugin: an external package can read
// neither the grammar's expected-token columns nor inject lookahead tokens.
func buildCssTokenMatcher(lowercaseProperties, lowercaseValues bool) jsonic.MakeLexMatcher {
	return func(_ *jsonic.LexConfig, _ *jsonic.Options) jsonic.LexMatcher {
		return func(lex *jsonic.Lex, rule *jsonic.Rule) *jsonic.Token {
			pnt := lex.Cursor()
			src := lex.Src
			sI := pnt.SI
			if sI >= len(src) {
				return nil
			}
			c := src[sI]

			// Defer whitespace and `/* */` comments to the builtin matchers.
			if c == ' ' || c == '\t' || c == '\r' || c == '\n' {
				return nil
			}
			if c == '/' && sI+1 < len(src) && src[sI+1] == '*' {
				return nil
			}

			// The at-rule flag lives in the per-parse context bag (ctx.U),
			// which is stable across the stylesheet/pair/val rules (the key
			// #TX may be lexed under any of them) and isolated per parse.
			ctx := lex.Ctx
			if ctx.U == nil {
				ctx.U = map[string]any{}
			}
			atValue, _ := ctx.U["cssAtValue"].(bool)
			valueMode := atValue || (rule.Name == "val" && rule.State == jsonic.OPEN)

			// Value position: a value is read right after `:` (a declaration,
			// where the val rule is open) or after a statement at-keyword.
			if valueMode {
				if c == '{' || c == '}' || c == ';' || c == ':' {
					return nil
				}
				if atValue {
					ctx.U["cssAtValue"] = false
				}
				endI := scanValueEnd(src, sI)
				raw := src[sI:endI]
				val := strings.TrimRight(raw, " \t\r\n")
				if lowercaseValues {
					val = strings.ToLower(val)
				}
				tkn := lex.Token("#VL", jsonic.TinVL, val, raw)
				pnt.SI = endI
				pnt.CI += endI - sI
				return tkn
			}

			// Key position (selector or property name). A selector may begin
			// with `:` (a pseudo-class), so `:` is not block punctuation here.
			if c == '{' || c == '}' || c == ';' {
				return nil
			}
			kind, idx := scanToBraceOrEnd(src, sI)
			if kind == selectorKind {
				ctx.U["cssAtValue"] = false
				raw := src[sI:idx]
				sel := strings.TrimRight(raw, " \t\r\n")
				tkn := lex.Token("#TX", jsonic.TinTX, sel, raw)
				pnt.SI = idx
				pnt.CI += idx - sI
				return tkn
			}
			// Declaration property or statement at-keyword: read the identifier
			// up to `:`, whitespace, `;` or `}`. A leading `@` marks a statement
			// at-rule, whose params are read as a value on the next call.
			eI := sI
			isAt := src[eI] == '@'
			if isAt {
				eI++
			}
			for eI < len(src) && isPropChar(src[eI]) {
				eI++
			}
			if eI == sI {
				return nil
			}
			ctx.U["cssAtValue"] = isAt
			name := src[sI:eI]
			if lowercaseProperties {
				name = strings.ToLower(name)
			}
			tkn := lex.Token("#TX", jsonic.TinTX, name, name)
			pnt.SI = eI
			pnt.CI += eI - sI
			return tkn
		}
	}
}

const (
	selectorKind = 0
	declKind     = 1
)

// scanToBraceOrEnd scans a key prelude: it returns where it ends and whether
// it is a selector (a top-level `{` was reached first) or a declaration (a
// top-level `;`/`}` or end-of-input was reached first). Strings, (), [] and
// comments are skipped.
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

// scanValueEnd returns the index of the next top-level `;` or `}` (or
// end-of-input). Strings, (), [] and comments are skipped.
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

// skipString skips a quoted string starting at the quote char; returns the
// index after the closing quote (honouring backslash escapes).
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

// skipComment skips a `/* ... */` comment starting at `/`; returns the index
// after `*/`.
func skipComment(src string, i int) int {
	i += 2
	for i < len(src) && !(src[i] == '*' && i+1 < len(src) && src[i+1] == '/') {
		i++
	}
	return i + 2
}

// isPropChar reports whether c is a CSS property / at-keyword name character.
func isPropChar(c byte) bool {
	return (c >= '0' && c <= '9') ||
		(c >= 'A' && c <= 'Z') ||
		(c >= 'a' && c <= 'z') ||
		c == '-' || c == '_'
}

// parseGrammarText parses grammar text into a GrammarSpec with refs attached.
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

// buildGrammarAlts converts a parsed-jsonic alt array into []*GrammarAltSpec.
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
		if a, ok := m["a"].(string); ok {
			ga.A = jsonic.FuncRef(a)
		}
		if c, ok := m["c"]; ok {
			switch cv := c.(type) {
			case string:
				ga.C = cv
			case map[string]any:
				ga.C = cv
			}
		}
		if u, ok := m["u"].(map[string]any); ok {
			ga.U = u
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
