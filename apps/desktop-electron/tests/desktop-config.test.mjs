#!/usr/bin/env node
/**
 * Desktop config tests (v1.192 P0-T2).
 *
 * Defends the frozen config contract without launching Electron: workspace-root
 * precedence (per-creator map → legacy mirror → `<documents>/nexus/default`),
 * bootstrap idempotence, creator switching (map + mirror + default slug),
 * corrupt-document non-overwrite, unrelated-key retention, entrance /
 * `setup_completed` defaults, and the agent-profile upsert. Every home is a
 * temporary directory — real user data is never touched. Runs against the
 * compiled output:
 *   pnpm --dir apps/desktop-electron run build && node --test apps/desktop-electron/tests/desktop-config.test.mjs
 */
import assert from 'node:assert/strict';
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test from 'node:test';
import { parse } from 'smol-toml';
import { createDesktopConfig } from '../dist/desktop-config.js';

const configPath = (home) => join(home, '.nexus42', 'config.toml');
const agentPath = (home) => join(home, '.nexus42', 'agent-host', 'config.toml');
const readText = (path) => readFileSync(path, 'utf8');
const parseToml = (path) => parse(readText(path), { integersAsBigInt: 'asNeeded' });

function writeTomlFile(path, text) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, text, 'utf8');
}

/** One temporary home + documents directory with a config bound to both. */
function setup(t) {
  const home = mkdtempSync(join(tmpdir(), 'nexus-desktop-config-'));
  t.after(() => rmSync(home, { recursive: true, force: true }));
  const documents = join(home, 'Documents');
  return { home, documents, config: createDesktopConfig(home, documents) };
}

function expectCode(promise, code) {
  return assert.rejects(promise, (err) => err.code === code);
}

// ---------------------------------------------------------------------------
// Trusted inputs
// ---------------------------------------------------------------------------

test('trusted home and documents path are required', () => {
  assert.throws(() => createDesktopConfig('', '/tmp/Documents'), (err) => err.code === 'invalid_input');
  assert.throws(() => createDesktopConfig('/tmp/home', ''), (err) => err.code === 'invalid_input');
});

test('factory exposes the P0-T2 handler map plus resolveWorkspaceRoot', async (t) => {
  const { config } = setup(t);
  assert.deepEqual(Object.keys(config).sort(), [
    'ensure_setup_bootstrap',
    'get_agent_profile',
    'get_entrance',
    'get_setup_completed',
    'get_workspace_root',
    'resolveWorkspaceRoot',
    'set_agent_profile',
    'set_entrance',
    'set_setup_completed',
    'set_workspace_path',
    'switch_active_creator',
  ]);
  // `resolveWorkspaceRoot` is the same authority the display read uses (P0-T3).
  assert.equal(await config.resolveWorkspaceRoot(), await config.get_workspace_root());
});

// ---------------------------------------------------------------------------
// Workspace root resolution
// ---------------------------------------------------------------------------

test('workspace root precedence: per-creator map, legacy mirror, default slug', async (t) => {
  const { home, documents, config } = setup(t);
  const fallback = join(documents, 'nexus', 'default');

  // No config at all — default slug, and the read creates no file.
  assert.equal(await config.get_workspace_root(), fallback);
  assert.equal(existsSync(configPath(home)), false);

  // Legacy mirror only.
  writeTomlFile(configPath(home), 'active_creator_id = "ctr_a"\nworkspace_path = "/legacy/ws"\n');
  assert.equal(await config.get_workspace_root(), '/legacy/ws');

  // Per-creator entry wins over the mirror.
  writeTomlFile(
    configPath(home),
    [
      'active_creator_id = "ctr_a"',
      'workspace_path = "/legacy/ws"',
      '',
      '[workspace_path_by_creator]',
      'ctr_a = "/map/ws"',
      'ctr_b = "/other/ws"',
      '',
    ].join('\n'),
  );
  assert.equal(await config.get_workspace_root(), '/map/ws');

  // Map present but without an entry for the active creator — mirror, not
  // another creator's path.
  writeTomlFile(
    configPath(home),
    [
      'active_creator_id = "ctr_a"',
      'workspace_path = "/legacy/ws"',
      '',
      '[workspace_path_by_creator]',
      'ctr_b = "/other/ws"',
      '',
    ].join('\n'),
  );
  assert.equal(await config.get_workspace_root(), '/legacy/ws');

  // No active creator — mirror, then the default slug.
  writeTomlFile(configPath(home), 'workspace_path = "/legacy/ws"\n');
  assert.equal(await config.get_workspace_root(), '/legacy/ws');
  writeTomlFile(configPath(home), 'active_creator_id = "ctr_a"\n');
  assert.equal(await config.get_workspace_root(), fallback);
});

// ---------------------------------------------------------------------------
// Setup bootstrap
// ---------------------------------------------------------------------------

test('bootstrap is idempotent, generates ctr_local + 12 hex, and never replaces an existing creator', async (t) => {
  const { home, config } = setup(t);
  const first = await config.ensure_setup_bootstrap();
  assert.match(first.creator_id, /^ctr_local[0-9a-f]{12}$/);
  assert.equal(first.already_bootstrapped, false);

  const doc = parseToml(configPath(home));
  assert.equal(doc.active_creator_id, first.creator_id);
  assert.equal(doc.active_workspace_slug_by_creator[first.creator_id], 'default');

  // A hand-added comment survives the second call: an existing creator is
  // reported without rewriting the document.
  writeTomlFile(configPath(home), `${readText(configPath(home))}\n# keep me\n`);
  assert.deepEqual(await config.ensure_setup_bootstrap(), {
    creator_id: first.creator_id,
    already_bootstrapped: true,
  });
  assert.match(readText(configPath(home)), /# keep me/);
});

test('concurrent bootstrap calls create exactly one creator', async (t) => {
  const { config } = setup(t);
  const results = await Promise.all([
    config.ensure_setup_bootstrap(),
    config.ensure_setup_bootstrap(),
    config.ensure_setup_bootstrap(),
  ]);
  const ids = new Set(results.map((result) => result.creator_id));
  assert.equal(ids.size, 1);
  assert.equal(results.filter((result) => !result.already_bootstrapped).length, 1);
  assert.match([...ids][0], /^ctr_local[0-9a-f]{12}$/);
});

// ---------------------------------------------------------------------------
// Workspace path write and creator switch
// ---------------------------------------------------------------------------

test('set_workspace_path requires an active creator, then updates map and mirror', async (t) => {
  const { home, config } = setup(t);
  await expectCode(config.set_workspace_path({ path: '/picked/ws' }), 'no_active_creator');
  assert.equal(existsSync(configPath(home)), false);

  const { creator_id } = await config.ensure_setup_bootstrap();
  assert.equal(await config.set_workspace_path({ path: '/picked/ws' }), null);
  assert.equal(await config.get_workspace_root(), '/picked/ws');

  const doc = parseToml(configPath(home));
  assert.equal(doc.workspace_path_by_creator[creator_id], '/picked/ws');
  assert.equal(doc.workspace_path, '/picked/ws');
});

test('creator switch updates map, mirror and default slug, preserving other creators', async (t) => {
  const { home, documents, config } = setup(t);
  const fallback = join(documents, 'nexus', 'default');
  const { creator_id: first } = await config.ensure_setup_bootstrap();
  await config.set_workspace_path({ path: '/first/ws' });

  assert.equal(await config.switch_active_creator({ creatorId: 'ctr_second' }), fallback);
  const doc = parseToml(configPath(home));
  assert.equal(doc.active_creator_id, 'ctr_second');
  assert.equal(doc.workspace_path_by_creator.ctr_second, fallback);
  assert.equal(doc.workspace_path_by_creator[first], '/first/ws');
  assert.equal(doc.workspace_path, fallback);
  assert.equal(doc.active_workspace_slug_by_creator.ctr_second, 'default');
  assert.equal(await config.get_workspace_root(), fallback);

  // A creator that already owns a path keeps it across a round trip.
  await config.set_workspace_path({ path: '/second/ws' });
  await config.switch_active_creator({ creatorId: first });
  assert.equal(await config.switch_active_creator({ creatorId: 'ctr_second' }), '/second/ws');
  assert.equal(await config.get_workspace_root(), '/second/ws');
});

test('creator switch rejects separators and traversal without writing', async (t) => {
  const { home, config } = setup(t);
  await config.ensure_setup_bootstrap();
  const before = readText(configPath(home));
  for (const invalid of ['../evil', 'a/b', 'a\\b']) {
    await expectCode(config.switch_active_creator({ creatorId: invalid }), 'invalid_input');
  }
  assert.equal(readText(configPath(home)), before);
});

test('serialized mutations keep both creators in the per-creator map', async (t) => {
  const { home, documents, config } = setup(t);
  await config.ensure_setup_bootstrap();
  const fallback = join(documents, 'nexus', 'default');
  await Promise.all([
    config.switch_active_creator({ creatorId: 'ctr_one' }),
    config.switch_active_creator({ creatorId: 'ctr_two' }),
  ]);
  const doc = parseToml(configPath(home));
  assert.equal(doc.workspace_path_by_creator.ctr_one, fallback);
  assert.equal(doc.workspace_path_by_creator.ctr_two, fallback);
});

// ---------------------------------------------------------------------------
// Entrance and setup_completed
// ---------------------------------------------------------------------------

test('entrance: missing default, enum round-trip, invalid stored value, no mutation on read', async (t) => {
  const { home, config } = setup(t);
  assert.equal(await config.get_entrance(), 'content-creator');
  assert.equal(existsSync(configPath(home)), false);

  assert.equal(await config.set_entrance({ value: 'developer' }), null);
  assert.equal(await config.get_entrance(), 'developer');
  assert.equal(await config.set_entrance({ value: 'content-creator' }), null);
  assert.equal(await config.get_entrance(), 'content-creator');

  writeTomlFile(configPath(home), 'entrance = "banana"\n');
  await expectCode(config.get_entrance(), 'invalid_input');

  // An unreadable document keeps the documented default (retired fail-soft read).
  writeTomlFile(configPath(home), 'not toml {{{\n');
  assert.equal(await config.get_entrance(), 'content-creator');
});

test('setup_completed: absent is false, writes round-trip, unreadable reads false', async (t) => {
  const { home, config } = setup(t);
  assert.equal(await config.get_setup_completed(), false);
  assert.equal(existsSync(configPath(home)), false);

  assert.equal(await config.set_setup_completed({ value: true }), null);
  assert.equal(await config.get_setup_completed(), true);
  assert.equal(await config.set_setup_completed({ value: false }), null);
  assert.equal(await config.get_setup_completed(), false);

  writeTomlFile(configPath(home), 'not toml {{{\n');
  assert.equal(await config.get_setup_completed(), false);
});

// ---------------------------------------------------------------------------
// Corrupt documents and unrelated-key retention
// ---------------------------------------------------------------------------

test('corrupt config is never replaced with an empty document and leaves no temp litter', async (t) => {
  const { home, config } = setup(t);
  const corrupt = 'this is not valid toml {{{\n';
  writeTomlFile(configPath(home), corrupt);

  const writes = [
    () => config.set_workspace_path({ path: '/ws' }),
    () => config.set_setup_completed({ value: true }),
    () => config.set_entrance({ value: 'developer' }),
    () => config.switch_active_creator({ creatorId: 'ctr_a' }),
    () => config.ensure_setup_bootstrap(),
  ];
  for (const write of writes) {
    await expectCode(write(), 'config_corrupt');
  }
  // The authoritative root read fails closed rather than fabricating a root.
  await expectCode(config.get_workspace_root(), 'config_corrupt');

  assert.equal(readText(configPath(home)), corrupt);
  assert.deepEqual(readdirSync(join(home, '.nexus42')), ['config.toml']);
});

test('mutations preserve unrelated keys and their value types', async (t) => {
  const { home, config } = setup(t);
  writeTomlFile(
    configPath(home),
    [
      'active_creator_id = "ctr_a"',
      'future_key = "keep"',
      'future_flag = true',
      'future_big = 9007199254740993',
      'future_arr = ["a", "b", 3]',
      'future_float = 1.5',
      '',
      '[future_table]',
      'nested = "value"',
      '',
    ].join('\n'),
  );

  await config.set_setup_completed({ value: true });
  await config.set_entrance({ value: 'developer' });
  await config.set_workspace_path({ path: '/ws' });
  await config.switch_active_creator({ creatorId: 'ctr_b' });

  const doc = parseToml(configPath(home));
  assert.equal(doc.future_key, 'keep');
  assert.equal(doc.future_flag, true);
  assert.equal(doc.future_big, 9007199254740993n);
  assert.deepEqual(doc.future_arr, ['a', 'b', 3]);
  assert.equal(doc.future_float, 1.5);
  assert.equal(doc.future_table.nested, 'value');
  assert.equal(doc.setup_completed, true);
  assert.equal(doc.entrance, 'developer');
  assert.equal(doc.active_creator_id, 'ctr_b');
  assert.equal(doc.workspace_path_by_creator.ctr_a, '/ws');
  assert.deepEqual(readdirSync(join(home, '.nexus42')), ['config.toml']);
});

// ---------------------------------------------------------------------------
// Agent profile (`~/.nexus42/agent-host/config.toml`)
// ---------------------------------------------------------------------------

test('agent profile: null when absent, native_cli upsert, other providers and keys preserved', async (t) => {
  const { home, config } = setup(t);
  assert.equal(await config.get_agent_profile(), null);
  assert.equal(existsSync(agentPath(home)), false);

  assert.equal(
    await config.set_agent_profile({ name: 'claude', launchCommand: 'claude --flag' }),
    null,
  );
  assert.deepEqual(await config.get_agent_profile(), {
    name: 'claude',
    launchCommand: 'claude --flag',
  });

  writeTomlFile(
    agentPath(home),
    [
      'future_key = "keep"',
      '',
      '[[providers]]',
      'id = "other-provider"',
      'protocol = "http"',
      'endpoint = "https://example.test"',
      '',
      '[[providers]]',
      'id = "old-native"',
      'protocol = "native_cli"',
      'command = "old"',
      '',
    ].join('\n'),
  );

  await config.set_agent_profile({ name: 'codex' });
  assert.deepEqual(await config.get_agent_profile(), { name: 'codex' });

  const doc = parseToml(agentPath(home));
  assert.equal(doc.future_key, 'keep');
  assert.equal(doc.providers.length, 2);
  assert.deepEqual(doc.providers[0], {
    id: 'other-provider',
    protocol: 'http',
    endpoint: 'https://example.test',
  });
  assert.deepEqual(doc.providers[1], { id: 'codex', protocol: 'native_cli' });
});

test('agent profile read skips malformed rows; unreadable documents never clobber', async (t) => {
  const { home, config } = setup(t);
  writeTomlFile(
    agentPath(home),
    [
      '[[providers]]',
      'protocol = "native_cli"',
      '',
      '[[providers]]',
      'id = ""',
      'protocol = "native_cli"',
      '',
      '[[providers]]',
      'id = "http-one"',
      'protocol = "http"',
      '',
      '[[providers]]',
      'id = "real"',
      'protocol = "native_cli"',
      'command = "run me"',
      '',
    ].join('\n'),
  );
  assert.deepEqual(await config.get_agent_profile(), { name: 'real', launchCommand: 'run me' });

  const corrupt = 'not toml {{{\n';
  writeTomlFile(agentPath(home), corrupt);
  assert.equal(await config.get_agent_profile(), null);
  await expectCode(config.set_agent_profile({ name: 'x' }), 'config_corrupt');
  assert.equal(readText(agentPath(home)), corrupt);
});

test('a saved launch command is never executed here', async (t) => {
  const { home, config } = setup(t);
  const sentinel = join(home, 'sentinel');
  await config.set_agent_profile({ name: 'danger', launchCommand: `touch ${sentinel}` });
  assert.equal(existsSync(sentinel), false);
  assert.deepEqual(await config.get_agent_profile(), {
    name: 'danger',
    launchCommand: `touch ${sentinel}`,
  });
});
