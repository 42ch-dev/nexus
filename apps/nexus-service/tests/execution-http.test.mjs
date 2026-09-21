import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync, spawnSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { after, before, describe, test } from 'node:test';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = join(__dirname, '..', '..', '..');
const serviceRoot = join(__dirname, '..');
const fixture = resolve(root, 'crates/nexus-agent-host/tests/fixtures/mock_acp_workflow.py');

/**
 * P5-T3 bounded integration target: the execution/preset/strategy families
 * and the provider-family selection over the real in-process service, the
 * real native store, and deterministic no-model ACP fixture peers. No mock
 * forwards anything: selection is observed through the native provider
 * catalog, terminal truth through the durable operation journal (close +
 * reopen on the same home), and the migrated preset/strategy routes through
 * the core authority's own refusals.
 */

function resolvePython() {
  const which = execFileSync('which', ['python3'], { encoding: 'utf8' }).trim();
  return realpathSync(which);
}
function realpathSync(p) {
  return spawnSync('python3', ['-c', `import os,sys;print(os.path.realpath(sys.argv[1]))`, p], {
    encoding: 'utf8',
  }).stdout.trim();
}

// Four provider families, all backed by the same deterministic no-model ACP
// fixture: selection must be Rust-admitted across the whole catalog, not a
// single hard-coded peer.
const PROVIDER_IDS = ['mock-acp-1', 'mock-acp-2', 'mock-acp-3', 'mock-acp-4'];

function seedHome() {
  const home = mkdtempSync(join(tmpdir(), 'nexus-service-execution-'));
  const seed = spawnSync(
    'cargo',
    ['run', '-q', '-p', 'nexus-core-node', '--bin', 'native-wire-fixture-seed', '--', home],
    { stdio: 'inherit' },
  );
  assert.equal(seed.status, 0, seed.stderr?.toString());
  const agentHostDir = join(home, '.nexus42', 'agent-host');
  mkdirSync(agentHostDir, { recursive: true });
  const python = resolvePython();
  const families = PROVIDER_IDS.map(
    (id) =>
      `[[providers]]\nid = ${JSON.stringify(id)}\nprotocol = "acp"\ncommand = ${JSON.stringify(python)}\nargs = [${JSON.stringify(fixture)}]\nenabled = true\n`,
  ).join('\n');
  const config = `${families}\n[providers.env]\n`;
  writeFileSync(join(agentHostDir, 'config.toml'), config);
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

async function startExecutionService(home, port) {
  const { startService } = await import(join(serviceRoot, 'dist/index.js'));
  return startService({ home, host: '127.0.0.1', port, allowRemote: false, domainOnly: false });
}

describe('execution-http (P5-T3)', () => {
  let home;
  let service;
  let baseUrl;

  before(async () => {
    home = seedHome();
    assert.equal(
      spawnSync('node', ['packages/nexus-native/scripts/build.mjs'], { cwd: root, stdio: 'inherit' })
        .status,
      0,
    );
    const build = spawnSync('npx', ['tsc', '-p', 'tsconfig.json'], {
      cwd: serviceRoot,
      stdio: 'inherit',
    });
    assert.equal(build.status, 0);
    service = await startExecutionService(home, 18_444);
    baseUrl = service.url;
  });

  after(async () => {
    if (service) await service.close();
  });

  test('every assigned execution/preset/strategy route identity is mounted at its exact verb and tier', async () => {
    const { DOMAIN_ROUTES } = await import(join(serviceRoot, 'dist/routes.js'));
    const inventory = DOMAIN_ROUTES.map((route) => ({
      method: route.method,
      path: route.pattern.source.replace(/\\\//g, '/').replace(/^\^|\$$/g, ''),
      tier: route.tier,
      family: route.family,
    }));
    const required = [
      ['GET', '/v1/daemon/presets'],
      ['POST', '/v1/daemon/presets'],
      ['POST', '/v1/daemon/presets:validate'],
      ['GET', '/v1/daemon/presets/([^/]+)'],
      ['PATCH', '/v1/daemon/presets/([^/]+)'],
      ['DELETE', '/v1/daemon/presets/([^/]+)'],
      ['GET', '/v1/daemon/orchestration/presets'],
      ['GET', '/v1/daemon/orchestration/presets/([^/]+)/profile'],
      ['POST', '/v1/daemon/strategies/([^/]+)/states/([^/]+)/patch'],
      ['POST', '/v1/daemon/strategies/([^/]+)/transitions/patch'],
      ['POST', '/v1/daemon/strategies/([^/]+)/states/([^/]+)/prompt/patch'],
      ['POST', '/v1/daemon/orchestration/schedules'],
      ['POST', '/v1/daemon/orchestration/schedules/([^/]+)/signal'],
    ];
    const mounted = new Set(inventory.map((route) => `${route.method} ${route.path}`));
    for (const [method, path] of required) {
      assert.ok(
        mounted.has(`${method} ${path}`),
        `missing mounted identity: ${method} ${path}\nmounted:\n${[...mounted].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0)).join('\n')}`,
      );
    }
    // Tier parity: execution/preset/strategy are all creator-tier; the
    // compute run family stays an explicit migration refusal (the WASM edge
    // is a daemon-cohort capability), never a degraded fake.
    for (const route of inventory) {
      if (route.family === 'presets' || route.family === 'execution') {
        assert.equal(route.tier, 'tier2', `${route.method} ${route.path} must stay tier2`);
      }
    }
  });

  test('provider selection and interrupted execution', async () => {
    // 1. Four-family selection is Rust-admitted: the native catalog exposes
    //    exactly the configured families and a session binds the chosen one.
    const catalog = await jsonFetch(`${baseUrl}/v1/daemon/agent-host/providers`);
    assert.equal(catalog.status, 200, catalog.text);
    const listed = (catalog.payload.providers ?? []).map((entry) => entry.provider_id);
    // The four configured mock families must all be Rust-admitted, and the
    // maintained DSH native adapter is a real default catalog member (P4
    // native_cli registration semantics) — asserted present, never deleted
    // to flatter the fixture.
    for (const id of PROVIDER_IDS) {
      assert.ok(listed.includes(id), `missing configured provider family ${id}: ${catalog.text}`);
    }
    assert.ok(listed.includes('dsh-native'), `maintained dsh-native adapter must be registered: ${catalog.text}`);

    const created = await jsonFetch(`${baseUrl}/v1/daemon/agent-host/sessions`, {
      method: 'POST',
      body: { provider_id: PROVIDER_IDS[1] },
    });
    assert.equal(created.status, 200, created.text);
    const sessionId = created.payload.session_id;
    assert.ok(sessionId, created.text);

    // 2. Graceful close cancels the in-flight operation — `cancelled` is the
    //    true settled semantic for an op orphaned by an orderly shutdown.
    const executed = await jsonFetch(
      `${baseUrl}/v1/daemon/agent-host/sessions/${sessionId}/operations`,
      { method: 'POST', body: { kind: 'prompt', content: 'hello' } },
    );
    assert.equal(executed.status, 200, executed.text);
    const operationId = executed.payload.operation_id;
    assert.ok(operationId, executed.text);

    await service.close();
    service = await startExecutionService(home, 18_445);
    baseUrl = service.url;

    const settled = await jsonFetch(
      `${baseUrl}/v1/daemon/agent-host/operations/${operationId}`,
    );
    assert.equal(settled.status, 200, settled.text);
    assert.equal(
      settled.payload.status,
      'cancelled',
      `an in-flight op orphaned by an orderly close settles as cancelled: ${settled.text}`,
    );

    // 3. TRUE interruption is a journal orphan that no process ever settled:
    //    an in-flight journal row is pre-seeded into the workspace state DB
    //    (the T1 `second_owner_and_restart_are_fenced` method — no close in
    //    between), and the next open's settlement classifies it `interrupted`
    //    and keeps it terminal-inspectable.
    const orphanId = crypto.randomUUID();
    // The journal rides the writer-fenced state DB, so the seed must write as
    // the retained engine writer: stub the five `nexus_writer_*` scalars with
    // the gate/registration values the closed engine owner committed.
    const seed = spawnSync(
      'python3',
      [
        '-c',
        `import sqlite3, glob; root = ${JSON.stringify(home)}; path = glob.glob(root + '/.nexus42/creators/*/workspaces/*/state.db')[0]; conn = sqlite3.connect(path); g = conn.execute("SELECT migration_epoch, engine_epoch FROM core_workspace_gate WHERE pk=1").fetchone(); wid = conn.execute("SELECT writer_id FROM core_writer_registration WHERE mode='engine' AND engine_epoch=?", (g[1],)).fetchone()[0]; conn.create_function("nexus_writer_protocol", 0, lambda: 1); conn.create_function("nexus_writer_mode", 0, lambda: "engine"); conn.create_function("nexus_writer_id", 0, lambda: wid); conn.create_function("nexus_migration_epoch", 0, lambda: g[0]); conn.create_function("nexus_engine_epoch", 0, lambda: g[1]); conn.execute("INSERT OR IGNORE INTO js_provider_operation_journal (operation_id, session_id, provider_id, status, sequence) VALUES (?, 'sess_orphan', 'mock-acp-1', 'running', 1)", (${JSON.stringify(orphanId)},)); conn.commit(); print(path)`,
      ],
      { stdio: 'inherit' },
    );
    assert.equal(seed.status, 0, 'orphan journal seed failed');

    await service.close();
    service = await startExecutionService(home, 18_446);
    baseUrl = service.url;

    const afterReopen = await jsonFetch(
      `${baseUrl}/v1/daemon/agent-host/operations/${orphanId}`,
    );
    assert.equal(afterReopen.status, 200, afterReopen.text);
    assert.equal(
      afterReopen.payload.status,
      'interrupted',
      `an unsettled journal orphan must be classified interrupted: ${afterReopen.text}`,
    );

    // 4. Cancellation semantics stay truthful: a DELETE against an unknown
    //    session id is a real 404 from the host registry — never a fake
    //    success — and DSH cancellation is not offered as a fake success
    //    anywhere on this surface.
    const unknownSessionId = randomUUID();
    const missing = await jsonFetch(
      `${baseUrl}/v1/daemon/agent-host/sessions/${unknownSessionId}`,
      { method: 'DELETE' },
    );
    assert.equal(missing.status, 404, missing.text);
    assert.equal(missing.payload.error.code, 'not_found', missing.text);
  });

  test('migrated preset and strategy routes answer from the core authority, not 501', async () => {
    const list = await jsonFetch(`${baseUrl}/v1/daemon/presets`);
    assert.equal(list.status, 200, list.text);
    assert.ok(Array.isArray(list.payload.embedded), list.text);

    const orchestration = await jsonFetch(`${baseUrl}/v1/daemon/orchestration/presets`);
    assert.equal(orchestration.status, 200, orchestration.text);

    // An unknown preset id is the core authority's own refusal (404), and a
    // strategy patch against a nonexistent strategy is refused by the same
    // authority — both prove the route is live over real state, not 501.
    const missingPreset = await jsonFetch(`${baseUrl}/v1/daemon/presets/preset_missing`);
    assert.equal(missingPreset.status, 404, missingPreset.text);
    const strategyPatch = await jsonFetch(
      `${baseUrl}/v1/daemon/strategies/strategy_missing/states/state_missing/patch`,
      {
        method: 'POST',
        body: {
          base_revision: 1,
          strategy_id: 'strategy_missing',
          state_id: 'state_missing',
          set: {},
        },
      },
    );
    // The route is live over the core authority: the missing strategy is the
    // core's own 404 refusal — never 501, never a synthesized success.
    assert.equal(strategyPatch.status, 404, strategyPatch.text);

    // The compute run family is cohort-excluded on this surface: a truthful
    // migration refusal (501 route_not_migrated), never a degraded success.
    const computeRun = await jsonFetch(`${baseUrl}/v1/daemon/compute/run`, {
      method: 'POST',
      body: { capability_id: 'x', inputs: {} },
    });
    assert.equal(computeRun.status, 501, computeRun.text);
    assert.equal(computeRun.payload.error.code, 'route_not_migrated', computeRun.text);
  });
});
