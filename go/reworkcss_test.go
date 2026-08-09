// Copyright (c) 2025 Richard Rodger and other contributors, MIT License

package tabnascss

// reworkcss_test.go — LANGUAGE CONFORMANCE against the third-party
// reworkcss/css corpus.
//
// README.md and AGENTS.md both state that this plugin's AST "follows the
// widely-used reworkcss/css model", so reworkcss's own
// `test/cases/*/ast.json` files are the authoritative expected VALUES for the
// parse — not merely accept/reject oracles. This runner compares the WHOLE
// tree, both without and with source positions.
//
// The corpus is third-party and is NOT vendored. Fetch it (pinned) with:
//
//	scripts/fetch-reworkcss-tests.sh
//
// `TestMain` in conformance_test.go runs that script before any test, so the
// corpus is normally present. If it is still absent the suite FAILS — it never
// skips, because a conformance suite that quietly does not run reports green
// while measuring nothing.
//
// ts/test/reworkcss.test.ts runs the SAME corpus with the SAME derivation, so
// the two runtimes cannot drift without one of them going red.

import (
	"encoding/json"
	"os"
	"path/filepath"
	"reflect"
	"regexp"
	"sort"
	"strings"
	"testing"
)

// Pinned upstream. Keep in step with scripts/fetch-reworkcss-tests.sh and
// ts/test/reworkcss.test.ts.
const reworkUpstream = "https://github.com/reworkcss/css"
const reworkSHA = "ae6a6f9bf939cbcbc759a12d9f208afb5d4dde75"

// Census of the pinned commit. A corpus that silently shrank must not quietly
// improve the conformance number, so these are hard assertions.
const (
	reworkCaseCount     = 46
	reworkThrowCount    = 4
	reworkNoThrowCount  = 2
	reworkOptionedCount = 1 // parse(src, {silent:true}) — no tabnas equivalent
)

func reworkCorpusDir() string { return filepath.Join("..", "test", "reworkcss-css") }

// reworkAbsent is a FAILURE message, not a skip message. A conformance suite
// that quietly does not run is worse than no suite at all, because the green
// tick is a lie: it reports success while measuring nothing. TestMain
// (conformance_test.go) fetches the corpus before any test runs, so reaching
// this message means the fetch itself failed.
const reworkAbsent = "MISSING CONFORMANCE CORPUS: reworkcss/css is not " +
	"installed, so the CSS conformance claim is UNVERIFIED. Fetch it (pinned) " +
	"with scripts/fetch-reworkcss-tests.sh. This test does NOT skip when the " +
	"corpus is absent."

func reworkCorpusPresent() bool {
	_, err := os.Stat(filepath.Join(reworkCorpusDir(), "test", "parse.js"))
	return err == nil
}

// reworkReadCase is byte-identical to upstream test/cases.js readFile,
// including its single-occurrence CRLF replace, so the parser sees exactly the
// source that produced the expected ASTs (line/column numbers depend on it).
func reworkReadCase(t *testing.T, path string) string {
	t.Helper()
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read %s: %v", path, err)
	}
	src := strings.Replace(string(raw), "\r\n", "\n", 1)
	return strings.TrimSuffix(src, "\n")
}

// reworkWithout strips keys recursively (used for `position`, and for `source`
// inside a position — this plugin has no notion of a source filename).
func reworkWithout(v any, keys ...string) any {
	switch t := v.(type) {
	case []any:
		out := make([]any, 0, len(t))
		for _, e := range t {
			out = append(out, reworkWithout(e, keys...))
		}
		return out
	case map[string]any:
		out := map[string]any{}
	next:
		for k, e := range t {
			for _, drop := range keys {
				if k == drop {
					continue next
				}
			}
			out[k] = reworkWithout(e, keys...)
		}
		return out
	}
	return v
}

// reworkUpstreamRules unwraps upstream's {type,stylesheet:{rules}} envelope;
// this plugin emits {type,rules}.
func reworkUpstreamRules(t *testing.T, path string) []any {
	t.Helper()
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read %s: %v", path, err)
	}
	var ast map[string]any
	if err := json.Unmarshal(raw, &ast); err != nil {
		t.Fatalf("parse %s: %v", path, err)
	}
	sheet, ok := ast["stylesheet"].(map[string]any)
	if !ok {
		t.Fatalf("%s: no .stylesheet", path)
	}
	rules, _ := sheet["rules"].([]any)
	return rules
}

func reworkParse(t *testing.T, src string, opts ...CssOptions) any {
	t.Helper()
	got, err := Parse(src, opts...)
	if err != nil {
		t.Fatalf("parse error: %v", err)
	}
	raw, err := json.Marshal(got)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	var out any
	if err := json.Unmarshal(raw, &out); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	return out
}

func reworkRules(t *testing.T, v any) any {
	t.Helper()
	m, ok := v.(map[string]any)
	if !ok {
		t.Fatalf("parse produced no stylesheet: %#v", v)
	}
	return m["rules"]
}

// reworkDivergent holds the cases where this plugin knowingly differs from
// upstream. Each is asserted explicitly (never skipped), so the divergence
// cannot widen unnoticed and cannot quietly disappear either.
var reworkDivergent = map[string]func(t *testing.T){
	// NO LONGER DIVERGENT — kept here so the case stays explicitly asserted.
	// A zero-length source used to yield nil. The cause was never the rule
	// iteration budget (the engine returns j.emptyResult for "" before the
	// rule loop is reachable); it was that css declared no emptyResult. It
	// now declares one in css.go, so "" matches upstream.
	"empty": func(t *testing.T) {
		want := map[string]any{"type": "stylesheet", "rules": []any{}}
		got, err := Parse("")
		if err != nil {
			t.Fatalf("empty source: unexpected error: %v", err)
		}
		if !reflect.DeepEqual(got, want) {
			t.Fatalf("empty source: got %#v, want %#v", got, want)
		}
		if v := reworkParse(t, " "); !reflect.DeepEqual(v, want) {
			t.Fatalf("single-space source: got %#v, want %#v", v, want)
		}
	},
}

// TestReworkcssCases checks the parse VALUE for every case in the corpus.
func TestReworkcssCases(t *testing.T) {
	if !reworkCorpusPresent() {
		t.Fatal(reworkAbsent)
	}
	casesDir := filepath.Join(reworkCorpusDir(), "test", "cases")
	entries, err := os.ReadDir(casesDir)
	if err != nil {
		t.Fatalf("read %s: %v", casesDir, err)
	}
	var names []string
	for _, e := range entries {
		if e.IsDir() {
			names = append(names, e.Name())
		}
	}
	sort.Strings(names)

	if len(names) != reworkCaseCount {
		t.Fatalf("expected %d cases at %s@%s, got %d; the pinned corpus "+
			"changed. Do not silently accept a different suite.",
			reworkCaseCount, reworkUpstream, reworkSHA, len(names))
	}

	for _, name := range names {
		if fn := reworkDivergent[name]; fn != nil {
			t.Run("cases/"+name+": documented divergence", fn)
			continue
		}
		input := reworkReadCase(t, filepath.Join(casesDir, name, "input.css"))
		expected := reworkUpstreamRules(t, filepath.Join(casesDir, name, "ast.json"))

		// Primary metric: the structural AST, positions removed (the plugin's
		// Position option is off by default).
		t.Run("cases/"+name+": AST", func(t *testing.T) {
			got := reworkWithout(reworkRules(t, reworkParse(t, input)), "position")
			want := reworkWithout(expected, "position")
			if !reflect.DeepEqual(got, want) {
				t.Errorf("cases/%s:\n  got  %s\n  want %s", name, mustJSON(got), mustJSON(want))
			}
		})

		// Secondary metric: the same tree WITH source positions, which upstream
		// records for every node. `source` is upstream-only (a filename).
		t.Run("cases/"+name+": AST + position", func(t *testing.T) {
			got := reworkRules(t, reworkParse(t, input, CssOptions{Position: boolPtr(true)}))
			want := reworkWithout(expected, "source")
			if !reflect.DeepEqual(got, want) {
				t.Errorf("cases/%s (position):\n  got  %s\n  want %s", name, mustJSON(got), mustJSON(want))
			}
		})
	}
}

func mustJSON(v any) string {
	raw, err := json.Marshal(v)
	if err != nil {
		return "<unmarshalable>"
	}
	return string(raw)
}

// --- The accept/reject oracle, extracted mechanically from upstream's own
// test/parse.js so it cannot drift from what reworkcss actually asserts. ---

var reworkAssertRe = regexp.MustCompile(
	`assert\.(throws|doesNotThrow)\(\s*function\s*\(\)\s*\{\s*parse\('((?:[^'\\]|\\.)*)'\s*(,[^)]*)?\)`)

type reworkErrorCase struct {
	kind     string
	src      string
	optioned bool
}

// reworkDecodeJS decodes a JavaScript single-quoted string literal.
func reworkDecodeJS(lit string) string {
	var b strings.Builder
	for i := 0; i < len(lit); i++ {
		c := lit[i]
		if c != '\\' || i+1 >= len(lit) {
			b.WriteByte(c)
			continue
		}
		i++
		switch lit[i] {
		case 'n':
			b.WriteByte('\n')
		case 'r':
			b.WriteByte('\r')
		case 't':
			b.WriteByte('\t')
		case 'f':
			b.WriteByte('\f')
		case 'b':
			b.WriteByte('\b')
		case 'v':
			b.WriteByte('\v')
		case '0':
			b.WriteByte(0)
		default:
			b.WriteByte(lit[i])
		}
	}
	return b.String()
}

func reworkExtractErrorCases(t *testing.T) []reworkErrorCase {
	t.Helper()
	raw, err := os.ReadFile(filepath.Join(reworkCorpusDir(), "test", "parse.js"))
	if err != nil {
		t.Fatalf("read upstream test/parse.js: %v", err)
	}
	var out []reworkErrorCase
	for _, m := range reworkAssertRe.FindAllStringSubmatch(string(raw), -1) {
		out = append(out, reworkErrorCase{
			kind:     m[1],
			src:      reworkDecodeJS(m[2]),
			optioned: m[3] != "",
		})
	}
	return out
}

// TestReworkcssAcceptReject checks the inputs upstream asserts must (not) fail.
func TestReworkcssAcceptReject(t *testing.T) {
	if !reworkCorpusPresent() {
		t.Fatal(reworkAbsent)
	}
	cases := reworkExtractErrorCases(t)

	throws, noThrows, optioned := 0, 0, 0
	for _, c := range cases {
		switch {
		case c.optioned:
			optioned++
		case c.kind == "throws":
			throws++
		default:
			noThrows++
		}
	}
	if throws != reworkThrowCount || noThrows != reworkNoThrowCount ||
		optioned != reworkOptionedCount {
		t.Fatalf("upstream test/parse.js changed shape (throws=%d doesNotThrow=%d "+
			"optioned=%d, want %d/%d/%d); the mechanical extraction is no longer "+
			"reading the corpus it was written for. Fix the extractor rather than "+
			"accepting a smaller must-fail set.",
			throws, noThrows, optioned,
			reworkThrowCount, reworkNoThrowCount, reworkOptionedCount)
	}

	for _, c := range cases {
		c := c
		label := strings.ReplaceAll(c.src, "\n", " ; ")
		if 60 < len(label) {
			label = label[:57] + "..."
		}
		// parse(src, {silent:true}) asserts reworkcss's error-RECOVERY mode,
		// which this plugin does not implement and does not claim to. Not a
		// skip of a case this plugin is judged on: it is a case about a
		// different API.
		if c.optioned {
			t.Run("[n/a: reworkcss silent option] "+label, func(t *testing.T) {
				if c.kind != "doesNotThrow" {
					t.Fatalf("unexpected optioned assertion kind %q", c.kind)
				}
			})
			continue
		}
		if c.kind == "throws" {
			t.Run("[reject] "+label, func(t *testing.T) {
				if _, err := Parse(c.src); err == nil {
					t.Fatalf("upstream reworkcss/css rejects this input; it must not parse")
				}
			})
			continue
		}
		t.Run("[accept] "+label, func(t *testing.T) {
			got, err := Parse(c.src)
			if err != nil {
				t.Fatalf("unexpected parse error: %v", err)
			}
			if got == nil {
				t.Fatalf("parse produced no value")
			}
		})
	}
}
