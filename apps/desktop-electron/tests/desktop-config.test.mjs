#!/usr/bin/env node
/**
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
  chmodSync,
  chownSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
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
// re-establish the plain-object shape these assertions pin, mirroring the
// normalization at the product's own parse seam in src/desktop-config.ts.
const toPlainToml = (value) =>
  Array.isArray(value)
    ? value.map(toPlainToml)
    : value && typeof value === 'object' && !(value instanceof Date)
      ? Object.fromEntries(Object.entries(value).map(([key, child]) => [key, toPlainToml(child)]))
      : value;
const parseToml = (path) =>
  toPlainToml(parse(readText(path), { integersAsBigInt: 'asNeeded' }));

/**
 * when it belongs to no other group. Without one a staged-and-renamed file
 * always inherits the process egid, so a captured-group assertion cannot fail.
 */
function foreignGroup() {
  const egid = process.getegid?.();
  return process.getgroups?.().find((gid) => gid !== egid) ?? null;
}

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
  assert.equal(await config.resolveWorkspaceRoot(), await config.get_workspace_root());
});


test('workspace root precedence: per-creator map, legacy mirror, default slug', async (t) => {
  const { home, documents, config } = setup(t);
  const fallback = join(documents, 'nexus', 'default');

  assert.equal(await config.get_workspace_root(), fallback);
  assert.equal(existsSync(configPath(home)), false);

  writeTomlFile(configPath(home), 'active_creator_id = "ctr_a"\nworkspace_path = "/legacy/ws"\n');
  assert.equal(await config.get_workspace_root(), '/legacy/ws');

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

  writeTomlFile(configPath(home), 'workspace_path = "/legacy/ws"\n');
  assert.equal(await config.get_workspace_root(), '/legacy/ws');
  writeTomlFile(configPath(home), 'active_creator_id = "ctr_a"\n');
  assert.equal(await config.get_workspace_root(), fallback);
});


test('bootstrap is idempotent, generates ctr_local + 12 hex, and never replaces an existing creator', async (t) => {
  const { home, config } = setup(t);
  const first = await config.ensure_setup_bootstrap();
  assert.match(first.creator_id, /^ctr_local[0-9a-f]{12}$/);
  assert.equal(first.already_bootstrapped, false);

  const doc = parseToml(configPath(home));
  assert.equal(doc.active_creator_id, first.creator_id);
  assert.equal(doc.active_workspace_slug_by_creator[first.creator_id], 'default');

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

  writeTomlFile(configPath(home), 'not toml {{{\n');
  assert.equal(await config.get_entrance(), 'content-creator');
});

test('set_entrance rejects a non-enum value at the handler boundary, byte-preserving', async (t) => {
  const { home, config } = setup(t);
  await expectCode(config.set_entrance({ value: 'banana' }), 'invalid_input');
  assert.equal(existsSync(configPath(home)), false);

  const stored = 'entrance = "developer"\nfuture_key = "keep"\n';
  writeTomlFile(configPath(home), stored);
  for (const invalid of ['banana', '', 'Developer', 'content_creator']) {
    await expectCode(config.set_entrance({ value: invalid }), 'invalid_input');
  }
  assert.equal(readText(configPath(home)), stored);
  assert.equal(await config.get_entrance(), 'developer');

  assert.equal(await config.set_entrance({ value: 'content-creator' }), null);
  assert.equal(await config.get_entrance(), 'content-creator');
  assert.deepEqual(readdirSync(join(home, '.nexus42')), ['config.toml']);
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
  await expectCode(config.get_workspace_root(), 'config_corrupt');

  assert.equal(readText(configPath(home)), corrupt);
  assert.deepEqual(readdirSync(join(home, '.nexus42')), ['config.toml']);
});


test('written documents are owner-only (0600)', async (t) => {
  const { home, config } = setup(t);
  await config.ensure_setup_bootstrap();
  await config.set_agent_profile({ name: 'claude', launchCommand: 'claude' });

  // neither the materialized copy nor the document is readable by other users.
  assert.equal(statSync(configPath(home)).mode & 0o777, 0o600);
  assert.equal(statSync(agentPath(home)).mode & 0o777, 0o600);
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

test('agent profile write fails closed on non-table provider rows, byte-preserving', async (t) => {
  const { home, config } = setup(t);
  const cases = [
    'providers = ["not-a-provider"]\nfuture_key = "keep"\n',
    'providers = [{ id = "ok", protocol = "http" }, "bad"]\n',
    'providers = [42]\n',
  ];
  for (const malformed of cases) {
    writeTomlFile(agentPath(home), malformed);
    await expectCode(config.set_agent_profile({ name: 'codex' }), 'config_corrupt');
    assert.equal(readText(agentPath(home)), malformed);
  }
  assert.deepEqual(readdirSync(join(home, '.nexus42', 'agent-host')), ['config.toml']);

  writeTomlFile(
    agentPath(home),
    '[[providers]]\nid = "other"\nprotocol = "http"\n',
  );
  assert.equal(await config.set_agent_profile({ name: 'codex' }), null);
  const doc = parseToml(agentPath(home));
  assert.deepEqual(doc.providers, [
    { id: 'other', protocol: 'http' },
    { id: 'codex', protocol: 'native_cli' },
  ]);
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

/**
 * QC1-002 (I-000105, contract §2): the whole line is the token. A foreign lock
 * that merely ends with our token line — a prefix was prepended — must never
 * be unlinked; only byte equality proves this holder still owns it.
 */
test('a release never unlinks a lock that only ends with our token', async (t) => {
  const source = readFileSync(new URL('../dist/desktop-config.js', import.meta.url), 'utf8');
  // lock is taken) so the lock can be rewritten underneath it.
  const marker = 'await writeDocument(path, doc);';
  assert.ok(source.includes(marker), 'the write marker must exist in dist/desktop-config.js');
  const copyUrl = new URL(
    `../dist/__rewritten-desktop-config-${process.pid}.mjs`,
    import.meta.url,
  );
  writeFileSync(
    copyUrl,
    source.replace(
      marker,
      `await new Promise((resolve) => setTimeout(resolve, 250)); ${marker}`,
    ),
    'utf8',
  );
  try {
    const { createDesktopConfig: createSlowConfig } = await import(copyUrl.href);
    const home = mkdtempSync(join(tmpdir(), 'nexus-desktop-lock-'));
    t.after(() => rmSync(home, { recursive: true, force: true }));
    const config = createSlowConfig(home, join(home, 'Documents'));
    writeTomlFile(configPath(home), 'active_creator_id = "ctr_a"\n');
    const lockPath = `${configPath(home)}.lock`;

    const pending = config.set_workspace_path({ path: '/ws' });
    // still ends with our token, so a suffix compare would unlink it.
    const lockDeadline = Date.now() + 2000;
    let held = '';
    while (!held.endsWith('\n')) {
      assert.ok(Date.now() < lockDeadline, 'the writer must write the lock within 2000 ms');
      held = existsSync(lockPath) ? readFileSync(lockPath, 'utf8') : '';
      if (!held.endsWith('\n')) await new Promise((resolve) => setTimeout(resolve, 5));
    }
    const rewritten = `foreign-prefix ${held}`;
    writeFileSync(lockPath, rewritten, 'utf8');

    await pending;
    assert.equal(existsSync(lockPath), true, 'a foreign lock must never be unlinked');
    assert.equal(readFileSync(lockPath, 'utf8'), rewritten);
    assert.equal(parseToml(configPath(home)).workspace_path, '/ws');
  } finally {
    rmSync(copyUrl, { force: true });
  }
});


/**
 * process may not write is refused instead of replaced by a rename that the
 * directory — not the file — authorized.
 */
test('a write-denied document is refused and keeps its bytes', async (t) => {
  const { home, config } = setup(t);
  const daemon = 'active_creator_id = "ctr_locked"\n';
  const agent = 'future_key = "keep"\n';
  writeTomlFile(configPath(home), daemon);
  writeTomlFile(agentPath(home), agent);
  chmodSync(configPath(home), 0o444);
  chmodSync(agentPath(home), 0o444);
  try {
    await expectCode(config.set_entrance({ value: 'developer' }), 'config_publish_refused');
    await expectCode(config.set_agent_profile({ name: 'codex' }), 'config_publish_refused');
  } finally {
    chmodSync(configPath(home), 0o600);
    chmodSync(agentPath(home), 0o600);
  }

  assert.equal(readText(configPath(home)), daemon);
  assert.equal(readText(agentPath(home)), agent);
  assert.deepEqual(readdirSync(join(home, '.nexus42')).sort(), ['agent-host', 'config.toml']);
  assert.deepEqual(readdirSync(join(home, '.nexus42', 'agent-host')), ['config.toml']);
});

/**
 * policy and reapplies it before the rename, so publication never silently
 * tightens or widens access.
 */
test('an existing 0640 document keeps its mode and group', async (t) => {
  const group = foreignGroup();
  if (group === null) {
    t.skip(
      'no supplementary group distinct from the process egid: a captured-group assertion cannot fail here',
    );
    return;
  }
  const { home, config } = setup(t);
  writeTomlFile(configPath(home), 'active_creator_id = "ctr_a"\n');
  writeTomlFile(agentPath(home), 'future_key = "keep"\n');
  const paths = [configPath(home), agentPath(home)];
  for (const path of paths) {
    chmodSync(path, 0o640);
    chownSync(path, -1, group);
    assert.equal(statSync(path).gid, group, 'the fixture group differs from the process egid');
  }

  await config.set_entrance({ value: 'developer' });
  await config.set_agent_profile({ name: 'codex' });

  for (const path of paths) {
    const after = statSync(path);
    assert.equal(after.mode & 0o777, 0o640, `${path} keeps its mode`);
    assert.equal(after.gid, group, `${path} keeps its group`);
  }
  assert.equal(parseToml(configPath(home)).entrance, 'developer');
  assert.equal(parseToml(agentPath(home)).future_key, 'keep');
});
