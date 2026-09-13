import { describe, test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { parseNativeCoreError } from '../dist/errors.js';
import { openCore } from '../dist/index.js';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = join(__dirname, '..', '..', '..');

function seedHome() {
  const home = mkdtempSync(join(tmpdir(), 'nexus-p2-amend-'));
  spawnSync(
    'cargo',
    ['run', '-q', '-p', 'nexus-core-node', '--bin', 'native-wire-fixture-seed', '--', home],
    { cwd: root },
  );
  return home;
}

describe('P2 interface amendment', () => {
  test('allow_uninitialized opens status-only shell and denies effects', async () => {
    const home = mkdtempSync(join(tmpdir(), 'nexus-p2-uninit-'));
    const core = await openCore({
      user_home: home,
      access: 'direct_writer',
      allow_uninitialized: true,
    });

    await assert.rejects(
      () => core.activePrincipal(),
      (err) => {
        const wire = parseNativeCoreError(err);
        assert.equal(wire?.code, 'uninitialized');
        return true;
      },
    );

    const report = await core.close();
    assert.equal(report.state, 'closed');
    assert.equal(report.cleanup_confirmed, true);
  });

  test('world kb stale patch surfaces structured conflict details', async () => {
    const home = seedHome();
    const core = await openCore({
      user_home: home,
      access: 'engine_owner',
      allow_uninitialized: false,
    });
    const principal = await core.activePrincipal();

    await core.worldKbGraph(principal, 'wld_owned', false);

    const stalePatch = {
      entity_id: 'kb_cas',
      expected_version: 1,
      patch: { title: 'Stale' },
    };

    await assert.rejects(
      () => core.patchWorldKbEntity(principal, 'wld_owned', stalePatch),
      (err) => {
        const wire = parseNativeCoreError(err);
        assert.equal(wire?.code, 'world_kb_conflict');
        assert.equal(wire?.details?.entity_id, 'kb_cas');
        assert.equal(wire?.details?.current_version, 2);
        assert.equal(typeof wire?.details?.conflicting_path, 'string');
        assert.equal(typeof wire?.details?.recovery_hint, 'string');
        return true;
      },
    );

    await core.close();
  });
});
