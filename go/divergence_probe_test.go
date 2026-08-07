// Copyright (c) 2026 Richard Rodger and other contributors, MIT License

package tabnascss

// divergence_probe_test.go — the Go half of the TS/Go differential probe
// driven by scripts/divergence-probe.sh.
//
// This is a TOOL, not a conformance assertion: it dumps this runtime's
// verdict for each input line so the script can diff it against the
// TypeScript runtime's. It is inert (and says so) unless CSS_DUMP_IN and
// CSS_DUMP_OUT are set, which is why it does not violate the project's
// "a conformance test must never skip" rule — it asserts nothing. The
// divergences it FOUND are pinned as failing fixtures in
// test/spec/divergence.tsv, which does run unconditionally in both runtimes.

import (
	"encoding/json"
	"os"
	"strings"
	"testing"
)

func TestDivergenceProbeDump(t *testing.T) {
	in := os.Getenv("CSS_DUMP_IN")
	out := os.Getenv("CSS_DUMP_OUT")
	if in == "" || out == "" {
		t.Log("inert: set CSS_DUMP_IN and CSS_DUMP_OUT to dump verdicts " +
			"(see scripts/divergence-probe.sh). This helper asserts nothing.")
		return
	}
	raw, err := os.ReadFile(in)
	if err != nil {
		t.Fatalf("read %s: %v", in, err)
	}
	var b strings.Builder
	for _, src := range strings.Split(string(raw), "\n") {
		q, _ := json.Marshal(src)
		v, perr := parseCss(src, map[string]any{})
		if perr != nil {
			b.WriteString(string(q) + "\tERR\n")
			continue
		}
		n, nerr := jsonNorm(v)
		if nerr != nil {
			t.Fatalf("normalise %q: %v", src, nerr)
		}
		j, _ := json.Marshal(n)
		b.WriteString(string(q) + "\tOK " + string(j) + "\n")
	}
	if err := os.WriteFile(out, []byte(b.String()), 0o644); err != nil {
		t.Fatalf("write %s: %v", out, err)
	}
}
