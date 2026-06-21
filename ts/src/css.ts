/* Copyright (c) 2025 Richard Rodger, MIT License */

// The engine is the tabnas parser; jsonic supplies the relaxed-JSON grammar
// whose fixed tokens (`{` `}` `:`) and machinery this plugin reuses, then
// reshapes into a CSS abstract syntax tree.
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
  // case-insensitive). Selectors, values and at-rule preludes are untouched.
  lowercaseProperties: boolean
  // When true, attach a `position: { start: { line, column }, end: {...} }`
  // (1-based) to every node. Off by default — positions add noise.
  position: boolean
}

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

// An AST node (plain object with a `type` discriminator).
type Node = Record<string, any>

// Plugin implementation.
const Css: Plugin = (tn: Tabnas, options: CssOptions) => {
  const lowercaseProperties = !!options.lowercaseProperties
  const position = !!options.position

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
            '#TX': 'a selector, keyframe value, or property name',
            '#GC': ', — selector-group separator',
            '#VL': 'a declaration value (raw text)',
            '#CC': 'a comment',
            '#ATR': 'an at-rule with a rules body (@media, @supports, …)',
            '#ATD': 'an at-rule with a declarations body (@font-face, @page)',
            '#ATK': '@keyframes',
            '#ATS': 'a statement at-rule (@import, @charset, …)',
          })
        },
      },
    },
  })

  const grammarDef = new Tabnas().use(jsonic).parse(grammarText)
  // The grammar builds the typed AST entirely from these grammar-local
  // actions (node constructors, field setters, and array pushers).
  grammarDef.ref = makeActions(lowercaseProperties, position)

  // All jsonic option overrides live on the grammar object so the plugin
  // applies them atomically alongside its rule alts.
  grammarDef.options = {
    rule: {
      // Remove jsonic extensions (implicit maps/lists, top-level commas, path
      // dives). The AST structure is supplied entirely by the rules above.
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
      KEY: ['#TX'],
    },
    // The cssToken matcher owns all non-fixed text (selectors, property
    // names, values, comments, at-rule preludes), so the default
    // string/number/text/value matchers are off.
    string: { chars: '' },
    number: { lex: false },
    text: { lex: false },
    value: { lex: false },
    // `/* ... */` block comments are lexed by the builtin matcher only when
    // the cssToken matcher declines them (i.e. away from statement positions,
    // where they would otherwise be captured as comment nodes); there they
    // are skipped as insignificant whitespace.
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
        cssToken: {
          order: 1e5,
          make: buildCssTokenMatcher(lowercaseProperties),
        },
      },
    },
  }

  // Tag every alt in this grammar with the 'css' group so callers can
  // selectively exclude css alts via `rule.exclude: 'css'`.
  tn.grammar(grammarDef, { rule: { alt: { g: 'css' } } })
}

// --- Grammar actions: build the AST ---------------------------------------

// All grammar-local actions, keyed by their `@name` reference. The node
// constructors overwrite `r.node`; the field setters mutate it; the pushers
// append a finished child node to a parent array. When `position` is on, the
// constructors record `node.position.start` (and single-token nodes their
// `end`); `@cssEnd` records the closing-brace `end`.
function makeActions(
  _lowercaseProperties: boolean,
  position: boolean,
): Record<string, Function> {
  const tokenVal = (r: Rule, i = 0): any => (r as any).o[i]?.val
  const childNode = (r: Rule): any => r.child && r.child.node

  // Record a node's start (and optionally end) position from a token.
  const withPos = (node: Node, tok: any, end?: boolean): Node => {
    if (position && tok) {
      node.position = { start: startPos(tok), end: endPos(tok) }
      if (false === end) node.position.end = undefined
    }
    return node
  }

  return {
    // Node constructors.
    '@cssSheet': (r: Rule) => {
      r.node = { type: 'stylesheet', rules: [] }
      if (position) r.node.position = { start: { line: 1, column: 1 }, end: undefined }
    },
    '@cssRule': (r: Rule) => {
      r.node = withPos(
        { type: 'rule', selectors: [], declarations: [] },
        (r as any).o[0],
        false,
      )
    },
    '@cssDecl': (r: Rule) => {
      r.node = withPos(
        { type: 'declaration', property: tokenVal(r), value: '' },
        (r as any).o[0],
        false,
      )
    },
    '@cssComment': (r: Rule) => {
      r.node = withPos({ type: 'comment', comment: tokenVal(r) }, (r as any).o[0])
    },
    '@cssKeyframe': (r: Rule) => {
      r.node = withPos(
        { type: 'keyframe', values: [], declarations: [] },
        (r as any).o[0],
        false,
      )
    },
    '@cssAtRules': (r: Rule) => {
      r.node = withPos(makeAtRules((r as any).o[0]), (r as any).o[0], false)
    },
    '@cssAtDecls': (r: Rule) => {
      r.node = withPos(makeAtDecls((r as any).o[0]), (r as any).o[0], false)
    },
    '@cssKeyframes': (r: Rule) => {
      r.node = withPos(makeKeyframes((r as any).o[0]), (r as any).o[0], false)
    },
    '@cssAtStmt': (r: Rule) => {
      r.node = withPos(makeAtStmt((r as any).o[0]), (r as any).o[0])
    },

    // Field setters (mutate the current node, which the rule inherited).
    '@cssSelector': (r: Rule) => {
      ;(r.node as Node).selectors.push(tokenVal(r))
    },
    '@cssKfValue': (r: Rule) => {
      ;(r.node as Node).values.push(tokenVal(r))
    },
    '@cssDeclVal': (r: Rule) => {
      const n = r.node as Node
      n.value = tokenVal(r)
      if (position && n.position) n.position.end = endPos((r as any).o[0])
    },

    // Record the closing-brace / end-of-input end position on the node the
    // block belongs to. Runs in a close phase, so the matched `}`/end token is
    // in r.c[0].
    '@cssEnd': (r: Rule) => {
      const n = r.node as Node
      const tok = (r as any).c[0]
      if (position && n && n.position && tok) n.position.end = endPos(tok)
    },

    // Array pushers (append the built child node to a parent array).
    '@cssPushRule': (r: Rule) => {
      const c = childNode(r)
      if (undefined !== c) (r.node as Node).rules.push(c)
    },
    '@cssPushDecl': (r: Rule) => {
      const c = childNode(r)
      if (undefined !== c) (r.node as Node).declarations.push(c)
    },
    '@cssPushKf': (r: Rule) => {
      const c = childNode(r)
      if (undefined !== c) (r.node as Node).keyframes.push(c)
    },
  }
}

// Position of a token's first character (1-based line/column).
function startPos(tok: any): { line: number; column: number } {
  return { line: tok.rI, column: tok.cI }
}

// Position just after a token's last character (1-based line/column).
function endPos(tok: any): { line: number; column: number } {
  const s: string = tok.src || ''
  let rows = 0
  let lastNL = -1
  for (let i = 0; i < s.length; i++) {
    if ('\n' === s[i]) {
      rows++
      lastNL = i
    }
  }
  if (rows > 0) return { line: tok.rI + rows, column: s.length - lastNL }
  return { line: tok.rI, column: tok.cI + s.length }
}

// Build a block at-rule node whose body is a list of rules (@media, @supports,
// @document, @host, and generic block at-rules).
function makeAtRules(tok: any): Node {
  const kw: string = tok.val
  const prelude: string = (tok.use && tok.use.prelude) || ''
  if ('media' === kw) return { type: 'media', media: prelude, rules: [] }
  if ('supports' === kw) return { type: 'supports', supports: prelude, rules: [] }
  if ('host' === kw) return { type: 'host', rules: [] }
  if ('document' === kw || /-document$/.test(kw)) {
    const node: Node = { type: 'document', document: prelude, rules: [] }
    const v = vendorPrefix(kw)
    if (v) node.vendor = v
    return node
  }
  // Generic block at-rule with a rules body (e.g. @container, @layer, @scope).
  return { type: kw, [kw]: prelude, rules: [] }
}

// Build a block at-rule node whose body is declarations (@font-face, @page,
// and generic declaration at-rules).
function makeAtDecls(tok: any): Node {
  const kw: string = tok.val
  const prelude: string = (tok.use && tok.use.prelude) || ''
  if ('font-face' === kw) return { type: 'font-face', declarations: [] }
  if ('page' === kw) {
    return { type: 'page', selectors: prelude ? [prelude] : [], declarations: [] }
  }
  return { type: kw, declarations: [] }
}

// Build a @keyframes node (possibly vendor-prefixed).
function makeKeyframes(tok: any): Node {
  const kw: string = tok.val
  const name: string = (tok.use && tok.use.prelude) || ''
  const node: Node = { type: 'keyframes', name }
  const v = vendorPrefix(kw)
  if (v) node.vendor = v
  node.keyframes = []
  return node
}

// Build a statement at-rule node (@import, @charset, @namespace, …): the
// at-keyword is the node type and the field carrying its params.
function makeAtStmt(tok: any): Node {
  const kw: string = tok.val
  const params: string = (tok.use && tok.use.params) || ''
  return { type: kw, [kw]: params }
}

// Extract a `-vendor-` prefix from an at-keyword (e.g. `-webkit-keyframes`).
function vendorPrefix(kw: string): string | undefined {
  const m = /^(-[a-z]+-)/.exec(kw)
  return m ? m[1] : undefined
}

// --- Lexer ----------------------------------------------------------------

// Rule names at which a `/* */` comment is captured as a node — the statement
// / declaration / keyframe LIST readers, which lex the first token of each
// item. (The item builders `statement`/`decl`/`keyframe` reuse that cached
// token; a comment seen mid-construct, e.g. between a property and its `:`,
// is under a builder rule and so is skipped, not captured.)
const COMMENT_NODE_RULES: Record<string, true> = {
  items: true,
  decls: true,
  kfitems: true,
  // The block wrappers lex the first body token (their empty-block `#OB #CB`
  // lookahead), so a comment immediately after `{` is captured here too.
  declbody: true,
  rulesbody: true,
  kfbody: true,
}

// The single lex matcher. It emits the AST's text tokens by position; the
// grammar assembles them into nodes:
//   - in `declval` (a value position) -> a value run up to `;`/`}` (#VL)
//   - a `,` separating selectors of a group -> #GC
//   - a `@`-rule -> #ATR / #ATD / #ATK / #ATS (keyword in val, prelude/params
//     in use), classified block-vs-statement by `{`-before-`;` lookahead
//   - a `/* */` comment at a list position -> #CC (else deferred / skipped)
//   - otherwise a single selector (up to a top-level `,` or `{`) or a
//     property name (up to `:`) -> #TX
// Fixed punctuation and whitespace are deferred to the builtin matchers.
function buildCssTokenMatcher(lowercaseProperties: boolean) {
  return function makeCssTokenMatcher(_cfg: Config, _opts: TabnasOptions) {
    return function cssTokenMatcher(lex: Lex, rule: Rule) {
      const { pnt } = lex
      const src: string = lex.src as unknown as string
      const { sI, cI } = pnt
      const c = src[sI]
      const name = (rule as any).name

      if (undefined === c) return undefined
      // Defer whitespace to the space/line matchers.
      if (' ' === c || '\t' === c || '\r' === c || '\n' === c) return undefined

      // Comments: a node at a list position, otherwise deferred (and skipped).
      if ('/' === c && '*' === src[sI + 1]) {
        if (true !== COMMENT_NODE_RULES[name]) return undefined
        let e = src.indexOf('*/', sI + 2)
        const contentEnd = e < 0 ? src.length : e
        const end = e < 0 ? src.length : e + 2
        const tkn = lex.token('#CC', src.substring(sI + 2, contentEnd),
          src.substring(sI, end), pnt)
        advance(pnt, src, sI, end)
        return tkn
      }

      // Value position: read a declaration value up to the next top-level
      // `;`/`}` and emit one #VL (comments stripped, trailing space trimmed).
      if ('declval' === name) {
        if ('{' === c || '}' === c || ';' === c || ':' === c) return undefined
        const endI = scanValueEnd(src, sI)
        const val = stripComments(src.substring(sI, endI)).trim()
        const tkn = lex.token('#VL', val, src.substring(sI, endI), pnt)
        advance(pnt, src, sI, endI)
        return tkn
      }

      // A top-level selector-group comma.
      if (',' === c) {
        const tkn = lex.token('#GC', ',', ',', pnt)
        advance(pnt, src, sI, sI + 1)
        return tkn
      }

      // An at-rule.
      if ('@' === c) return matchAtRule(lex, src, sI, cI)

      // Other fixed punctuation belongs to the grammar.
      if ('{' === c || '}' === c || ';' === c) return undefined

      // A selector or a property name, by `{`-before-`;` lookahead.
      const brace = scanToBraceOrEnd(src, sI)
      if (brace.kind === 'selector') {
        // One selector of a (possible) group: up to the next top-level `,`/`{`
        // (comments stripped, surrounding space trimmed).
        const end = scanSelectorEnd(src, sI)
        const sel = stripComments(src.substring(sI, end)).trim()
        const tkn = lex.token('#TX', sel, src.substring(sI, end), pnt)
        advance(pnt, src, sI, end)
        return tkn
      }
      // A property name: the identifier up to `:`, whitespace, `;` or `}`.
      let eI = sI
      while (eI < src.length && isPropChar(src.charCodeAt(eI))) eI++
      if (eI === sI) return undefined
      let prop = src.substring(sI, eI)
      if (lowercaseProperties) prop = prop.toLowerCase()
      const tkn = lex.token('#TX', prop, src.substring(sI, eI), pnt)
      advance(pnt, src, sI, eI)
      return tkn
    }
  }
}

// Lex an at-rule starting at `@`. Classifies it block-vs-statement by lookahead
// and, for blocks, by keyword, emitting #ATR/#ATD/#ATK (with the prelude) or
// #ATS (with the params).
function matchAtRule(lex: Lex, src: string, sI: number, cI: number) {
  const { pnt } = lex
  let kEnd = sI + 1
  while (kEnd < src.length && isAtChar(src.charCodeAt(kEnd))) kEnd++
  const kw = src.substring(sI + 1, kEnd)

  const brace = scanToBraceOrEnd(src, sI)
  if (brace.kind === 'selector') {
    // Block at-rule: the prelude is the text between the keyword and `{`.
    const prelude = src.substring(kEnd, brace.index).trim()
    const tin =
      isKeyframesKw(kw) ? '#ATK' : isDeclsKw(kw) ? '#ATD' : '#ATR'
    const tkn = lex.token(tin, kw, src.substring(sI, brace.index), pnt, {
      prelude,
    })
    advance(pnt, src, sI, brace.index)
    return tkn
  }
  // Statement at-rule: params run up to the next top-level `;`/`}`. A `;` is
  // consumed (it terminates the statement); a `}`/end-of-input is left.
  const pEnd = scanValueEnd(src, kEnd)
  const params = src.substring(kEnd, pEnd).trim()
  const end = ';' === src[pEnd] ? pEnd + 1 : pEnd
  const tkn = lex.token('#ATS', kw, src.substring(sI, end), pnt, { params })
  advance(pnt, src, sI, end)
  return tkn
}

// Advance the scan point to `end`, updating row/column across any newlines in
// the consumed span [sI, end) (this matcher consumes multi-line runs the
// space/line matchers never see, so it must track rows itself).
function advance(pnt: any, src: string, sI: number, end: number) {
  let rows = 0
  let lastNL = -1
  for (let i = sI; i < end; i++) {
    if (10 === src.charCodeAt(i)) {
      rows++
      lastNL = i
    }
  }
  if (rows > 0) {
    pnt.rI += rows
    pnt.cI = end - lastNL
  } else {
    pnt.cI += end - sI
  }
  pnt.sI = end
}

const KEYFRAMES_RE = /^(-[a-z]+-)?keyframes$/
function isKeyframesKw(kw: string): boolean {
  return KEYFRAMES_RE.test(kw)
}
const DECLS_KW: Record<string, true> = {
  'font-face': true,
  page: true,
  viewport: true,
  '-ms-viewport': true,
  'counter-style': true,
  property: true,
  'font-palette-values': true,
}
function isDeclsKw(kw: string): boolean {
  return true === DECLS_KW[kw]
}

// Scan a key prelude: where it ends and whether it is a selector (a top-level
// `{` is reached first) or a declaration (a top-level `;`/`}` or end-of-input
// is reached first). Strings, (), [] and comments are skipped.
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

// Scan a single selector: the next top-level `,` (a group separator) or `{`
// (or end-of-input). Strings, (), [] and comments are skipped.
function scanSelectorEnd(src: string, i: number): number {
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
    } else if (0 === depth && (',' === c || '{' === c)) {
      return i
    }
    i++
  }
  return i
}

// Scan a declaration value / at-rule params: the next top-level `;` or `}`
// (or end-of-input). Strings, (), [] and comments are skipped.
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

// Skip a quoted string; returns the index after the closing quote.
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

// Remove `/* ... */` comments from a selector / value run, leaving quoted
// strings (which may contain `/*`) untouched.
function stripComments(s: string): string {
  if (s.indexOf('/*') < 0) return s
  let out = ''
  let i = 0
  while (i < s.length) {
    const c = s[i]
    if ('"' === c || '\'' === c) {
      const j = skipString(s, i)
      out += s.substring(i, j)
      i = j
      continue
    }
    if ('/' === c && '*' === s[i + 1]) {
      i = skipComment(s, i)
      continue
    }
    out += c
    i++
  }
  return out
}

// Skip a `/* ... */` comment; returns the index after `*/`.
function skipComment(src: string, i: number): number {
  i += 2
  while (i < src.length && !('*' === src[i] && '/' === src[i + 1])) i++
  return i + 2
}

// CSS property name characters.
function isPropChar(c: number): boolean {
  return (
    (48 <= c && c <= 57) || // 0-9
    (65 <= c && c <= 90) || // A-Z
    (97 <= c && c <= 122) || // a-z
    45 === c || // -
    95 === c // _
  )
}

// At-keyword characters (letters, digits, `-`).
function isAtChar(c: number): boolean {
  return (
    (48 <= c && c <= 57) ||
    (65 <= c && c <= 90) ||
    (97 <= c && c <= 122) ||
    45 === c
  )
}

// Default option values.
Css.defaults = {
  lowercaseProperties: false,
  position: false,
} as CssOptions

export { Css }
export type { CssOptions }
