// Copyright (c) 2025 Richard Rodger and other contributors, MIT License

package tabnascss

// divergent_test.go — the divergence register, executed in the Go port.
//
// ../test/divergent.tsv holds one row per input where this repo's three
// ports DISAGREE, with a cell per runtime. This file reads the "go" column
// through github.com/tabnas/support/go's Register, the same mechanism
// ts/test/divergent.test.ts and rs/tests/divergent.rs read the "ts" and
// "rust" columns with.
//
// WHY THIS IS NOT A FIXTURE. A fixture fails when behaviour REGRESSES. The
// register fails both ways: a port repaired to agree with the others still
// faces a register claiming they differ, so the suite goes red and names
// the row to delete. That is why the file sits beside test/spec/ rather
// than in it, where TestSpec would run every row of it.
//
// Every row records a divergence of THIS port from the canonical
// TypeScript one, so the repair for each is here. The register is where
// they are pinned until it lands; ../AGENTS.md carries the prose under
// "Known cross-runtime divergence".

import (
	"encoding/json"
	"path/filepath"
	"testing"

	jsonic "github.com/tabnas/jsonic/go"
	support "github.com/tabnas/support/go"
)

func TestDivergenceRegister(t *testing.T) {
	dir, err := support.FindSpecDir("")
	if err != nil {
		t.Fatal(err)
	}

	support.Register{
		Runner: support.Runner{
			// A fresh parser per row, as in parity_test.go: the `opts`
			// column is per-case, and plugin options must not leak from
			// one row into the next.
			ParseRow: func(input string, row *support.Row) (any, error) {
				opts := map[string]any{}
				if raw := row.Named("opts"); "" != raw {
					if err := json.Unmarshal([]byte(raw), &opts); err != nil {
						return nil, err
					}
				}

				j := jsonic.Make()
				if err := j.UseDefaults(Css, Defaults, opts); err != nil {
					return nil, err
				}
				return j.Parse(input)
			},

			Normalize: jsonFlatten,
		},
		Runtime:  "go",
		Runtimes: []string{"ts", "go", "rust"},
	}.File(t, filepath.Join(dir, "..", "divergent.tsv"))
}
