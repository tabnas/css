/* Copyright (c) 2025 Richard Rodger and other contributors, MIT License */

// The divergence register, executed in the canonical runtime.
//
// `test/divergent.tsv` holds one row per input where this repo's three
// ports DISAGREE, with a cell per runtime. This file reads the `ts` column
// through @tabnas/support's DivergenceRegister, the same mechanism
// `go/divergent_test.go` and `rs/tests/divergent.rs` read the `go` and
// `rust` columns with.
//
// WHY THIS IS NOT A FIXTURE. A fixture fails when behaviour REGRESSES. The
// register fails both ways: a port repaired to agree with the others still
// faces a register claiming they differ, so the suite goes red and names
// the row to delete. That is what keeps a divergence from outliving its own
// repair, and it is why the file sits beside `test/spec/` rather than in
// it, where all three parity runners would run it.
//
// Every row here records a Go divergence from the canonical runtime, which
// is why the `ts` and `rust` cells agree: AGENTS.md makes TypeScript
// canonical, so the repair is in the Go port. See the "Known cross-runtime
// divergence" section there for the prose.

import { join } from 'node:path'

import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { findSpecDir, makeRegister } from '@tabnas/support'

import { Css } from '../dist/css'

const REPO = join(findSpecDir(__dirname), '..', '..')

makeRegister({
  runtime: 'ts',
  runtimes: ['ts', 'go', 'rust'],

  // A fresh Tabnas per row, as in parity.test.ts: the `opts` column is
  // per-case, and plugin options must not leak from one row into the next.
  parse: (input, row) => {
    const opts = row.named('opts')
    return new Tabnas()
      .use(jsonic)
      .use(Css, '' === opts.trim() ? {} : JSON.parse(opts))
      .parse(input)
  },

  // Compare after a JSON round trip, which is what the Go and Rust runners
  // do and what test/AGENTS.md states the contract to be. It decides a row
  // here: an unrecorded `position.end` is `undefined` in the live object and
  // absent once serialised, and only the second of those is what the `ts`
  // cell can hold.
  normalize: (value) => JSON.parse(JSON.stringify(value)),
}).file(join(REPO, 'test', 'divergent.tsv'))
