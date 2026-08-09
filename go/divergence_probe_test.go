// Copyright (c) 2025 Richard Rodger and other contributors, MIT License

package tabnascss

// divergence_probe_test.go — the Go half of the TS/Go differential probe
// driven by scripts/divergence-probe.sh.
//
// This is a TOOL, not a conformance assertion: it dumps this runtime's verdict
// for each input line so the script can diff it against the TypeScript
// runtime's. It is inert unless CSS_DUMP_IN and CSS_DUMP_OUT are set, which is
// why it does not breach the "a conformance test must never skip" rule — it
// asserts nothing about the parser either way.

import (
	"encoding/json"
	"os"
	"strings"
	"testing"

	jsonic "github.com/tabnas/jsonic/go"
)

// probeParse parses with the documented stack and reports failure as an error
// rather than a test failure, because a rejected input is a valid verdict.
func probeParse(src string) (any, error) {
	j := jsonic.Make()
	if err := j.UseDefaults(Css, Defaults, nil); err != nil {
		return nil, err
	}
	return j.Parse(src)
}

// probeNorm normalises a value through JSON so the Go and TS dumps are
// comparable shapes (the script sorts keys on both sides).
func probeNorm(v any) (any, error) {
	raw, err := json.Marshal(v)
	if err != nil {
		return nil, err
	}
	var out any
	if err := json.Unmarshal(raw, &out); err != nil {
		return nil, err
	}
	return out, nil
}

func TestDivergenceProbeDump(t *testing.T) {
	in := os.Getenv("CSS_DUMP_IN")
	out := os.Getenv("CSS_DUMP_OUT")
	if "" == in || "" == out {
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
		v, perr := probeParse(src)
		if nil != perr {
			b.WriteString(string(q) + "\tERR\n")
			continue
		}
		n, nerr := probeNorm(v)
		if nil != nerr {
			t.Fatalf("normalise %q: %v", src, nerr)
		}
		j, _ := json.Marshal(n)
		b.WriteString(string(q) + "\tOK " + string(j) + "\n")
	}

	if err := os.WriteFile(out, []byte(b.String()), 0o644); err != nil {
		t.Fatalf("write %s: %v", out, err)
	}
}
