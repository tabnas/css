// Copyright (c) 2026 Richard Rodger and other contributors, MIT License

package tabnascss

// reworkcss_test.go — conformance against the THIRD-PARTY reworkcss/css
// corpus. Mirrors ts/test/reworkcss.test.ts exactly (same corpus, same
// derivation, same census), so the two runtimes cannot drift without one of
// them going red.
//
// README.md/AGENTS.md state this plugin's AST "follows the widely-used
// reworkcss/css model", so upstream's test/cases/*/ast.json are the
// authoritative expected VALUES, not accept/reject oracles. The whole tree is
// compared. There is no SKIP list.

import (
	"encoding/json"
	"os"
	"path/filepath"
	"reflect"
	"regexp"
	"sort"
	"strconv"
	"strings"
	"testing"
)

// Pinned upstream. Keep in step with scripts/fetch-reworkcss-tests.sh and
// ts/test/reworkcss.test.ts.
const rwUpstream = "https://github.com/reworkcss/css"
const rwSHA = "ae6a6f9bf939cbcbc759a12d9f208afb5d4dde75"

// Census of the pinned commit.
const rwCaseCount = 46
const rwThrowCount = 4
const rwNoThrowCount = 2
const rwOptionedCount = 1 // parse(src, {silent:true}) — no tabnas equivalent

func rwDir() string { return filepath.Join(repoRoot(), "test", "reworkcss-css") }

func rwRequire(t *testing.T) {
	t.Helper()
	if _, err := os.Stat(filepath.Join(rwDir(), "test", "parse.js")); err != nil {
		t.Fatal(corpusMissing(rwDir(), rwSHA, "scripts/fetch-reworkcss-tests.sh"))
	}
}

// rwReadCase is byte-identical to upstream test/cases.js readFile, including
// its single-occurrence CRLF replace, so the parser sees exactly the source
// that produced the expected ASTs (line/column numbers depend on it).
var rwCRLF = regexp.MustCompile(`\r\n`)

func rwReadCase(t *testing.T, path string) string {
	t.Helper()
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read %s: %v", path, err)
	}
	src := string(raw)
	// Replace only the FIRST occurrence, as upstream's /\r\n/ (no /g) does.
	if loc := rwCRLF.FindStringIndex(src); loc != nil {
		src = src[:loc[0]] + "\n" + src[loc[1]:]
	}
	src = strings.TrimSuffix(src, "\n")
	return src
}

// rwWithout strips the named keys recursively.
func rwWithout(v any, keys ...string) any {
	switch t := v.(type) {
	case []any:
		out := make([]any, len(t))
		for i, e := range t {
			out[i] = rwWithout(e, keys...)
		}
		return out
	case map[string]any:
		out := map[string]any{}
		for k, e := range t {
			drop := false
			for _, kk := range keys {
				if k == kk {
					drop = true
					break
				}
			}
			if drop {
				continue
			}
			out[k] = rwWithout(e, keys...)
		}
		return out
	}
	return v
}

// rwUpstreamRules unwraps {type:'stylesheet', stylesheet:{rules}}.
func rwUpstreamRules(t *testing.T, name string, raw []byte) any {
	t.Helper()
	var ast map[string]any
	if err := json.Unmarshal(raw, &ast); err != nil {
		t.Fatalf("cases/%s: bad ast.json: %v", name, err)
	}
	sheet, ok := ast["stylesheet"].(map[string]any)
	if !ok {
		t.Fatalf("cases/%s: ast.json has no .stylesheet", name)
	}
	return sheet["rules"]
}

func TestReworkcssCases(t *testing.T) {
	rwRequire(t)
	casesDir := filepath.Join(rwDir(), "test", "cases")
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

	t.Run("corpus census", func(t *testing.T) {
		if len(names) != rwCaseCount {
			t.Fatalf("expected %d cases at %s@%s, found %d; the pinned corpus "+
				"changed. Do not silently accept a different suite.",
				rwCaseCount, rwUpstream, rwSHA, len(names))
		}
	})

	for _, name := range names {
		input := rwReadCase(t, filepath.Join(casesDir, name, "input.css"))
		astRaw, err := os.ReadFile(filepath.Join(casesDir, name, "ast.json"))
		if err != nil {
			t.Fatalf("cases/%s: %v", name, err)
		}
		want := rwUpstreamRules(t, name, astRaw)

		// Primary metric: the structural AST, positions removed.
		t.Run("cases/"+name+": AST", func(t *testing.T) {
			got, err := parseCss(input, map[string]any{})
			if err != nil {
				t.Fatalf("cases/%s: unexpected parse error: %v", name, err)
			}
			norm, err := jsonNorm(got)
			if err != nil {
				t.Fatalf("cases/%s: %v", name, err)
			}
			m, ok := norm.(map[string]any)
			if !ok {
				t.Fatalf("cases/%s: parse produced %#v, not a stylesheet", name, norm)
			}
			g := rwWithout(m["rules"], "position")
			w := rwWithout(want, "position")
			if !reflect.DeepEqual(g, w) {
				t.Errorf("cases/%s:\n  got  %s\n  want %s", name, mustJSON(g), mustJSON(w))
			}
		})

		// Secondary metric: the same tree WITH source positions, which
		// upstream records for every node. `source` is upstream-only.
		t.Run("cases/"+name+": AST + position", func(t *testing.T) {
			got, err := parseCss(input, map[string]any{"position": true})
			if err != nil {
				t.Fatalf("cases/%s: unexpected parse error: %v", name, err)
			}
			norm, err := jsonNorm(got)
			if err != nil {
				t.Fatalf("cases/%s: %v", name, err)
			}
			m, ok := norm.(map[string]any)
			if !ok {
				t.Fatalf("cases/%s: parse produced %#v, not a stylesheet", name, norm)
			}
			g := m["rules"]
			w := rwWithout(want, "source")
			if !reflect.DeepEqual(g, w) {
				t.Errorf("cases/%s (position):\n  got  %s\n  want %s", name, mustJSON(g), mustJSON(w))
			}
		})
	}
}

func mustJSON(v any) string {
	b, err := json.Marshal(v)
	if err != nil {
		return "<unmarshalable>"
	}
	return string(b)
}

// ---------------------------------------------------------------------------
// The must-fail / must-accept half. Upstream keeps these in code
// (test/parse.js), not in a data file, so they are extracted mechanically
// from the fetched source and the extraction count is pinned — a change in
// upstream's file shape fails loudly instead of yielding an empty corpus.
// ---------------------------------------------------------------------------

type rwErrCase struct {
	kind     string // "throws" | "doesNotThrow"
	src      string
	optioned bool
}

var rwAssertRE = regexp.MustCompile(
	`assert\.(throws|doesNotThrow)\(\s*function\s*\(\)\s*\{\s*parse\('((?:[^'\\]|\\.)*)'\s*(,[^)]*)?\)`)

// rwDecodeJS decodes a JavaScript single-quoted string literal.
func rwDecodeJS(lit string) string {
	var b strings.Builder
	r := []rune(lit)
	for i := 0; i < len(r); i++ {
		if r[i] != '\\' {
			b.WriteRune(r[i])
			continue
		}
		i++
		if i >= len(r) {
			break
		}
		switch r[i] {
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
		case 'u':
			if i+4 < len(r) {
				if n, err := strconv.ParseInt(string(r[i+1:i+5]), 16, 32); err == nil {
					b.WriteRune(rune(n))
					i += 4
					continue
				}
			}
			b.WriteRune('u')
		case 'x':
			if i+2 < len(r) {
				if n, err := strconv.ParseInt(string(r[i+1:i+3]), 16, 32); err == nil {
					b.WriteRune(rune(n))
					i += 2
					continue
				}
			}
			b.WriteRune('x')
		default:
			b.WriteRune(r[i])
		}
	}
	return b.String()
}

func rwExtractErrCases(t *testing.T) []rwErrCase {
	t.Helper()
	raw, err := os.ReadFile(filepath.Join(rwDir(), "test", "parse.js"))
	if err != nil {
		t.Fatalf("read parse.js: %v", err)
	}
	var out []rwErrCase
	for _, m := range rwAssertRE.FindAllStringSubmatch(string(raw), -1) {
		out = append(out, rwErrCase{kind: m[1], src: rwDecodeJS(m[2]), optioned: m[3] != ""})
	}
	return out
}

func TestReworkcssMustFail(t *testing.T) {
	rwRequire(t)
	cases := rwExtractErrCases(t)

	t.Run("extraction census", func(t *testing.T) {
		var thr, no, opt int
		for _, c := range cases {
			switch {
			case c.optioned:
				opt++
			case c.kind == "throws":
				thr++
			default:
				no++
			}
		}
		if thr != rwThrowCount || no != rwNoThrowCount || opt != rwOptionedCount {
			t.Fatalf("upstream test/parse.js changed shape: got throws=%d "+
				"doesNotThrow=%d optioned=%d, want %d/%d/%d. Fix the extractor "+
				"rather than accepting a smaller must-fail set.",
				thr, no, opt, rwThrowCount, rwNoThrowCount, rwOptionedCount)
		}
	})

	for _, c := range cases {
		c := c
		label := strconv.Quote(c.src)
		if 60 < len(label) {
			label = label[:57] + `..."`
		}
		if c.optioned {
			// parse(src, {silent:true}) asserts reworkcss's error-RECOVERY
			// mode, which this plugin does not implement and does not claim
			// to. Not a skipped conformance case: a different API.
			t.Run("[n/a reworkcss silent option] "+label, func(t *testing.T) {
				if c.kind != "doesNotThrow" {
					t.Fatalf("unexpected optioned assertion kind %q", c.kind)
				}
			})
			continue
		}
		if c.kind == "throws" {
			t.Run("[reject] "+label, func(t *testing.T) {
				got, err := parseCss(c.src, map[string]any{})
				if err == nil {
					t.Fatalf("upstream reworkcss/css rejects this input; it "+
						"must not parse. Got: %s", mustJSON(got))
				}
			})
		} else {
			t.Run("[accept] "+label, func(t *testing.T) {
				got, err := parseCss(c.src, map[string]any{})
				if err != nil {
					t.Fatalf("upstream reworkcss/css accepts this input: %v", err)
				}
				if got == nil {
					t.Fatalf("parse produced no value")
				}
			})
		}
	}
}
