// Copyright (c) 2025 Richard Rodger and other contributors, MIT License

package tabnascss

// conformance_test.go — corpus setup for the third-party conformance runner
// (reworkcss_test.go).
//
// The corpus is never vendored: scripts/fetch-reworkcss-tests.sh clones the
// pinned upstream commit into a gitignored directory. `TestMain` runs that
// script BEFORE any test in this package, exactly as the TypeScript side's
// `pretest` npm script does, so `go test ./...` always has the corpus.
//
// Why this file exists: without it nothing fetched the corpus for the Go
// runtime, so in CI (a clean checkout) `TestReworkcssCases` and
// `TestReworkcssAcceptReject` SKIPPED on every run — the Go half of the
// conformance claim was reported green while measuring nothing. It is now
// fetched here, and reworkcss_test.go FAILS rather than skips if it is still
// absent afterwards.
//
// If the fetch fails (offline, upstream unreachable) TestMain does NOT abort
// the whole package: it prints a warning and lets the conformance tests
// themselves fail loudly, so a network problem is still reported as a
// conformance test that did not run rather than as a silent pass.

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"testing"
)

func TestMain(m *testing.M) {
	script := filepath.Join("..", "scripts", "fetch-reworkcss-tests.sh")
	if _, err := os.Stat(script); err == nil {
		out, err := exec.Command("bash", script).CombinedOutput()
		if err != nil {
			fmt.Fprintf(os.Stderr,
				"WARNING: %s failed: %v\n%s\n"+
					"The conformance tests will FAIL (they never skip).\n",
				script, err, out)
		}
	} else {
		fmt.Fprintf(os.Stderr,
			"WARNING: %s not found; the conformance tests will FAIL if the "+
				"corpus is absent (they never skip).\n", script)
	}
	os.Exit(m.Run())
}
