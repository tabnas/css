// Copyright (c) 2026 Richard Rodger and other contributors, MIT License

package tabnascss

// window_test.go — a parse from a reader (Tabnas.ParseReader, admin ADR-30
// and ADR-33).
//
// MakeJsonic builds the parser whole, as jsonic.Make() then
// j.UseDefaults(Css, Defaults, opts) builds it, and declares lex.window on
// it, so that a reader is read a window at a time. The cssToken matcher
// reads the source only through the window: a scan that reaches the
// window's end fills it and the token is cut again. These tests hold it to
// that: MakeJsonic declares the window and parses every fixture row as that
// composition does; every row, under its opts and again with positions on,
// and every document of the reworkcss corpus, with and without positions,
// parses the same from a reader, in pieces of 1, 3, 64 and 4096 bytes, as
// from its text; and tokens and lookaheads of every kind longer than the
// window, and a long document, parse the same too.
//
// Go's window sizes are fixed at 64 KiB (the engine's small-window hook is
// unexported), and before each token the window reaches 64 KiB past the
// cursor, so a scan meets the window's edge only where it reads further
// than that: the long tokens are each longer, and the long document holds
// them among short tokens of every kind, so that its tokens straddle the
// window's 64 KiB blocks at many offsets.

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"reflect"
	"sort"
	"strings"
	"testing"

	jsonic "github.com/tabnas/jsonic/go"
	tabnas "github.com/tabnas/parser/go"
	support "github.com/tabnas/support/go"
)

// readerMargin is how far past the cursor the window reaches before each
// token is cut: a scan that reads further meets the window's edge.
const readerMargin = 64 << 10

// pieces is a reader that hands its bytes over at most size at a time.
type pieces struct {
	text []byte
	at   int
	size int
}

func (p *pieces) Read(buf []byte) (int, error) {
	if p.at == len(p.text) {
		return 0, io.EOF
	}
	n := min(p.size, len(buf), len(p.text)-p.at)
	copy(buf, p.text[p.at:p.at+n])
	p.at += n
	return n, nil
}

// outcome is what a parse shows: its value, or its error's code, position,
// row, column and token text. An error's excerpt is left out: a parse from
// a reader shows a long line only as far back as the window reaches
// (tabnas/parser DIVERGENCE.md), and that is the one difference allowed.
type outcome struct {
	value any
	err   string
}

func outcomeOf(v any, err error) outcome {
	if err == nil {
		return outcome{value: v}
	}
	var te *tabnas.TabnasError
	if errors.As(err, &te) {
		return outcome{err: fmt.Sprintf("%s at %d %d:%d %q", te.Code, te.Pos, te.Row, te.Col, te.Src)}
	}
	return outcome{err: "not a TabnasError: " + err.Error()}
}

func (o outcome) String() string {
	if o.err != "" {
		return "error " + o.err
	}
	s := fmt.Sprintf("value %v", o.value)
	if len(s) > 300 {
		s = s[:300] + "..."
	}
	return s
}

func (o outcome) same(p outcome) bool {
	return o.err == p.err && reflect.DeepEqual(o.value, p.value)
}

// sameFromReader parses text whole and from a reader in pieces of each
// size, and reports where the two differ. It returns the number of reader
// parses compared.
func sameFromReader(t *testing.T, j *tabnas.Tabnas, at, text string, sizes []int) int {
	t.Helper()
	whole := outcomeOf(j.Parse(text))
	for _, size := range sizes {
		read := outcomeOf(j.ParseReader(&pieces{text: []byte(text), size: size}))
		if !read.same(whole) {
			show := text
			if len(show) > 80 {
				show = show[:80]
			}
			t.Errorf("%s: %q in pieces of %d:\n  whole:  %s\n  reader: %s", at, show, size, whole, read)
		}
	}
	return len(sizes)
}

// readerSizes are the pieces a document is handed over in: down to a byte
// for a short one, and from 64 bytes for one over 8 KiB, whose every
// one-byte read the Go window answers with a copy of itself
// (tabnas/parser#321).
func readerSizes(text string) []int {
	if len(text) > 8<<10 {
		return []int{64, 4096, readerMargin + 3}
	}
	return []int{1, 3, 64, 4096}
}

// optionsOf reads a fixture row's opts as the typed options MakeJsonic
// takes: each key read for its truthiness, as the plugin reads it.
func optionsOf(opts map[string]any) CssOptions {
	var o CssOptions
	if v, ok := opts["lowercaseProperties"]; ok {
		b := toBool(v)
		o.LowercaseProperties = &b
	}
	if v, ok := opts["position"]; ok {
		b := toBool(v)
		o.Position = &b
	}
	return o
}

// fixtureRows calls each with every row of the shared fixtures: where it
// is, its input and its plugin options.
func fixtureRows(t *testing.T, each func(at, input string, opts map[string]any)) {
	t.Helper()
	dir, err := support.FindSpecDir("")
	if err != nil {
		t.Fatal(err)
	}
	paths, err := filepath.Glob(filepath.Join(dir, "*.tsv"))
	if err != nil || len(paths) == 0 {
		t.Fatalf("no fixtures in %s: %v", dir, err)
	}
	sort.Strings(paths)
	for _, path := range paths {
		spec, err := support.LoadSpec(path, nil)
		if err != nil {
			t.Fatal(err)
		}
		for _, row := range spec.Rows {
			opts := map[string]any{}
			if raw := row.Named("opts"); raw != "" {
				if err := json.Unmarshal([]byte(raw), &opts); err != nil {
					t.Fatalf("%s:%d: opts: %v", filepath.Base(path), row.Line, err)
				}
			}
			each(fmt.Sprintf("%s:%d", filepath.Base(path), row.Line), row.UnescNamed("input"), opts)
		}
	}
}

// TestMakeJsonicDeclaresTheWindow: MakeJsonic declares lex.window; the
// plugin alone, which a host may install beneath matchers of its own,
// declares nothing.
func TestMakeJsonicDeclaresTheWindow(t *testing.T) {
	if !MakeJsonic().Config().Window {
		t.Error("MakeJsonic does not declare lex.window")
	}
	// jsonic's base declares the window itself (tabnas/jsonic#121), so the
	// plugin goes on that base with the window turned off.
	no := false
	j := jsonic.Make(tabnas.Options{Lex: &tabnas.LexOptions{Window: &no}})
	if err := j.UseDefaults(Css, Defaults, nil); err != nil {
		t.Fatal(err)
	}
	if j.Config().Window {
		t.Error("the Css plugin declares lex.window")
	}
}

// TestMakeJsonicParsesAsTheComposition: every fixture row parses through
// MakeJsonic as through jsonic.Make() then j.UseDefaults(Css, Defaults,
// opts), which dmog composed before.
func TestMakeJsonicParsesAsTheComposition(t *testing.T) {
	n := 0
	fixtureRows(t, func(at, input string, opts map[string]any) {
		j := jsonic.Make()
		if err := j.UseDefaults(Css, Defaults, opts); err != nil {
			t.Fatal(err)
		}
		composed := outcomeOf(j.Parse(input))
		made := outcomeOf(MakeJsonic(optionsOf(opts)).Parse(input))
		if !made.same(composed) {
			t.Errorf("%s:\n  MakeJsonic: %s\n  composed:   %s", at, made, composed)
		}
		n++
	})
	if n < 150 {
		t.Errorf("only %d rows", n)
	}
}

// TestFixturesFromAReader: every fixture row parses the same from a reader
// as from its text, under MakeJsonic with the row's options, and again
// with positions on, whose columns count from the start of a line the
// window may have dropped.
func TestFixturesFromAReader(t *testing.T) {
	compared := 0
	fixtureRows(t, func(at, input string, opts map[string]any) {
		o := optionsOf(opts)
		compared += sameFromReader(t, MakeJsonic(o), at, input, readerSizes(input))
		if o.Position == nil || !*o.Position {
			yes := true
			o.Position = &yes
			compared += sameFromReader(t, MakeJsonic(o), at+" (position)", input, readerSizes(input))
		}
	})
	t.Logf("reader fixtures: %d reader parses (100%%)", compared)
	if compared < 1000 {
		t.Errorf("only %d reader parses", compared)
	}
}

// TestCorpusFromAReader: every case of the reworkcss corpus, with and
// without positions, and every input of its accept/reject oracle, parses
// the same from a reader as from its text.
func TestCorpusFromAReader(t *testing.T) {
	if !reworkCorpusPresent() {
		t.Fatal(reworkAbsent)
	}
	cases := filepath.Join(reworkCorpusDir(), "test", "cases")
	entries, err := os.ReadDir(cases)
	if err != nil {
		t.Fatal(err)
	}
	yes := true
	plain, positioned := MakeJsonic(), MakeJsonic(CssOptions{Position: &yes})
	compared := 0
	for _, entry := range entries {
		input := reworkReadCase(t, filepath.Join(cases, entry.Name(), "input.css"))
		at := "cases/" + entry.Name()
		compared += sameFromReader(t, plain, at, input, readerSizes(input))
		compared += sameFromReader(t, positioned, at+" (position)", input, readerSizes(input))
	}
	oracle := reworkExtractErrorCases(t)
	for _, c := range oracle {
		compared += sameFromReader(t, plain, "parse.js "+c.kind, c.src, readerSizes(c.src))
	}
	t.Logf("reader corpus: %d cases and %d oracle inputs (100%%), %d reader parses",
		len(entries), len(oracle), compared)
	if len(entries) != 46 {
		t.Errorf("%d cases, want 46", len(entries))
	}
	if compared < 300 {
		t.Errorf("only %d reader parses", compared)
	}
}

// longTokens are documents each holding a token, or a lookahead, longer
// than the window, of every kind the matcher cuts or scans, and each parses.
func longTokens() []string {
	long := readerMargin + 2<<10
	third := long / 3
	return []string{
		// A selector, and a group whose `{` lies past the window, past a
		// comment too.
		"." + strings.Repeat("s", long) + " { b: c }",
		"h1 " + strings.Repeat("x", third) + ", h2 " + strings.Repeat("y", third) +
			", h3 " + strings.Repeat("z", third) + " { b: c }",
		"a /* " + strings.Repeat("{;}", long/3) + " */ , b { c: d }",
		// A property name, and a value, with a string holding `;` and `}`,
		// and with a comment holding them.
		"a { " + strings.Repeat("p", long) + ": c }",
		"a { b: " + strings.Repeat("v", long) + " }",
		"a { b: \"" + strings.Repeat(";}", long/2) + "\"; c: d }",
		"a { b: c /* " + strings.Repeat(";}", long/2) + " */ d; e: f }",
		// A comment node, and a comment the declaration skips.
		"/* " + strings.Repeat("c", long) + " */ a { b: c }",
		"a { b /* " + strings.Repeat("c", long) + " */ : c }",
		// An at-rule's prelude, with a comment, a statement's params, and
		// its keyword.
		"@media " + strings.Repeat("m", long) + " { a { b: c } }",
		"@media x /* " + strings.Repeat("{;}", long/3) + " */ { a { b: c } }",
		"@import \"" + strings.Repeat("u", long) + "\"; a { b: c }",
		"@" + strings.Repeat("k", long) + " x; a { b: c }",
		// Escapes, brackets and parentheses the scans skip.
		"." + strings.Repeat("\\:", long/2) + " { b: c }",
		"a[x=\"" + strings.Repeat("q", long) + "\"] { b: c }",
		"a { b: url(" + strings.Repeat("w", long) + ") }",
		// Space and lines between tokens.
		"a {" + strings.Repeat(" ", long) + "b: c }",
		"a {" + strings.Repeat("\n", long) + "b: c }",
		// Characters past ASCII, astral ones among them, before a node on its line.
		strings.Repeat("\U0001d11e", long/4) + "{a:b} c { d: é }",
	}
}

// longErrors are documents whose error lies past the window: an unclosed
// comment, at the end and in a value, and a selector with no block.
func longErrors() []string {
	long := readerMargin + 2<<10
	return []string{
		"a { b: c } /* " + strings.Repeat("u", long),
		"a { b: c /* " + strings.Repeat("u", long),
		strings.Repeat("s", long),
	}
}

// TestLongTokensFromAReader: each long token, and each error past the
// window, parses the same from a reader as from its text, with and without
// positions.
func TestLongTokensFromAReader(t *testing.T) {
	yes := true
	for _, o := range []CssOptions{{}, {Position: &yes}} {
		j := MakeJsonic(o)
		for n, doc := range append(longTokens(), longErrors()...) {
			sameFromReader(t, j, fmt.Sprintf("long token %d", n), doc, []int{4096, readerMargin + 3})
		}
	}
}

// TestLongDocumentFromAReader: a document of over a megabyte, its tokens of
// every kind long and short, the long ones longer than the window, parses
// the same from a reader as from its text, with and without positions, and
// so does the same document with an error at its end, which a parse from a
// reader places where the whole text does after the window has moved on
// many times.
func TestLongDocumentFromAReader(t *testing.T) {
	var b strings.Builder
	b.WriteString("/* sheet */\n@charset \"utf-8\";\n")
	long := longTokens()
	for i := 0; i < 4000; i++ {
		fmt.Fprintf(&b, ".c%d, h%d > a[href^=\"x;%d\"]:not(.d, .e) {\n", i, i%6, i)
		fmt.Fprintf(&b, "  color: #%d; /* note %d */\n", i%999, i)
		fmt.Fprintf(&b, "  background: url(\"i%d.png\") no-repeat; content: \"é\U0001d11e%d\";\n}\n", i, i)
		if i%100 == 0 {
			fmt.Fprintf(&b, "@media (min-width: %dpx) { a { b: %d } }\n", i, i)
			fmt.Fprintf(&b, "@keyframes k%d { from { o: 0 } 50%%, to { o: 1 } }\n", i)
		}
		if i == 2000 {
			// The long tokens that parse, each closed off so that the
			// next statement starts afresh.
			for _, doc := range long {
				b.WriteString(doc)
				b.WriteString("\n")
			}
		}
	}
	body := b.String()
	text := body + "z { last: 1 }\n"
	broken := body + "z { last: 1 } /* open\n"
	if len(text) < 1<<20 {
		t.Fatalf("the document is %d bytes", len(text))
	}
	yes := true
	for n, o := range []CssOptions{{}, {Position: &yes}} {
		j := MakeJsonic(o)
		if _, err := j.Parse(text); err != nil {
			t.Fatalf("the long document does not parse: %v", err)
		}
		if _, err := j.Parse(broken); err == nil || err.(*tabnas.TabnasError).Code != "unterminated_comment" {
			t.Fatalf("the broken document is not unterminated_comment: %v", err)
		}
		sameFromReader(t, j, "the long document", text, []int{64, 4096, readerMargin + 3})
		sameFromReader(t, j, "the long document with an error at its end", broken, []int{4096, readerMargin + 3})
		t.Logf("reader long document: %d of 2 option sets (%d%%), %d bytes", n+1, (n+1)*50, len(text))
	}
}
