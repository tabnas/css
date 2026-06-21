/* Copyright (c) 2025 Richard Rodger, MIT License */

// The engine is the tabnas parser; jsonic supplies the relaxed-JSON
// grammar whose fixed tokens (`{` `}` `:`) and machinery this plugin
// reuses, then reshapes into CSS.
import {
  Tabnas,
  Rule,
  Plugin,
  Config,
  TabnasOptions,
  Lex,
} from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'

// Plugin options.
type CssOptions = {
  // When true, lowercase declaration property names (CSS property names are
  // case-insensitive). Selectors are left untouched.
  lowercaseProperties: boolean
  // When true, lowercase declaration values. Off by default because parts of
  // a value (strings, url() contents, custom idents) are case-sensitive.
  lowercaseValues: boolean
}

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

// Plugin implementation.
const Css: Plugin = (tn: Tabnas, options: CssOptions) => {
  const lowercaseProperties = !!options.lowercaseProperties
  const lowercaseValues = !!options.lowercaseValues

  // Human descriptions for the CSS tokens, surfaced in railroad diagram
  // legends (read off the live config by @tabnas/railroad).
  tn.options({
    config: {
      modify: {
        'css-tokendesc': (cfg: any) => {
          cfg.tokenDesc = Object.assign(cfg.tokenDesc || {}, {
            '#OB': '{ — start of a block',
            '#CB': '} — end of a block',
            '#CL': ': — declaration separator',
            '#CA': '; — declaration terminator',
            '#TX': 'key: a selector or property name',
            '#VL': 'value: a declaration value (raw text)',
          })
        },
      },
    },
  })

  const grammarDef = new Tabnas().use(jsonic).parse(grammarText)
  // No grammar-local closures are needed; the rule alts use only builtin
  // ($) actions.
  grammarDef.ref = {}

  // All jsonic option overrides live on the grammar object so the plugin
  // applies them atomically alongside its rule alts.
  grammarDef.options = {
    rule: {
      // Remove jsonic extensions (implicit maps/lists, top-level commas,
      // path dives). CSS structure is supplied entirely by the rules above.
      exclude: 'jsonic,imp',
      start: 'stylesheet',
    },
    fixed: {
      token: {
        // `;` is the declaration terminator — remap the member-separator
        // token (#CA, jsonic's comma) onto it. `:` stays #CL.
        '#CA': ';',
        // Bare `[` `]` are not CSS structure; they only ever appear inside
        // selectors/values, which the cssToken matcher consumes as text.
        '#OS': null,
        '#CS': null,
      },
    },
    tokenSet: {
      // Keys are the text token produced by the cssToken matcher.
      KEY: ['#TX'],
    },
    // The cssToken matcher owns all non-fixed text (selectors, property
    // names, values), so the default string/number/text matchers are off.
    string: {
      chars: '',
    },
    number: {
      lex: false,
    },
    text: {
      lex: false,
    },
    value: {
      lex: false,
    },
    // Only `/* ... */` block comments in CSS.
    comment: {
      lex: true,
      def: {
        hash: { lex: false },
        slash: { lex: false },
        multi: { line: false, start: '/*', end: '*/', lex: true },
      },
    },
    lex: {
      match: {
        // Runs ahead of the fixed-token matcher so it owns selectors,
        // property names and values; it defers (returns undefined) on the
        // fixed punctuation and on whitespace/comments.
        cssToken: {
          order: 1e5,
          make: buildCssTokenMatcher(lowercaseProperties, lowercaseValues),
        },
      },
    },
  }

  // Tag every alt in this grammar with the 'css' group so callers can
  // selectively exclude css alts via `rule.exclude: 'css'`.
  tn.grammar(grammarDef, { rule: { alt: { g: 'css' } } })
}

// The single context-sensitive lex matcher. It uses the active rule to
// decide what to emit at the current source position:
//   - value mode  -> read a declaration value up to `;`/`}` and emit #VL.
//     Selected when the `val` rule is open, or when the previous key was a
//     statement at-keyword (flagged on `rule.u`).
//   - key mode    -> read a selector (up to `{`) or a property / at-keyword
//     (up to `:`/whitespace), chosen by lookahead, and emit #TX.
// Anything else (fixed punctuation, whitespace, comments) is deferred to
// the later builtin matchers. Using only `rule.name`/`rule.state` and a
// `rule.u` flag keeps this logic identical to the Go port, which cannot
// read the grammar's expected-token columns.
function buildCssTokenMatcher(
  lowercaseProperties: boolean,
  lowercaseValues: boolean,
) {
  return function makeCssTokenMatcher(_cfg: Config, _opts: TabnasOptions) {
    return function cssTokenMatcher(lex: Lex, rule: Rule) {
      const { pnt } = lex
      const src: string = lex.src as unknown as string
      const { sI, cI } = pnt
      const c = src[sI]

      // Defer whitespace and `/* */` comments to the builtin matchers.
      if (undefined === c) return undefined
      if (' ' === c || '\t' === c || '\r' === c || '\n' === c) return undefined
      if ('/' === c && '*' === src[sI + 1]) return undefined

      // The at-rule flag lives in the per-parse context bag (ctx.u), which is
      // stable across the stylesheet/pair/val rules (the key #TX may be lexed
      // under any of them) and isolated per parse — unlike rule.u.
      const ctx: any = (lex as any).ctx
      const bag: any = ctx.u || (ctx.u = {})
      const atValue = !!bag.cssAtValue
      const valueMode =
        atValue || ('val' === (rule as any).name && 'o' === (rule as any).state)

      // Value position: a value is read right after `:` (a declaration, where
      // the val rule is open) or after a statement at-keyword (the flag).
      if (valueMode) {
        // Fixed punctuation here belongs to the grammar, not a value.
        if ('{' === c || '}' === c || ';' === c || ':' === c) return undefined
        if (atValue) bag.cssAtValue = false
        const endI = scanValueEnd(src, sI)
        let val = src.substring(sI, endI).replace(/\s+$/, '')
        if (lowercaseValues) val = val.toLowerCase()
        const tkn = lex.token('#VL', val, src.substring(sI, endI), pnt)
        pnt.sI = endI
        pnt.cI = cI + (endI - sI)
        return tkn
      }

      // Key position (selector or property name). A selector may begin with
      // `:` (a pseudo-class), so `:` is NOT block punctuation here.
      if ('{' === c || '}' === c || ';' === c) return undefined
      const brace = scanToBraceOrEnd(src, sI)
      if (brace.kind === 'selector') {
        bag.cssAtValue = false
        const sel = src.substring(sI, brace.index).replace(/\s+$/, '')
        const tkn = lex.token('#TX', sel, src.substring(sI, brace.index), pnt)
        pnt.sI = brace.index
        pnt.cI = cI + (brace.index - sI)
        return tkn
      }
      // Declaration property or statement at-keyword: read the identifier up
      // to `:`, whitespace, `;` or `}`. A leading `@` marks a statement
      // at-rule, whose params are read as a value on the next call.
      let eI = sI
      const isAt = '@' === src[eI]
      if (isAt) eI++
      while (eI < src.length && isPropChar(src.charCodeAt(eI))) eI++
      if (eI === sI) return undefined
      bag.cssAtValue = isAt
      let name = src.substring(sI, eI)
      if (lowercaseProperties) name = name.toLowerCase()
      const tkn = lex.token('#TX', name, src.substring(sI, eI), pnt)
      pnt.sI = eI
      pnt.cI = cI + (eI - sI)
      return tkn
    }
  }
}

// Scan a key prelude: return where it ends and whether it is a selector (a
// top-level `{` was reached first) or a declaration (a top-level `;`/`}` or
// end-of-input was reached first). Strings, (), [] and comments are skipped.
function scanToBraceOrEnd(
  src: string,
  i: number,
): { kind: 'selector' | 'decl'; index: number } {
  let depth = 0
  while (i < src.length) {
    const c = src[i]
    if ('"' === c || '\'' === c) {
      i = skipString(src, i)
      continue
    }
    if ('/' === c && '*' === src[i + 1]) {
      i = skipComment(src, i)
      continue
    }
    if ('(' === c || '[' === c) {
      depth++
    } else if (')' === c || ']' === c) {
      if (depth > 0) depth--
    } else if (0 === depth) {
      if ('{' === c) return { kind: 'selector', index: i }
      if (';' === c || '}' === c) return { kind: 'decl', index: i }
    }
    i++
  }
  return { kind: 'decl', index: i }
}

// Scan a declaration value: return the index of the next top-level `;` or
// `}` (or end-of-input). Strings, (), [] and comments are skipped.
function scanValueEnd(src: string, i: number): number {
  let depth = 0
  while (i < src.length) {
    const c = src[i]
    if ('"' === c || '\'' === c) {
      i = skipString(src, i)
      continue
    }
    if ('/' === c && '*' === src[i + 1]) {
      i = skipComment(src, i)
      continue
    }
    if ('(' === c || '[' === c) {
      depth++
    } else if (')' === c || ']' === c) {
      if (depth > 0) depth--
    } else if (0 === depth && (';' === c || '}' === c)) {
      return i
    }
    i++
  }
  return i
}

// Skip a quoted string starting at the quote char; returns the index after
// the closing quote (honouring backslash escapes).
function skipString(src: string, i: number): number {
  const q = src[i]
  i++
  while (i < src.length) {
    if ('\\' === src[i]) {
      i += 2
      continue
    }
    if (src[i] === q) return i + 1
    i++
  }
  return i
}

// Skip a `/* ... */` comment starting at `/`; returns the index after `*/`.
function skipComment(src: string, i: number): number {
  i += 2
  while (i < src.length && !('*' === src[i] && '/' === src[i + 1])) i++
  return i + 2
}

// CSS property / at-keyword name characters.
function isPropChar(c: number): boolean {
  return (
    (48 <= c && c <= 57) || // 0-9
    (65 <= c && c <= 90) || // A-Z
    (97 <= c && c <= 122) || // a-z
    45 === c || // -
    95 === c // _
  )
}

// Default option values.
Css.defaults = {
  lowercaseProperties: false,
  lowercaseValues: false,
} as CssOptions

export { Css }
export type { CssOptions }
