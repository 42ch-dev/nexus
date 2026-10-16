import assert from 'node:assert/strict';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { after, before, describe, test } from 'node:test';

/**
 * v1.215 P1 T2 (residual R-V1208-P2-12): a create-work request that omits the
 * `world_id` key lands on the curated `world_id_required` refusal, identical to
 * the empty-string shape — one remediation-text owner, never a duplicated one.
 *
 * The curated discriminator rides `error.details.field`: the wire vocabulary
 * has no `world_id_required` code, so the core guard's
 * `WorkFault::BadRequest { code: "world_id_required" }` maps to
 * `CoreError::InvalidInput { field, reason }` and the native boundary + service
 * error mapper publish it as `invalid_input` + `details.field`. The generic
 * decode-failure 400 carries no `details` at all.
 *
 * Self-contained per the `apps/nexus-service/tests/` convention: it seeds and
 * starts its own bounded service so the assertion does not couple to a sibling
 * suite's shared Work state.
 */

const __dirname = dirname(fileURLToPath(import.meta.url));
const serviceRoot = join(__dirname, '..');

function seedHome() {
  const home = mkdtempSync(join(tmpdir(), 'nexus-service-world-id-'));
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
  return { status: response.status, payload, text };
}

async function startDomainService(home, port) {
  const { startService } = await import(join(serviceRoot, 'dist/index.js'));
  return startService({
    home,
    host: '127.0.0.1',
    port,
    allowRemote: false,
    domainOnly: true,
  });
}

const WORK_BODY = {
  title: 'World Binding Work',
  long_term_goal: 'Prove the curated world_id refusal',
  initial_idea: 'Seeded by the v1.215 P1 T2 test',
  world_id: 'wld_owned',
};

describe('create-work world_id curated refusal (v1.215 P1 T2)', () => {
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
    service = await startDomainService(home, 18_490);
    baseUrl = service.url;
  });

  after(async () => {
    if (service) await service.close();
  });

  test('an absent world_id key lands on the curated world_id_required 400', async () => {
    const { world_id: _omitted, ...withoutWorld } = WORK_BODY;
    const missing = await jsonFetch(`${baseUrl}/v1/daemon/works`, {
      method: 'POST',
      body: withoutWorld,
    });
    assert.equal(missing.status, 400, missing.text);
    assert.equal(missing.payload.error.details?.field, 'world_id_required', missing.text);
    assert.match(missing.payload.error.message, /World binding is required for new Works/);

    // The empty-string shape is rejected by the same core guard with the exact
    // same remediation text — one text owner, not two.
    const empty = await jsonFetch(`${baseUrl}/v1/daemon/works`, {
      method: 'POST',
      body: { ...WORK_BODY, world_id: '' },
    });
    assert.equal(empty.status, 400, empty.text);
    assert.equal(empty.payload.error.details?.field, 'world_id_required', empty.text);
    assert.equal(
      empty.payload.error.message,
      missing.payload.error.message,
      'both shapes must share one remediation text',
    );
  });

  test('a different missing field and a wrong-typed world_id stay generic', async () => {
    const { title: _omitted, ...withoutTitle } = WORK_BODY;
    const noTitle = await jsonFetch(`${baseUrl}/v1/daemon/works`, {
      method: 'POST',
      body: withoutTitle,
    });
    assert.equal(noTitle.status, 400, noTitle.text);
    assert.equal(noTitle.payload.error.code, 'invalid_input', noTitle.text);
    assert.equal(noTitle.payload.error.details?.field, undefined, 'title has no curated code');

    const nullWorld = await jsonFetch(`${baseUrl}/v1/daemon/works`, {
      method: 'POST',
      body: { ...WORK_BODY, world_id: null },
    });
    assert.equal(nullWorld.status, 400, nullWorld.text);
    assert.equal(nullWorld.payload.error.code, 'invalid_input', nullWorld.text);
    assert.equal(
      nullWorld.payload.error.details?.field,
      undefined,
      'a wrong-typed world_id is a type error, not a missing binding',
    );
  });
});
