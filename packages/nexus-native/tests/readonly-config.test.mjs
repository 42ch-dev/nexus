#!/usr/bin/env node
// Read-only legacy-snapshot loading through the real native binding.
//
// The Rust integration test (`crates/nexus-core/tests/readonly_config_open.rs`)
// proves the service semantics; this suite proves the native prerequisite path
// (`validate_initialized_prerequisites` + `open_core`) makes the same choice:
// a sealed read-only home loads a legacy `config.json` snapshot without taking
// the lock, publishing `config.toml`, renaming the legacy file or creating a
// directory entry, while a writer open still migrates.
//
// LOCAL ONLY — not part of `pnpm test` and not wired into CI: it seeds through
// `cargo run -q -p nexus-core-node --bin native-wire-fixture-seed`, an uncached
// debug build of the whole core that no CI job stages. The Rust half runs in
// `crates/nexus-core/tests/readonly_config_open.rs`.

import { describe, test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import {
  chmodSync,
  existsSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { openCore } from '../dist/index.js';

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');

/** A provisioned home (config.toml + workspace DB) via the fixture seed bin. */
function seedHome() {
  const home = mkdtempSync(join(tmpdir(), 'nexus-readonly-config-'));
  const seeded = spawnSync(
    'cargo',
    ['run', '-q', '-p', 'nexus-core-node', '--bin', 'native-wire-fixture-seed', '--', home],
    { cwd: root, encoding: 'utf8' },
  );
  assert.equal(seeded.status, 0, `the fixture seed failed: ${seeded.stderr}`);
  return home;
}

/** Replace the provisioned `config.toml` with the legacy two-field snapshot. */
function legacyHome() {
  const home = seedHome();
  const nexusHome = join(home, '.nexus42');
  const [creator] = readdirSync(join(nexusHome, 'creators'));
  const [slug] = readdirSync(join(nexusHome, 'creators', creator, 'workspaces'));
  writeFileSync(
    join(nexusHome, 'config.json'),
    JSON.stringify({
      active_creator_id: creator,
      active_workspace_slug_by_creator: { [creator]: slug },
    }),
  );
  rmSync(join(nexusHome, 'config.toml'));
  return { home, nexusHome };
}

describe('read-only legacy config (real native binding)', () => {
  test('a sealed read-only open loads the snapshot without writing', async (t) => {
    const { home, nexusHome } = legacyHome();
    t.after(() => {
      chmodSync(nexusHome, 0o755);
      rmSync(home, { recursive: true, force: true });
    });
    const before = readdirSync(nexusHome).sort();

    chmodSync(nexusHome, 0o555);
    let core;
    try {
      core = await openCore({ user_home: home, access: 'read_only' });
    } finally {
      chmodSync(nexusHome, 0o755);
    }
    await core.close();

    assert.equal(existsSync(join(nexusHome, 'config.toml')), false, 'no TOML published');
    assert.equal(existsSync(join(nexusHome, 'config.json')), true, 'the legacy file stays');
    assert.equal(existsSync(join(nexusHome, 'config.json.migrated')), false, 'no rename');
    assert.deepEqual(readdirSync(nexusHome).sort(), before, 'no entry was created');
  });

  test('a writer open still migrates the legacy snapshot', async (t) => {
    const { home, nexusHome } = legacyHome();
    t.after(() => rmSync(home, { recursive: true, force: true }));

    const writer = await openCore({ user_home: home, access: 'engine_owner' });
    await writer.close();

    const published = readFileSync(join(nexusHome, 'config.toml'), 'utf8');
    assert.match(published, /active_creator_id\s*=\s*"ctr_/);
    assert.equal(existsSync(join(nexusHome, 'config.json.migrated')), true);
    assert.equal(existsSync(join(nexusHome, 'config.json')), false);
    assert.equal(existsSync(join(nexusHome, 'config.toml.lock')), false);
  });
});
