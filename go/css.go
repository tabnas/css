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
//	  ".foo": { "margin": "0" },
//	  ".bar": { "margin": "0" },
//	  "@media screen": { "a": { "color": "blue" } }
//	}
//
// A comma-grouped selector is expanded into one entry per selector.
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
# A comma-grouped selector is expanded into one entry per selector.
#
# Example:
#   a { color: red; font-size: 12px; }
#   .foo, .bar { margin: 0 }
#   @media screen { a { color: blue } }
# parses to:
#   {
#     "a": { "color": "red", "font-size": "12px" },
#     ".foo": { "margin": "0" },
#     ".bar": { "margin": "0" },
#     "@media screen": { "a": { "color": "blue" } }
#   }
#
# The custom cssToken lex matcher emits the text tokens by position:
#   - #TX : a key in key position — one selector (up to a top-level "," or
#           "{") or a property name (the identifier up to ":").
#   - #AT : a statement at-keyword (e.g. "@import") — a key whose value
#           follows without a ":" separator.
#   - #GC : a top-level comma separating the selectors of a group (so each
#           selector arrives as its own #TX key, never a split string).
#   - #VL : a value, in value position (when the val rule is open): the run
#           of text up to the next top-level ";" or "}" (trimmed), so
#           '1px solid #fff' is one value.
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
        # Otherwise the first key (#KEY = #TX or #AT) starts the rule list.
        # b:1 re-feeds the key to the pair rule.
        { s: '#KEY' b: 1 a: '@object$' p: pair g: 'css,sheet' }
      ]
      close: [
        { s: '#ZZ' g: 'css,sheet,end' }
      ]
    }

    # An explicit "{ ... }" block: a declaration block or a nested ruleset.
    # @cssClearPend gives the block a fresh (empty) pending-key list so the
    # enclosing ruleset's selectors don't leak into this block's members.
    block: {
      open: [
        # Empty block: {}.
        { s: '#OB #CB' b: 1 a: '@object$' g: 'css,block,empty' }
        { s: '#OB' a: ['@object$' '@cssClearPend'] p: pair g: 'css,block' }
      ]
      close: [
        { s: '#CB' g: 'css,block,end' }
      ]
    }

    # A member of a map. Four open shapes, disambiguated by the key token and
    # what follows it:
    #   #TX ":"  -> declaration       (single key, captured with @key$)
    #   #AT      -> statement at-rule  (single key, captured with @key$)
    #   #TX ","  -> grouped selector   (pend this key, loop for the next)
    #   #TX "{"  -> ruleset (last/only selector: pend the key, push the block)
    # A selector group accumulates its keys on the kept "pend" list via
    # @cssPendKey across the open-phase "r: pair" loop; @cssSetval then assigns
    # the built value to every pending key (or to the single @key$ key for a
    # declaration / at-rule). No selector text is split — each selector arrives
    # as its own #TX token, with #GC commas between them.
    pair: {
      open: [
        # Declaration:  property : value
        { s: '#TX #CL' a: '@key$' p: val g: 'css,decl' }
        # Statement at-rule:  @import "x"   The at-keyword (#AT) pushes val
        # directly; val then reads the params as a value (#VL).
        { s: '#AT' a: '@key$' p: val g: 'css,atrule' }
        # Grouped selector:  selector ,   Pend the key and loop for the next
        # selector (k.pend propagates across the replace).
        { s: '#TX #GC' a: '@cssPendKey' r: pair g: 'css,rule,group' }
        # Ruleset (last/only selector):  selector { ... }   Pend the key, then
        # push val (b:1 re-feeds "{" to the val/block).
        { s: '#TX #OB' b: 1 a: '@cssPendKey' p: val g: 'css,rule' }
      ]
      close: [
        # Trailing ";" before "}" -> end of block (re-fed to block close).
        { s: '#CA #CB' b: 1 a: '@cssSetval' g: 'css,decl,trailing' }
        # Trailing ";" before end-of-input -> end of stylesheet.
        { s: '#CA #ZZ' b: 1 a: '@cssSetval' g: 'css,decl,trailing,end' }
        # ";" -> next declaration in the same block.
        { s: '#CA' a: '@cssSetval' r: pair g: 'css,decl,next' }
        # "}" -> end of the enclosing block (re-fed to block close).
        { s: '#CB' b: 1 a: '@cssSetval' g: 'css,pair,endblock' }
        # End of input -> end of the stylesheet.
        { s: '#ZZ' b: 1 a: '@cssSetval' g: 'css,pair,endsheet' }
        # A new key (#TX or #AT) with no separator -> next rule (implicit
        # continuation, e.g. between adjacent rulesets).
        { s: '#KEY' b: 1 a: '@cssSetval' r: pair g: 'css,rule,next' }
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

	// Resolve the tin for the custom statement-at-keyword token (#AT) on this
	// instance, so the matcher emits the same tin the grammar's `#AT` alts
	// resolve to.
	atTin := j.Token("#AT")
	gcTin := j.Token("#GC")

	// Three grammar-local actions handle selector grouping structurally (no
	// string splitting): @cssPendKey accumulates each grouped selector token
	// as a pending key, @cssSetval assigns the built value to every pending
	// key, and @cssClearPend resets the pending list when entering a block so
	// an enclosing ruleset's selectors don't leak in. Everything else uses
	// builtin ($) actions.
	gs, err := parseGrammarText(grammarText, map[jsonic.FuncRef]any{
		"@cssPendKey":   jsonic.AltAction(cssPendKey),
		"@cssSetval":    jsonic.AltAction(cssSetval),
		"@cssClearPend": jsonic.AltAction(cssClearPend),
	})
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
			// Keys are the text tokens produced by the cssToken matcher: a
			// selector / property name (#TX) or a statement at-keyword (#AT).
			"KEY": {"#TX", "#AT"},
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
				"cssToken": {Order: 100000, Make: buildCssTokenMatcher(lowercaseProperties, lowercaseValues, atTin, gcTin)},
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

// The single lex matcher. It emits one of three text tokens by position; all
// the structural decisions live in the grammar:
//
//   - value mode (the val rule is open) -> read a declaration value up to
//     `;`/`}` and emit #VL.
//   - key mode (otherwise) -> a selector / block-at-rule prelude up to `{`
//     (#TX), a property name up to `:` (#TX), or a statement at-keyword
//     (#AT), chosen by a single lookahead.
//
// Anything else (fixed punctuation, whitespace, comments) is deferred to the
// later builtin matchers. The matcher is stateless: value position is read
// straight off rule.Name/rule.State because the grammar always pushes val at
// a value position (after `:`, or after an #AT key). This keeps the logic
// identical to the TS plugin, which likewise cannot read the grammar's
// expected-token columns or inject lookahead tokens.
func buildCssTokenMatcher(lowercaseProperties, lowercaseValues bool, atTin, gcTin jsonic.Tin) jsonic.MakeLexMatcher {
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

			// Value mode is driven entirely by the grammar: the val rule is
			// open exactly at a value position (after a `:` declaration
			// separator, or after a statement at-keyword pushes it). No flag
			// or lookbehind is needed.
			if rule.Name == "val" && rule.State == jsonic.OPEN {
				if c == '{' || c == '}' || c == ';' || c == ':' {
					return nil
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

			// Key position. A top-level comma separates the selectors of a
			// group (`h1, h2`); emit it as a #GC token so the grammar can
			// collect each selector as its own key. (Commas inside selectors /
			// values / at-rule preludes never reach here — they are consumed
			// as text below.)
			if c == ',' {
				tkn := lex.Token("#GC", gcTin, ",", ",")
				pnt.SI = sI + 1
				pnt.CI++
				return tkn
			}
			// A selector may begin with `:` (a pseudo-class), so `:` is not
			// block punctuation here.
			if c == '{' || c == '}' || c == ';' {
				return nil
			}
			kind, idx := scanToBraceOrEnd(src, sI)
			if kind == selectorKind {
				// A single selector (one member of a possible group) ends at
				// the next top-level `,` or `{`. A block at-rule prelude
				// (`@media …`) is kept whole up to `{` — its commas are a
				// media-query list, not a group.
				end := idx
				if src[sI] != '@' {
					end = scanSelectorEnd(src, sI)
				}
				raw := src[sI:end]
				sel := strings.TrimRight(raw, " \t\r\n")
				tkn := lex.Token("#TX", jsonic.TinTX, sel, raw)
				pnt.SI = end
				pnt.CI += end - sI
				return tkn
			}
			// A property name or a statement at-keyword: the identifier up to
			// `:`, whitespace, `;` or `}`. A leading `@` makes it an at-keyword
			// (#AT), which the grammar follows directly with a value; else #TX.
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
			name := src[sI:eI]
			if lowercaseProperties {
				name = strings.ToLower(name)
			}
			if isAt {
				tkn := lex.Token("#AT", atTin, name, name)
				pnt.SI = eI
				pnt.CI += eI - sI
				return tkn
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

// scanSelectorEnd returns the index of the next top-level `,` (a group
// separator) or `{` (or end-of-input). Strings, (), [] and comments are
// skipped, so a comma inside `:not(.a, .b)` is part of the selector.
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

// cssPendKey records the just-matched key token (a selector of a group) onto
// the rule's kept "pend" list. K propagates across the open-phase `r: pair`
// loop, so each selector in `h1, h2, …` accumulates a pending key.
func cssPendKey(r *jsonic.Rule, _ *jsonic.Context) {
	if r.O0 == nil {
		return
	}
	sel, _ := r.O0.Val.(string)
	pend, _ := r.K["pend"].([]any)
	r.K["pend"] = append(pend, sel)
}

// cssSetval assigns the pair's built value (r.Child.Node) into the enclosing
// map. A ruleset has collected one or more keys on K["pend"] (one per grouped
// selector); the value is assigned to each, with its own deep copy so the
// entries stay independent. A declaration / statement at-rule has a single
// captured key in U["key"]. No selector text is parsed or split here — the
// keys arrived as separate tokens from the lexer.
func cssSetval(r *jsonic.Rule, _ *jsonic.Context) {
	if r.Child == nil {
		return
	}
	m, ok := r.Node.(map[string]any)
	if !ok {
		return
	}
	val := r.Child.Node
	if pend, ok := r.K["pend"].([]any); ok && len(pend) > 0 {
		for i, k := range pend {
			sel, _ := k.(string)
			if i == 0 {
				m[sel] = val
			} else {
				m[sel] = cloneNode(val)
			}
		}
		r.K["pend"] = []any{} // reset for the next member at this level
		return
	}
	key, _ := r.U["key"].(string)
	m[key] = val
}

// cssClearPend resets the pending-key list for a freshly opened block. The
// list is given a NEW slice (not cleared in place), so the enclosing ruleset's
// own K["pend"] — shared up the stack — keeps its selectors.
func cssClearPend(r *jsonic.Rule, _ *jsonic.Context) {
	r.K["pend"] = []any{}
}

// cloneNode deep-copies a parsed value (map / slice / scalar) so grouped
// selectors don't share a mutable value.
func cloneNode(v any) any {
	switch t := v.(type) {
	case map[string]any:
		out := make(map[string]any, len(t))
		for k, vv := range t {
			out[k] = cloneNode(vv)
		}
		return out
	case []any:
		out := make([]any, len(t))
		for i, vv := range t {
			out[i] = cloneNode(vv)
		}
		return out
	default:
		return v
	}
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
		if a, ok := m["a"]; ok {
			switch av := a.(type) {
			case string:
				ga.A = jsonic.FuncRef(av)
			case []any:
				// An ordered list of action refs, e.g. ['@object$' '@cssClearPend'].
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
