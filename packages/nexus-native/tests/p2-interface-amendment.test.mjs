import { describe, test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
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

function agentHostConfigPath(home) {
  return join(home, '.nexus42', 'agent-host', 'config.toml');
}

function expectUninitialized(reject) {
  const wire = parseNativeCoreError(reject);
  assert.equal(wire?.code, 'uninitialized');
  return true;
}

describe('P2 interface amendment', { concurrency: 1 }, () => {
  test('allow_uninitialized opens status-only shell and denies operations', async () => {
    const home = mkdtempSync(join(tmpdir(), 'nexus-p2-uninit-'));
    const core = await openCore({
      user_home: home,
      access: 'direct_writer',
      allow_uninitialized: true,
    });

    try {
      await assert.rejects(() => core.activePrincipal(), expectUninitialized);
      await assert.rejects(
        () => core.worldKbGraph('principal', 'wld', false),
        expectUninitialized,
      );
      await assert.rejects(
        () =>
          core.patchWorldKbEntity('principal', 'wld', {
            entity_id: 'kb_x',
            expected_version: 0,
            patch: { title: 'Denied' },
          }),
        expectUninitialized,
      );
      await assert.rejects(
        () => core.worldKbCandidates('principal', 'wld', 10),
        expectUninitialized,
      );
      await assert.rejects(
        () => core.changes('principal', { after_sequence: '0' }),
        expectUninitialized,
      );
      await assert.rejects(
        () => core.hostQuery({ query: 'health' }),
        expectUninitialized,
      );
      await assert.rejects(
        () =>
          core.providerCall({
            request_id: 'deny-1',
            method: 'probe',
            deadline_ms: 1000,
            payload: {},
          }),
        expectUninitialized,
      );

    } finally {
      const report = await core.close();
      assert.equal(report.state, 'closed');
      assert.equal(report.cleanup_confirmed, true);
    }
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

  test('world kb invalid patch surfaces validation_summary', async () => {
    const home = seedHome();
    const core = await openCore({
      user_home: home,
      access: 'engine_owner',
      allow_uninitialized: false,
    });
    const principal = await core.activePrincipal();

    const invalidPatch = {
      entity_id: 'kb_def456',
      expected_version: 0,
      patch: { title: '   ', block_type: 'character' },
    };

    await assert.rejects(
      () => core.patchWorldKbEntity(principal, 'wld_owned', invalidPatch),
      (err) => {
        const wire = parseNativeCoreError(err);
        assert.equal(wire?.code, 'world_kb_validation');
        assert.ok(Array.isArray(wire?.details?.validation_summary?.errors));
        assert.ok(wire.details.validation_summary.errors.length > 0);
        return true;
      },
    );

    await core.close();
  });

  test('host query missing session returns sanitized not_found', async () => {
    const home = seedHome();
    const core = await openCore({
      user_home: home,
      access: 'engine_owner',
      allow_uninitialized: false,
    });

    await assert.rejects(
      () =>
        core.hostQuery({
          query: 'get_session',
          session_id: '00000000-0000-0000-0000-000000000099',
        }),
      (err) => {
        const wire = parseNativeCoreError(err);
        assert.equal(wire?.code, 'not_found');
        assert.equal(wire?.http_status, 404);
        assert.equal(wire?.message, 'session not found');
        assert.notEqual(err.message.includes('00000000'), true);
        return true;
      },
    );

    await core.close();
  });

  test('host query malformed session_id returns invalid_input', async () => {
    const home = seedHome();
    const core = await openCore({
      user_home: home,
      access: 'engine_owner',
      allow_uninitialized: false,
    });

    await assert.rejects(
      () => core.hostQuery({ query: 'get_session', session_id: 'not-a-uuid' }),
      (err) => {
        const wire = parseNativeCoreError(err);
        assert.equal(wire?.code, 'invalid_input');
        assert.equal(wire?.http_status, 400);
        assert.equal(wire?.message, 'invalid session_id');
        return true;
      },
    );

    await core.close();
  });

  test('failed initialized open cleans up and reopens after host config correction', async () => {
    const home = seedHome();
    const agentHostDir = dirname(agentHostConfigPath(home));
    mkdirSync(agentHostDir, { recursive: true });
    writeFileSync(agentHostConfigPath(home), 'not = [valid', 'utf8');

    await assert.rejects(
      () =>
        openCore({
          user_home: home,
          access: 'engine_owner',
          allow_uninitialized: false,
        }),
      (err) => {
        const wire = parseNativeCoreError(err);
        assert.equal(wire?.code, 'internal');
        assert.notEqual(err.message.includes('spoke'), true);
        assert.notEqual(err.message.includes('config.toml'), true);
        return true;
      },
    );

    rmSync(agentHostConfigPath(home));

    const core = await openCore({
      user_home: home,
      access: 'engine_owner',
      allow_uninitialized: false,
    });
    await core.activePrincipal();
    await core.close();
  });
});
