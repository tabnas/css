// Copyright (c) 2026 Richard Rodger and other contributors, MIT License

package tabnascss

// conformance_test.go — shared setup for the two third-party conformance
// runners (reworkcss_test.go, cssparsing_test.go).
//
// Neither corpus is vendored: `scripts/fetch-suites.sh` clones each upstream
// repository at a PINNED commit into a gitignored directory. TestMain runs
// that script BEFORE any test, so `go test ./...` always has the corpora,
// exactly as the TypeScript side's `pretest` npm script does.
//
// If the fetch fails (offline, upstream gone) TestMain does NOT abort: the
// individual conformance tests then FAIL LOUDLY with a message naming the
// script. Nothing skips. A conformance suite that quietly does not run is
// worse than no suite at all, because the green tick is a lie.

import (
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"testing"

	jsonic "github.com/tabnas/jsonic/go"
)

func repoRoot() string { return ".." }

func TestMain(m *testing.M) {
	script := filepath.Join("..", "scripts", "fetch-suites.sh")
	if _, err := os.Stat(script); err == nil {
		cmd := exec.Command("bash", script)
		cmd.Dir = "."
		out, err := cmd.CombinedOutput()
		if err != nil {
			fmt.Fprintf(os.Stderr,
				"WARNING: %s failed: %v\n%s\n"+
					"The conformance tests below will FAIL (they never skip).\n",
				script, err, out)
		}
	}
	os.Exit(m.Run())
}

// corpusMissing builds the loud failure message used when a corpus is absent.
func corpusMissing(dir, sha, script string) string {
	return fmt.Sprintf(
		"MISSING CONFORMANCE CORPUS: %s\n"+
			"This third-party corpus is never committed. Fetch it (pinned to %s) with:\n\n"+
			"    bash %s\n\n"+
			"This test does NOT skip when the corpus is absent: a conformance "+
			"suite that quietly does not run is worse than no suite at all.",
		dir, sha, script)
}

// parseCss parses with the documented stack (jsonic engine + Css plugin).
func parseCss(src string, opts map[string]any) (any, error) {
	j := jsonic.Make()
	if err := j.UseDefaults(Css, Defaults, opts); err != nil {
		return nil, err
	}
	return j.Parse(src)
}

// jsonNorm normalises a value through JSON so *OrderedMap and map[string]any
// compare structurally against a corpus-decoded shape.
func jsonNorm(v any) (any, error) {
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
