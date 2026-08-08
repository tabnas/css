/* Copyright (c) 2026 Richard Rodger, MIT License */

// The exported VERSION must equal package.json "version".
//
// This is the CI check for version drift. It exists because the constant HAS
// drifted: @tabnas/json exported Version = '1.0.0' for several releases while
// the package shipped 0.4.x, because nothing rewrote it and AGENTS.md wrongly
// claimed `make publish-go` kept it in sync. A release that bumps
// package.json and forgets the constant now fails here.

import { describe, test } from 'node:test'
import assert from 'node:assert'

// Read through the package root (package.json "main"), not the built file
// directly: VERSION must be reachable to consumers of `@tabnas/css`, which is
// the property that actually matters. Both requires throw — never skip — if
// the file is missing or unreadable, because a version check that silently
// does not run is the failure mode this test exists to prevent.
const pkg = require('../package.json')
const api = require('..')

describe('version', () => {
  test('VERSION matches package.json', () => {
    assert.equal(
      api.VERSION,
      pkg.version,
      `VERSION drift: ${pkg.name} exports ${api.VERSION} but package.json is ` +
        `${pkg.version}. Both are rewritten by admin/publish.sh at release; ` +
        `if you bumped one by hand, bump the other.`,
    )
  })

  test('VERSION is exported and looks like a semver', () => {
    assert.equal(
      typeof api.VERSION,
      'string',
      'VERSION must be exported as a string',
    )
    assert.match(api.VERSION, /^\d+\.\d+\.\d+/, 'VERSION must be a semver')
  })
})
