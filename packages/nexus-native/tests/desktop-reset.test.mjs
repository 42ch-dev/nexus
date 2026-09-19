// Product local-state reset through the real unsigned native binding
// (v1.192 P0-T3R; plan row 19, compass D18/D20).
//
// The Rust integration test (`crates/nexus-local-db/tests/desktop_reset.rs`)
// proves the fence/busy and denial semantics against real files; this test
// proves the built `.node` payload loads and that the JS contract deletes
// exactly the product's state files and surfaces a structured refusal.

import { describe, test } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, lstatSync, mkdirSync, mkdtempSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { nativeCompatibility, resetLocalState } from '../dist/index.js';
import { parseNativeCoreError } from '../dist/errors.js';

const STATE_FILES = ['state.db', 'state.db-wal', 'state.db-shm'];

function makeHome() {
  return mkdtempSync(join(tmpdir(), 'nexus-desktop-reset-'));
}

function seedStore(home, creatorId, slug, names = STATE_FILES) {
  const dir = join(home, '.nexus42', 'creators', creatorId, 'workspaces', slug);
  mkdirSync(dir, { recursive: true });
  for (const name of names) writeFileSync(join(dir, name), `${name}-payload`);
  return dir;
}

describe('desktop local-state reset (real native binding)', () => {
  test('the built unsigned binding loads and reports compatibility', () => {
    const manifest = nativeCompatibility();
    assert.equal(manifest.napi_minimum, 8);
    assert.equal(manifest.writer_protocol, 1);
    assert.match(manifest.contract_tree_sha256, /^[a-f0-9]{64}$/);
  });

  test('resets exactly the product state files and preserves everything else', async () => {
    const home = makeHome();
    const store = seedStore(home, 'ctr_alpha', 'default');
    // Sibling workspace data and the store's stable admission locks stay.
    writeFileSync(join(store, 'state.db.migration.lock'), '');
    writeFileSync(join(store, 'state.db.engine.lock'), '');
    writeFileSync(join(store, 'workspace.toml'), 'keep');
    mkdirSync(join(store, 'kb', 'entries'), { recursive: true });
    writeFileSync(join(store, 'kb', 'entries', 'note.md'), 'keep');
    // A store holding only WAL/SHM siblings is an exact-file target too.
    const siblingsOnly = seedStore(home, 'ctr_beta', 'ws2', ['state.db-wal', 'state.db-shm']);
    // A non-workspace entry under `creators/` is not a store.
    writeFileSync(join(home, '.nexus42', 'creators', 'stray.txt'), 'keep');
    // The creative user workspace is never a reset target.
    const userDoc = join(home, 'Documents', 'nexus', 'default');
    mkdirSync(userDoc, { recursive: true });
    writeFileSync(join(userDoc, 'state.db'), 'user-document');

    assert.equal(await resetLocalState(home), 1, 'only a store owning state.db counts');

    for (const name of STATE_FILES) assert.equal(existsSync(join(store, name)), false);
    assert.equal(existsSync(join(siblingsOnly, 'state.db-wal')), false);
    assert.equal(existsSync(join(siblingsOnly, 'state.db-shm')), false);
    assert.equal(existsSync(join(store, 'workspace.toml')), true);
    assert.equal(existsSync(join(store, 'kb', 'entries', 'note.md')), true);
    assert.equal(existsSync(join(store, 'state.db.migration.lock')), true);
    assert.equal(existsSync(join(store, 'state.db.engine.lock')), true);
    assert.equal(existsSync(join(home, '.nexus42', 'creators', 'stray.txt')), true);
    assert.equal(existsSync(join(userDoc, 'state.db')), true);
  });

  test(
    'a symlinked state file is refused with a structured error and nothing is deleted',
    { skip: process.platform === 'win32' },
    async () => {
      const home = makeHome();
      const intact = seedStore(home, 'ctr_alpha', 'default');
      const escapeTarget = join(home, 'outside-state.db');
      writeFileSync(escapeTarget, 'outside');
      const linked = seedStore(home, 'ctr_beta', 'ws2', ['state.db']);
      symlinkSync(escapeTarget, join(linked, 'state.db-wal'));

      await assert.rejects(resetLocalState(home), (error) => {
        const wire = parseNativeCoreError(error);
        assert.equal(wire?.code, 'forbidden');
        assert.equal(wire?.details?.path, join(linked, 'state.db-wal'));
        return true;
      });

      // The refusal is a scan-time denial: nothing anywhere was deleted.
      for (const name of STATE_FILES) {
        assert.equal(existsSync(join(intact, name)), true);
      }
      assert.equal(existsSync(join(linked, 'state.db')), true);
      assert.equal(lstatSync(join(linked, 'state.db-wal')).isSymbolicLink(), true);
      assert.equal(existsSync(escapeTarget), true);
    },
  );

  test('a relative home is rejected at the facade before the native call', async () => {
    await assert.rejects(resetLocalState('relative-home'), (error) => {
      assert.match(error.message, /must be an absolute path/);
      assert.equal(
        parseNativeCoreError(error),
        null,
        'the facade refused it, so no native CoreError envelope exists',
      );
      return true;
    });
    // The empty/non-string boundary keeps its own rejection.
    await assert.rejects(resetLocalState(''), /must be a non-empty path string/);
  });

  test('a home without product state resets zero stores', async () => {
    assert.equal(await resetLocalState(makeHome()), 0);
  });
});
