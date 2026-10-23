import assert from 'node:assert/strict';
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
import { spawnSync } from 'node:child_process';
import { after, before, describe, test } from 'node:test';
import { parse } from 'smol-toml';

const __dirname = dirname(fileURLToPath(import.meta.url));
const serviceRoot = join(__dirname, '..');

/**
 * I-000064 bounded integration target: the service-backed agent profile
 * (`GET/PUT /v1/daemon/agent-host/profile`) over the real in-process service and
 * the real daemon home. Persistence is observed through a full service close +
 * reopen on the same home; the desktop IPC path is not involved.
 */

function seedHome() {
  const home = mkdtempSync(join(tmpdir(), 'nexus-service-agent-profile-'));
  const seed = spawnSync(
    'cargo',
    ['run', '-q', '-p', 'nexus-core-node', '--bin', 'native-wire-fixture-seed', '--', home],
    { stdio: 'inherit' },
  );
  assert.equal(seed.status, 0, seed.stderr?.toString());
  return home;
}

async function jsonFetch(url, { method = 'GET', body } = {}) {
  const response = await fetch(url, {
    method,
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  const text = await response.text();
  const payload = text.length > 0 ? JSON.parse(text) : null;
  return { status: response.status, payload };
}

async function startProfileService(home) {
  const { startService } = await import(join(serviceRoot, 'dist/index.js'));
  // Port 0: the OS picks a free port, so this file never collides with the
  // other service suites running in parallel.
  return startService({ home, host: '127.0.0.1', port: 0, allowRemote: false, domainOnly: true });
}

/** The daemon config store: `<user home>/.nexus42/config.toml`. */
const configPathFor = (home) => join(home, '.nexus42', 'config.toml');

const PROFILE_URL_PATH = '/v1/daemon/agent-host/profile';

describe('agent-profile (I-000064)', () => {
  let home;
  let service;
  let baseUrl;

  before(async () => {
    home = seedHome();
    const build = spawnSync('npx', ['tsc', '-p', 'tsconfig.json'], {
      cwd: serviceRoot,
      stdio: 'inherit',
    });
    assert.equal(build.status, 0);
    service = await startProfileService(home);
    baseUrl = service.url;
  });

  after(async () => {
    if (service) await service.close();
  });

  test('GET answers null before any profile is saved', async () => {
    const res = await jsonFetch(`${baseUrl}${PROFILE_URL_PATH}`);
    assert.equal(res.status, 200);
    assert.deepEqual(res.payload, { profile: null });
  });

  test('PUT persists the profile and GET reads it back', async () => {
    const saved = await jsonFetch(`${baseUrl}${PROFILE_URL_PATH}`, {
      method: 'PUT',
      body: { name: 'claude-code', launch_command: '/usr/local/bin/claude' },
    });
    assert.equal(saved.status, 200);
    assert.deepEqual(saved.payload, {
      profile: { name: 'claude-code', launch_command: '/usr/local/bin/claude' },
    });

    const read = await jsonFetch(`${baseUrl}${PROFILE_URL_PATH}`);
    assert.equal(read.status, 200);
    assert.deepEqual(read.payload, saved.payload);
  });

  test('PUT rejects a blank name and a missing name with invalid_input', async () => {
    const blank = await jsonFetch(`${baseUrl}${PROFILE_URL_PATH}`, {
      method: 'PUT',
      body: { name: '   ', launch_command: '/bin/x' },
    });
    assert.equal(blank.status, 400);
    assert.equal(blank.payload.error.code, 'invalid_input');

    const missing = await jsonFetch(`${baseUrl}${PROFILE_URL_PATH}`, {
      method: 'PUT',
      body: { launch_command: '/bin/x' },
    });
    assert.equal(missing.status, 400);
    assert.equal(missing.payload.error.code, 'invalid_input');

    const wrongType = await jsonFetch(`${baseUrl}${PROFILE_URL_PATH}`, {
      method: 'PUT',
      body: { name: 'claude-code', launch_command: 42 },
    });
    assert.equal(wrongType.status, 400);
    assert.equal(wrongType.payload.error.code, 'invalid_input');

    // The rejected writes never replaced the stored profile.
    const read = await jsonFetch(`${baseUrl}${PROFILE_URL_PATH}`);
    assert.deepEqual(read.payload.profile, {
      name: 'claude-code',
      launch_command: '/usr/local/bin/claude',
    });
  });

  test('rejects a profile payload that is not a JSON object', async () => {
    const res = await jsonFetch(`${baseUrl}${PROFILE_URL_PATH}`, {
      method: 'PUT',
      body: ['not-an-object'],
    });
    assert.equal(res.status, 400);
    assert.equal(res.payload.error.code, 'invalid_input');
  });

  test('a service restart on the same home resolves the saved profile', async () => {
    await service.close();
    service = await startProfileService(home);
    baseUrl = service.url;

    const read = await jsonFetch(`${baseUrl}${PROFILE_URL_PATH}`);
    assert.equal(read.status, 200);
    assert.deepEqual(read.payload, {
      profile: { name: 'claude-code', launch_command: '/usr/local/bin/claude' },
    });
  });

  test('other daemon-home config keys survive the profile write', async () => {
    // Establish an unrelated key first (simulates creator/workspace config the
    // core already owns), then verify the profile write preserves it.
    const configPath = configPathFor(home);
    const raw = readFileSync(configPath, 'utf8');
    const before = parse(raw);
    const foreignKey = 'agent_profile_test_marker';
    writeFileSync(configPath, `${foreignKey} = "kept"\n${raw}`, 'utf8');

    const saved = await jsonFetch(`${baseUrl}${PROFILE_URL_PATH}`, {
      method: 'PUT',
      // Explicit blank clears the inherited command (omitting it would preserve).
      body: { name: 'codex-cli', launch_command: '' },
    });
    assert.equal(saved.status, 200);

    const afterDoc = parse(readFileSync(configPath, 'utf8'));
    // The seeded daemon keys are the core's own store contents and must survive
    // a service-side profile write (same file, whole-document rewrite).
    assert.equal(afterDoc.active_creator_id, before.active_creator_id);
    assert.deepEqual(
      afterDoc.active_workspace_slug_by_creator,
      before.active_workspace_slug_by_creator,
    );
    assert.equal(afterDoc[foreignKey], 'kept');
    // The service owns no file in the raw user home.
    assert.equal(existsSync(join(home, 'config.toml')), false);
    for (const [key, value] of Object.entries(before)) {
      if (key === 'agent_profile') continue;
      assert.deepEqual(afterDoc[key], value, `config key ${key} was preserved`);
    }
    assert.deepEqual({ ...afterDoc.agent_profile }, { name: 'codex-cli' });
  });

  test('a name-only PUT preserves the stored launch command; an explicit blank clears it', async () => {
    const seeded = await jsonFetch(`${baseUrl}${PROFILE_URL_PATH}`, {
      method: 'PUT',
      body: { name: 'codex-cli', launch_command: 'codex --custom' },
    });
    assert.deepEqual(seeded.payload, {
      profile: { name: 'codex-cli', launch_command: 'codex --custom' },
    });

    const nameOnly = await jsonFetch(`${baseUrl}${PROFILE_URL_PATH}`, {
      method: 'PUT',
      body: { name: 'codex-cli' },
    });
    assert.equal(nameOnly.status, 200);
    assert.deepEqual(nameOnly.payload, {
      profile: { name: 'codex-cli', launch_command: 'codex --custom' },
    });

    const cleared = await jsonFetch(`${baseUrl}${PROFILE_URL_PATH}`, {
      method: 'PUT',
      body: { name: 'codex-cli', launch_command: '   ' },
    });
    assert.deepEqual(cleared.payload, { profile: { name: 'codex-cli' } });
  });

  test('switching to a different agent with an omitted command stores no command', async () => {
    const seeded = await jsonFetch(`${baseUrl}${PROFILE_URL_PATH}`, {
      method: 'PUT',
      body: { name: 'claude-code', launch_command: '/custom/claude --flag' },
    });
    assert.deepEqual(seeded.payload, {
      profile: { name: 'claude-code', launch_command: '/custom/claude --flag' },
    });

    // Different name + omitted command: the previous agent's override must not
    // leak into the new profile.
    const switched = await jsonFetch(`${baseUrl}${PROFILE_URL_PATH}`, {
      method: 'PUT',
      body: { name: 'codex-cli' },
    });
    assert.equal(switched.status, 200);
    assert.deepEqual(switched.payload, { profile: { name: 'codex-cli' } });

    const read = await jsonFetch(`${baseUrl}${PROFILE_URL_PATH}`);
    assert.deepEqual(read.payload, { profile: { name: 'codex-cli' } });
  });

  test('oversized name and launch_command are invalid_input and leave the profile intact', async () => {
    const before = await jsonFetch(`${baseUrl}${PROFILE_URL_PATH}`);

    const longName = await jsonFetch(`${baseUrl}${PROFILE_URL_PATH}`, {
      method: 'PUT',
      body: { name: 'a'.repeat(201) },
    });
    assert.equal(longName.status, 400);
    assert.equal(longName.payload.error.code, 'invalid_input');

    const longCommand = await jsonFetch(`${baseUrl}${PROFILE_URL_PATH}`, {
      method: 'PUT',
      body: { name: 'codex-cli', launch_command: 'x'.repeat(4097) },
    });
    assert.equal(longCommand.status, 400);
    assert.equal(longCommand.payload.error.code, 'invalid_input');

    const after = await jsonFetch(`${baseUrl}${PROFILE_URL_PATH}`);
    assert.deepEqual(after.payload, before.payload);
  });

  test('a failed write leaves no temp file behind', async () => {
    const { writeAgentProfile } = await import(join(serviceRoot, 'dist/agent-profile.js'));
    const brokenHome = mkdtempSync(join(tmpdir(), 'nexus-agent-profile-broken-'));
    const originalError = console.error;
    try {
      chmodSync(brokenHome, 0o500);
      console.error = () => {};
      await assert.rejects(() => writeAgentProfile(brokenHome, { name: 'claude-code' }));
      assert.deepEqual(
        readdirSync(brokenHome).filter((name) => name.endsWith('.tmp')),
        [],
      );
    } finally {
      console.error = originalError;
      chmodSync(brokenHome, 0o700);
      rmSync(brokenHome, { recursive: true, force: true });
    }
  });
});
