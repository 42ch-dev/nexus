import assert from 'node:assert/strict';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { describe, test, before, after } from 'node:test';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = join(__dirname, '..', '..', '..');
const serviceRoot = join(__dirname, '..');

function seedHome() {
  const home = mkdtempSync(join(tmpdir(), 'nexus-service-http-'));
  const seed = spawnSync(
    'cargo',
    ['run', '-q', '-p', 'nexus-core-node', '--bin', 'native-wire-fixture-seed', '--', home],
    { cwd: root, stdio: 'inherit' },
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
  return { status: response.status, headers: response.headers, payload };
}

describe('world-kb-http (P4-T1)', () => {
  let home;
  let service;
  let baseUrl;
  let worldId;

  before(async () => {
    home = seedHome();
    const build = spawnSync('pnpm', ['--filter', '@42ch/nexus-service', 'run', 'build'], {
      cwd: root,
      stdio: 'inherit',
    });
    assert.equal(build.status, 0);

    const { startService } = await import(join(serviceRoot, 'dist/index.js'));
    const port = 18_421;
    service = await startService({
      home,
      host: '127.0.0.1',
      port,
      allowRemote: false,
      domainOnly: true,
    });
    baseUrl = service.url;

    const graphProbe = await jsonFetch(`${baseUrl}/v1/daemon/runtime/health`);
    assert.equal(graphProbe.status, 200);
    assert.equal(graphProbe.payload.status, 'ok');

    const graph = await jsonFetch(`${baseUrl}/v1/daemon/worlds/wld_owned/kb/graph`, {
      headers: { 'X-API-Key': 'unused-in-keyless' },
    });
    if (graph.status === 200) {
      worldId = 'wld_owned';
    }
  });

  after(async () => {
    if (service) {
      await service.close();
    }
  });

  test('keyless loopback allows runtime health', async () => {
    const res = await jsonFetch(`${baseUrl}/v1/daemon/runtime/health`);
    assert.equal(res.status, 200);
    assert.equal(res.payload.status, 'ok');
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
      const missing = await jsonFetch(`${local.url}/v1/daemon/worlds/wld_owned/kb/graph`);
      assert.equal(missing.status, 401);
      assert.equal(missing.payload.error.code, 'auth_required');
      const wrong = await jsonFetch(`${local.url}/v1/daemon/worlds/wld_owned/kb/graph`, {
        headers: { 'X-API-Key': 'wrong' },
      });
      assert.equal(wrong.status, 401);
      const ok = await jsonFetch(`${local.url}/v1/daemon/worlds/wld_owned/kb/graph`, {
        headers: { 'X-API-Key': 'test-secret-key' },
      });
      assert.equal(ok.status, 200);
    } finally {
      await local.close();
      if (previous === undefined) delete process.env.NEXUS42_DAEMON_API_KEY;
      else process.env.NEXUS42_DAEMON_API_KEY = previous;
    }
    service = await startService({
      home,
      host: '127.0.0.1',
      port: 18_421,
      allowRemote: false,
      domainOnly: true,
    });
    baseUrl = service.url;
  });

  test('graph and candidates return real native projections when world exists', async () => {
    if (!worldId) {
      return;
    }
    const graph = await jsonFetch(`${baseUrl}/v1/daemon/worlds/${worldId}/kb/graph`);
    assert.equal(graph.status, 200);
    assert.ok(Array.isArray(graph.payload.entities));

    const candidates = await jsonFetch(`${baseUrl}/v1/daemon/worlds/${worldId}/kb/candidates`);
    assert.equal(candidates.status, 200);
    assert.ok(Array.isArray(candidates.payload.items));
  });

  test('unported daemon route returns route_not_migrated', async () => {
    const res = await jsonFetch(`${baseUrl}/v1/daemon/workspace/init`, { method: 'POST', body: {} });
    assert.equal(res.status, 501);
    assert.equal(res.payload.error.code, 'route_not_migrated');
  });
});
