/* Copyright (c) 2025 Richard Rodger, MIT License */

// The engine is the tabnas parser; jsonic supplies the relaxed-JSON
// grammar whose fixed tokens (`{` `}` `:`) and machinery this plugin
// reuses, then reshapes into CSS.
import {
  Tabnas,
  Rule,
  Context,
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
# The custom cssToken lex matcher emits three text tokens by position:
#   - #TX : a key in key position — a selector (whole prelude up to "{",
#           when a "{" is reached before any ";"/"}") or a property name
#           (the identifier up to ":", otherwise).
#   - #AT : a statement at-keyword (e.g. "@import") — a key whose value
#           follows without a ":" separator.
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

    # A member of a map. Three shapes, disambiguated by the key token and
    # what follows it: #TX ":" -> declaration, #TX "{" -> nested ruleset,
    # #AT -> statement at-rule. In every case the value side is the val rule,
    # so the matcher reads a value purely because val is open (no flag).
    # @key$ captures the key; @cssSetval assigns the built value, expanding a
    # comma-grouped selector key (e.g. "h1, h2") into one entry per selector.
    pair: {
      open: [
        # Declaration:  property : value
        { s: '#TX #CL' a: '@key$' p: val g: 'css,decl' }
        # Ruleset:  selector { ... }   (b:1 re-feeds "{" to the val/block).
        { s: '#TX #OB' b: 1 a: '@key$' p: val g: 'css,rule' }
        # Statement at-rule:  @import "x"   The at-keyword (#AT) pushes val
        # directly; val then reads the params as a value (#VL).
        { s: '#AT' a: '@key$' p: val g: 'css,atrule' }
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
            '#AT': 'key: a statement at-keyword (e.g. @import)',
            '#VL': 'value: a declaration value (raw text)',
          })
        },
      },
    },
  })

  const grammarDef = new Tabnas().use(jsonic).parse(grammarText)
  // @cssSetval is the one grammar-local action: it assigns a pair's built
  // value into the enclosing map, expanding a comma-grouped selector key
  // (e.g. `h1, h2`) into a separate entry per selector. Everything else uses
  // builtin ($) actions.
  grammarDef.ref = { '@cssSetval': cssSetval }

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
      // Keys are the text tokens produced by the cssToken matcher: a
      // selector / property name (#TX) or a statement at-keyword (#AT).
      KEY: ['#TX', '#AT'],
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

// The single lex matcher. It emits one of three text tokens by position;
// all the structural decisions live in the grammar:
//   - value mode (the `val` rule is open) -> read a declaration value up to
//     `;`/`}` and emit #VL.
//   - key mode (otherwise) -> a selector / block-at-rule prelude up to `{`
//     (#TX), a property name up to `:` (#TX), or a statement at-keyword
//     (#AT), chosen by a single lookahead.
// Anything else (fixed punctuation, whitespace, comments) is deferred to the
// later builtin matchers. The matcher is stateless: value position is read
// straight off `rule.name`/`rule.state` because the grammar always pushes
// `val` at a value position (after `:`, or after an #AT key). This keeps the
// logic identical to the Go port, which cannot read the grammar's
// expected-token columns or inject lookahead tokens.
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

      // Value mode is driven entirely by the grammar: the val rule is open
      // exactly at a value position (after a `:` declaration separator, or
      // after a statement at-keyword pushes it). No flag or lookbehind is
      // needed — every value is read under val.
      if ('val' === (rule as any).name && 'o' === (rule as any).state) {
        // Fixed punctuation here belongs to the grammar, not a value.
        if ('{' === c || '}' === c || ';' === c || ':' === c) return undefined
        const endI = scanValueEnd(src, sI)
        let val = src.substring(sI, endI).replace(/\s+$/, '')
        if (lowercaseValues) val = val.toLowerCase()
        const tkn = lex.token('#VL', val, src.substring(sI, endI), pnt)
        pnt.sI = endI
        pnt.cI = cI + (endI - sI)
        return tkn
      }

      // Key position. A selector may begin with `:` (a pseudo-class), so `:`
      // is NOT block punctuation here.
      if ('{' === c || '}' === c || ';' === c) return undefined
      const brace = scanToBraceOrEnd(src, sI)
      if (brace.kind === 'selector') {
        // A selector or block at-rule prelude: the whole run up to `{`.
        const sel = src.substring(sI, brace.index).replace(/\s+$/, '')
        const tkn = lex.token('#TX', sel, src.substring(sI, brace.index), pnt)
        pnt.sI = brace.index
        pnt.cI = cI + (brace.index - sI)
        return tkn
      }
      // A property name or a statement at-keyword: the identifier up to `:`,
      // whitespace, `;` or `}`. A leading `@` makes it an at-keyword (#AT),
      // which the grammar follows directly with a value; otherwise #TX.
      let eI = sI
      const isAt = '@' === src[eI]
      if (isAt) eI++
      while (eI < src.length && isPropChar(src.charCodeAt(eI))) eI++
      if (eI === sI) return undefined
      let name = src.substring(sI, eI)
      if (lowercaseProperties) name = name.toLowerCase()
      const tkn = lex.token(isAt ? '#AT' : '#TX', name, name, pnt)
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

// Grammar action: assign the pair's built value (r.child.node) into the
// enclosing map under the captured key. A comma-grouped selector key like
// `h1, h2` is expanded into one entry per selector, each with its own copy of
// the value (so the entries are independent). At-rule preludes (e.g.
// `@media screen, print`) are left intact — their commas are a media-query
// list, not a selector group — as are keys with no top-level comma.
function cssSetval(r: Rule, _ctx: Context) {
  const node: any = r.node
  if (null == node || 'object' !== typeof node) return
  const key: any = r.u.key
  const val = r.child.node
  if ('string' === typeof key && '@' !== key[0] && -1 < key.indexOf(',')) {
    const sels = splitSelectors(key)
    if (1 < sels.length) {
      for (let i = 0; i < sels.length; i++) {
        node[sels[i]] = 0 === i ? val : cloneNode(val)
      }
      return
    }
  }
  node[key] = val
}

// Split a selector group on top-level commas, skipping commas inside strings,
// `()`/`[]` and comments (so `:not(.a, .b)` stays one selector). Each part is
// trimmed; empty parts are dropped.
function splitSelectors(s: string): string[] {
  const out: string[] = []
  let depth = 0
  let start = 0
  let i = 0
  while (i < s.length) {
    const c = s[i]
    if ('"' === c || '\'' === c) {
      i = skipString(s, i)
      continue
    }
    if ('/' === c && '*' === s[i + 1]) {
      i = skipComment(s, i)
      continue
    }
    if ('(' === c || '[' === c) depth++
    else if (')' === c || ']' === c) {
      if (depth > 0) depth--
    } else if (0 === depth && ',' === c) {
      out.push(s.substring(start, i).trim())
      start = i + 1
    }
    i++
  }
  out.push(s.substring(start).trim())
  return out.filter((p) => '' !== p)
}

// Deep-copy a parsed value (plain map / array / scalar) so grouped selectors
// don't share a mutable value object. Maps are rebuilt null-prototype to
// match the engine's output.
function cloneNode(v: any): any {
  if (null == v || 'object' !== typeof v) return v
  if (Array.isArray(v)) return v.map(cloneNode)
  const out: any = Object.create(null)
  for (const k of Object.keys(v)) out[k] = cloneNode(v[k])
  return out
}

// Default option values.
Css.defaults = {
  lowercaseProperties: false,
  lowercaseValues: false,
} as CssOptions

export { Css }
export type { CssOptions }
