import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { describe, test, before, after } from 'node:test';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = join(__dirname, '..', '..', '..');
const serviceRoot = join(__dirname, '..');
const OWNED_WORLD = 'wld_owned';
const FOREIGN_WORLD = 'wld_foreign';

function seedHome() {
  const home = mkdtempSync(join(tmpdir(), 'nexus-service-http-'));
  const seed = spawnSync(
    'cargo',
    ['run', '-q', '-p', 'nexus-core-node', '--bin', 'native-wire-fixture-seed', '--', home],
    { stdio: 'inherit', stdio: 'inherit' },
  );
  assert.equal(seed.status, 0, seed.stderr?.toString());
  return home;
}

async function jsonFetch(url, { method = 'GET', headers = {}, body } = {}) {
  const response = await fetch(url, {
    method,
    headers,
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await response.text();
  const payload = text.length > 0 ? JSON.parse(text) : null;
  return { status: response.status, headers: response.headers, payload, text };
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

describe('world-kb-http (P4-T1)', () => {
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
    service = await startDomainService(home, 18_421);
    baseUrl = service.url;

    const graph = await jsonFetch(`${baseUrl}/v1/daemon/worlds/${OWNED_WORLD}/kb/graph`);
    assert.equal(graph.status, 200, graph.text);
    assert.ok(Array.isArray(graph.payload.entities));

    const candidates = await jsonFetch(`${baseUrl}/v1/daemon/worlds/${OWNED_WORLD}/kb/candidates`);
    assert.equal(candidates.status, 200, candidates.text);
    assert.equal(candidates.payload.items.length, 2, 'fixture must expose two pending candidates');
  });

  after(async () => {
    if (service) {
      await service.close();
    }
  });

  test('keyless loopback allows runtime health with request id', async () => {
    const res = await jsonFetch(`${baseUrl}/v1/daemon/runtime/health`, {
      headers: { 'X-Request-Id': 'req-health-1' },
    });
    assert.equal(res.status, 200);
    assert.equal(res.payload.status, 'ok');
    assert.equal(res.headers.get('x-request-id'), 'req-health-1');
  });

  test('runtime status reflects initialized workspace', async () => {
    const res = await jsonFetch(`${baseUrl}/v1/daemon/runtime/status`);
    assert.equal(res.status, 200);
    assert.equal(res.payload.workspace_initialized, true);
    assert.equal(res.payload.runtime_mode, 'domain_only');
  });

  test('configured key rejects missing and wrong before native mutation', async () => {
    await service.close();
    service = undefined;
    const previous = process.env.NEXUS42_DAEMON_API_KEY;
    process.env.NEXUS42_DAEMON_API_KEY = 'test-secret-key';
    const { startService } = await import(join(serviceRoot, 'dist/index.js'));
    const local = await startService({
      home,
      host: '127.0.0.1',
      port: 18_422,
      allowRemote: false,
      domainOnly: true,
    });
    try {
      const missing = await jsonFetch(`${local.url}/v1/daemon/worlds/${OWNED_WORLD}/kb/graph`);
      assert.equal(missing.status, 401);
      assert.equal(missing.payload.error.code, 'auth_required');
      assert.ok(missing.payload.error.request_id);

      const wrong = await jsonFetch(`${local.url}/v1/daemon/worlds/${OWNED_WORLD}/kb/graph`, {
        headers: { 'X-API-Key': 'wrong' },
      });
      assert.equal(wrong.status, 401);

      const ok = await jsonFetch(`${local.url}/v1/daemon/worlds/${OWNED_WORLD}/kb/graph`, {
        headers: { 'X-API-Key': 'test-secret-key' },
      });
      assert.equal(ok.status, 200);
    } finally {
      await local.close();
      if (previous === undefined) delete process.env.NEXUS42_DAEMON_API_KEY;
      else process.env.NEXUS42_DAEMON_API_KEY = previous;
    }
    service = await startDomainService(home, 18_421);
    baseUrl = service.url;
  });

  test('origin outside allowlist is denied before mutation', async () => {
    const res = await jsonFetch(`${baseUrl}/v1/daemon/worlds/${OWNED_WORLD}/kb/graph`, {
      headers: { Origin: 'http://evil.example.com:9999' },
    });
    assert.equal(res.status, 403);
    assert.equal(res.payload.error.code, 'forbidden');
  });

  test('malformed candidate limit suffix is rejected', async () => {
    const res = await jsonFetch(`${baseUrl}/v1/daemon/worlds/${OWNED_WORLD}/kb/candidates?limit=2junk`);
    assert.equal(res.status, 400);
    assert.equal(res.payload.error.code, 'invalid_input');
  });

  test('candidate limit clamps high values instead of rejecting', async () => {
    const res = await jsonFetch(`${baseUrl}/v1/daemon/worlds/${OWNED_WORLD}/kb/candidates?limit=999`);
    assert.equal(res.status, 200);
    assert.ok(res.payload.items.length <= 250);
  });

  test('foreign and missing worlds preserve ordering', async () => {
    const foreign = await jsonFetch(`${baseUrl}/v1/daemon/worlds/${FOREIGN_WORLD}/kb/graph`);
    assert.equal(foreign.status, 403);
    assert.equal(foreign.payload.error.code, 'forbidden');

    const missing = await jsonFetch(`${baseUrl}/v1/daemon/worlds/wld_does_not_exist/kb/graph`);
    assert.equal(missing.status, 404);
    assert.equal(missing.payload.error.code, 'not_found');
  });

  test('patch create/update/stale CAS against real fixture DB', async () => {
    const create = await jsonFetch(`${baseUrl}/v1/daemon/worlds/${OWNED_WORLD}/kb/patch-entity`, {
      method: 'POST',
      body: {
        entity_id: 'kb_abc124',
        expected_version: 0,
        patch: { title: 'Created', block_type: 'character' },
      },
    });
    assert.equal(create.status, 200, create.text);
    assert.equal(create.payload.version, 1);

    const update = await jsonFetch(`${baseUrl}/v1/daemon/worlds/${OWNED_WORLD}/kb/patch-entity`, {
      method: 'POST',
      body: {
        entity_id: 'kb_mod',
        expected_version: 0,
        patch: { title: 'Updated Mod' },
      },
    });
    assert.equal(update.status, 200, update.text);
    assert.equal(update.payload.version, 1);

    const stale = await jsonFetch(`${baseUrl}/v1/daemon/worlds/${OWNED_WORLD}/kb/patch-entity`, {
      method: 'POST',
      body: {
        entity_id: 'kb_cas',
        expected_version: 1,
        patch: { title: 'Stale' },
      },
    });
    assert.equal(stale.status, 409, stale.text);
    assert.equal(stale.payload.error.code, 'world_kb_conflict');
  });

  test('oversized request body is rejected before native effects', async () => {
    const padding = 'x'.repeat(1024 * 1024);
    const body = `{"entity_id":"kb_mod","expected_version":0,"patch":{"title":"${padding}"}}`;
    const response = await fetch(`${baseUrl}/v1/daemon/worlds/${OWNED_WORLD}/kb/patch-entity`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body,
    });
    assert.equal(response.status, 413);
    const payload = await response.json();
    assert.equal(payload.error.code, 'input_too_large');
  });

  test('host session not found maps to 404', async () => {
    const res = await jsonFetch(`${baseUrl}/v1/daemon/agent-host/sessions/00000000-0000-0000-0000-000000000000`);
    assert.equal(res.status, 404);
    assert.equal(res.payload.error.code, 'not_found');
  });

  test('non-loopback bind is rejected before listen', async () => {
    const { startService } = await import(join(serviceRoot, 'dist/index.js'));
    await assert.rejects(
      () =>
        startService({
          home,
          host: '0.0.0.0',
          port: 18_423,
          allowRemote: false,
          domainOnly: true,
        }),
      (error) => {
        assert.match(String(error), /non-loopback|forbidden|allow-remote/i);
        return true;
      },
    );
  });

  test('unported daemon route returns route_not_migrated', async () => {
    const res = await jsonFetch(`${baseUrl}/v1/daemon/workspace/init`, { method: 'POST', body: {} });
    assert.equal(res.status, 501);
    assert.equal(res.payload.error.code, 'route_not_migrated');
  });

  test('close returns native cleanup report', async () => {
    const report = await service.close();
    service = undefined;
    assert.ok(['closed', 'interrupted'].includes(report.state));
    assert.equal(typeof report.cleanup_confirmed, 'boolean');
  });
});
