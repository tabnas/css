// Copyright (c) 2026 Richard Rodger and other contributors, MIT License

package tabnascss

// cssparsing_test.go — conformance against the THIRD-PARTY CSS Syntax Level 3
// corpus (SimonSapin/css-parsing-tests: the implementation-independent suite
// used by rust-cssparser/Servo, tinycss2 and Crass).
//
// Mirrors ts/test/cssparsing.test.ts exactly: same files, same derivation,
// same census, same must-reject verdict. Read the long comment at the top of
// that file (or test/AGENTS.md) for WHY a kind sequence is compared and which
// upstream files are deliberately not wired — those exclusions are reasoned,
// not a skip list. Among the wired files every case runs and is asserted.
//
// The corpus is never committed; TestMain fetches it at a pinned commit. If
// it is absent these tests FAIL LOUDLY — they never skip.

import (
	"encoding/json"
	"os"
	"path/filepath"
	"reflect"
	"strconv"
	"testing"
)

// Pinned upstream. Keep in step with scripts/fetch-css-parsing-tests.sh and
// ts/test/cssparsing.test.ts.
const cpUpstream = "https://github.com/SimonSapin/css-parsing-tests"
const cpSHA = "203ce36bffd617db7f118c551e32794561fb273d"

func cpDir() string { return filepath.Join(repoRoot(), "test", "css-parsing-tests") }

func cpRequire(t *testing.T) {
	t.Helper()
	if _, err := os.Stat(filepath.Join(cpDir(), "stylesheet.json")); err != nil {
		t.Fatal(corpusMissing(cpDir(), cpSHA, "scripts/fetch-css-parsing-tests.sh"))
	}
}

type cpPair struct {
	input    string
	expected []any
}

func cpLoad(t *testing.T, file string) []cpPair {
	t.Helper()
	raw, err := os.ReadFile(filepath.Join(cpDir(), file))
	if err != nil {
		t.Fatalf("read %s: %v", file, err)
	}
	var flat []any
	if err := json.Unmarshal(raw, &flat); err != nil {
		t.Fatalf("parse %s: %v", file, err)
	}
	var out []cpPair
	for i := 0; i+1 < len(flat); i += 2 {
		in, ok := flat[i].(string)
		if !ok {
			t.Fatalf("%s: pair %d input is not a string", file, i/2)
		}
		exp, ok := flat[i+1].([]any)
		if !ok {
			t.Fatalf("%s: pair %d expected is not an array", file, i/2)
		}
		out = append(out, cpPair{input: in, expected: exp})
	}
	return out
}

func cpUpstreamKind(item any) string {
	arr, ok := item.([]any)
	if !ok || len(arr) == 0 {
		return "other:" + mustJSON(item)
	}
	head, _ := arr[0].(string)
	switch head {
	case "at-rule":
		name, _ := arr[1].(string)
		return "@" + name
	case "qualified rule":
		return "rule"
	case "declaration":
		name, _ := arr[1].(string)
		return "decl:" + name
	case "error":
		return "error"
	}
	return "other:" + mustJSON(head)
}

// cpNodeKind maps one tabnas AST node to a kind. Comment nodes return "" and
// are dropped: CSS Syntax discards comments, so upstream has no item to match.
func cpNodeKind(node any) string {
	m, ok := node.(map[string]any)
	if !ok {
		return "other:" + mustJSON(node)
	}
	typ, _ := m["type"].(string)
	switch typ {
	case "comment":
		return ""
	case "rule":
		return "rule"
	case "declaration":
		prop, _ := m["property"].(string)
		return "decl:" + prop
	}
	vendor, _ := m["vendor"].(string)
	return "@" + vendor + typ
}

func cpNodeKinds(nodes any) []string {
	out := []string{}
	arr, ok := nodes.([]any)
	if !ok {
		return out
	}
	for _, n := range arr {
		if k := cpNodeKind(n); k != "" {
			out = append(out, k)
		}
	}
	return out
}

func cpLabel(s string) string {
	q := strconv.Quote(s)
	if 60 < len(q) {
		return q[:57] + `..."`
	}
	return q
}

// cpRunFile: `wrap` turns a declaration-list/block-contents input into a whole
// stylesheet; `pick` pulls the matching node array back out of the result.
func cpRunFile(
	t *testing.T,
	file string,
	count int,
	wrap func(string) string,
	pick func(map[string]any) any,
) {
	t.Helper()
	cpRequire(t)
	pairs := cpLoad(t, file)

	t.Run(file+"/corpus census", func(t *testing.T) {
		if len(pairs) != count {
			t.Fatalf("expected %d cases in %s at %s@%s, found %d; the pinned "+
				"corpus changed. Do not silently accept a different suite.",
				count, file, cpUpstream, cpSHA, len(pairs))
		}
	})

	for _, p := range pairs {
		p := p
		want := []string{}
		mustReject := false
		for _, item := range p.expected {
			k := cpUpstreamKind(item)
			if k == "error" {
				mustReject = true
			}
			want = append(want, k)
		}

		verdict := "[accept] "
		if mustReject {
			verdict = "[reject] "
		}
		t.Run(file+"/"+verdict+cpLabel(p.input), func(t *testing.T) {
			src := wrap(p.input)
			got, err := parseCss(src, map[string]any{})

			if mustReject {
				if err == nil {
					t.Fatalf("CSS Syntax L3 reports a parse error for this "+
						"input (upstream expects %v); this plugin has no "+
						"error-recovery mode, so it must reject. Got: %s",
						want, mustJSON(got))
				}
				return
			}

			if err != nil {
				t.Fatalf("unexpected parse error: %v", err)
			}
			// An empty stylesheet parses to nil by design (AGENTS.md), and
			// upstream expects [] for it — that is a match.
			nodes := any([]any{})
			if got != nil {
				norm, nerr := jsonNorm(got)
				if nerr != nil {
					t.Fatalf("%v", nerr)
				}
				m, ok := norm.(map[string]any)
				if !ok {
					t.Fatalf("parse produced %#v, not a stylesheet", norm)
				}
				nodes = pick(m)
			}
			if g := cpNodeKinds(nodes); !reflect.DeepEqual(g, want) {
				t.Errorf("%s:\n  got  %v\n  want %v", cpLabel(p.input), g, want)
			}
		})
	}
}

func cpIdentity(s string) string  { return s }
func cpWrapBlock(s string) string { return "a{\n" + s + "\n}" }

func cpTopRules(ast map[string]any) any { return ast["rules"] }

// A declaration list / block contents has no standalone entry point here, so
// it is wrapped in a style rule and that rule's `declarations` read back out.
// CSS Nesting means nested rules and at-rules also land in `declarations`,
// which is exactly what blocks_contents.json expects.
func cpBlockDecls(ast map[string]any) any {
	rules, ok := ast["rules"].([]any)
	if !ok || len(rules) == 0 {
		return []any{}
	}
	first, ok := rules[0].(map[string]any)
	if !ok {
		return []any{}
	}
	return first["declarations"]
}

func TestCssParsingTests(t *testing.T) {
	cpRequire(t)
	cpRunFile(t, "stylesheet.json", 16, cpIdentity, cpTopRules)
	cpRunFile(t, "rule_list.json", 15, cpIdentity, cpTopRules)
	cpRunFile(t, "declaration_list.json", 10, cpWrapBlock, cpBlockDecls)
	cpRunFile(t, "blocks_contents.json", 13, cpWrapBlock, cpBlockDecls)
}
