#!/usr/bin/env node
/**
 * P0-T4 / D17 — guarded `GET /v1/daemon/runtime/discovery`.
 *
 * The route is the attach identity anchor (`rust-core-service-boundary.md`
 * §8.1): the closed v1 discovery record of the running instance under the same
 * API-key/loopback admission as the operator stop path. These cases drive the
 * real server with a boundary service stub (no native payload): the record
 * shape, the identity source (this process, never the replaceable published
 * file), the guards and the absence of secret fields.
 */
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import http from 'node:http';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test, { after } from 'node:test';
import { fileURLToPath } from 'node:url';

const serviceRoot = join(dirname(fileURLToPath(import.meta.url)), '..');
const tempDirs = [];

after(() => {
  for (const dir of tempDirs) rmSync(dir, { recursive: true, force: true });
});

function tempHome() {
  const home = mkdtempSync(join(tmpdir(), 'nexus-discovery-route-'));
  tempDirs.push(home);
  return home;
}

function stubService(overrides = {}) {
  return {
    instanceId: 'inst-route-test',
    workspaceInitialized: false,
    creatorId: null,
    workspaceSlug: null,
    engineEpoch: null,
    tlsFingerprint: null,
    startedAt: new Date().toISOString(),
    domainOnly: false,
    providerReady: false,
    ...overrides,
  };
}

async function startServer(home, overrides = {}) {
  const { resolveServiceConfig } = await import(join(serviceRoot, 'dist/config.js'));
  const { createServiceServer, listenServer } = await import(join(serviceRoot, 'dist/server.js'));
  const config = resolveServiceConfig({ home, host: '127.0.0.1', port: 0, allowRemote: false });
  const service = stubService(overrides);
  const created = createServiceServer(config, service, async () => ({
    state: 'closed',
    cleanup_confirmed: true,
    pending_operations: [],
  }));
  const endpoint = await listenServer(created.server, config);
  return {
    service,
    config,
    endpoint,
    url: endpoint.url,
    close: () => new Promise((resolve) => {
      created.server.closeAllConnections();
      created.server.close(() => resolve());
    }),
  };
}

function request(url, { method = 'GET', headers = {} } = {}) {
  return new Promise((resolveRequest, rejectRequest) => {
    const req = http.request(url, { method, headers }, (res) => {
      const chunks = [];
      res.on('data', (chunk) => chunks.push(chunk));
      res.on('end', () => {
        const text = Buffer.concat(chunks).toString('utf8');
        resolveRequest({
          status: res.statusCode,
          headers: res.headers,
          text,
          payload: text.length > 0 ? JSON.parse(text) : null,
        });
      });
    });
    req.on('error', rejectRequest);
    req.end();
  });
}

test('the guarded response is the published record of this instance (uninitialized shell)', async () => {
  const home = tempHome();
  const server = await startServer(home);
  const { createDiscoveryRecord, publishDiscovery, discoveryRecordPath } = await import(
    join(serviceRoot, 'dist/discovery.js')
  );
  const published = createDiscoveryRecord({
    instanceId: server.service.instanceId,
    userHome: home,
    endpoint: server.endpoint,
    tlsFingerprint: null,
    readiness: 'uninitialized',
    creatorId: null,
    workspaceSlug: null,
    engineEpoch: null,
  });
  await publishDiscovery(published);

  const response = await request(`${server.url}/v1/daemon/runtime/discovery`);
  assert.equal(response.status, 200, response.text);
  assert.deepEqual(response.payload, published);
  assert.deepEqual(
    response.payload,
    JSON.parse(readFileSync(discoveryRecordPath(home), 'utf8')),
    'the identity anchor matches the private record exactly',
  );
  assert.equal(response.payload.readiness, 'uninitialized');
  assert.equal(response.payload.user_home, home);
  assert.deepEqual(response.payload.endpoint, server.endpoint);

  await server.close();
});

test('a ready instance reports creator, workspace and engine epoch', async () => {
  const home = tempHome();
  const server = await startServer(home, {
    workspaceInitialized: true,
    creatorId: 'ctr_local_route',
    workspaceSlug: 'default',
    engineEpoch: 3,
  });
  const { createDiscoveryRecord, publishDiscovery } = await import(join(serviceRoot, 'dist/discovery.js'));
  const published = createDiscoveryRecord({
    instanceId: server.service.instanceId,
    userHome: home,
    endpoint: server.endpoint,
    tlsFingerprint: null,
    readiness: 'ready',
    creatorId: 'ctr_local_route',
    workspaceSlug: 'default',
    engineEpoch: 3,
  });
  await publishDiscovery(published);

  const response = await request(`${server.url}/v1/daemon/runtime/discovery`);
  assert.equal(response.status, 200, response.text);
  assert.deepEqual(response.payload, published);
  assert.equal(response.payload.readiness, 'ready');

  await server.close();
});

test('a stale published record cannot make the response name another instance', async () => {
  const home = tempHome();
  const server = await startServer(home, { workspaceInitialized: true, creatorId: 'ctr_a', workspaceSlug: 'default', engineEpoch: 1 });
  const { createDiscoveryRecord, publishDiscovery } = await import(join(serviceRoot, 'dist/discovery.js'));
  // A replacement (or a crash survivor) owns the file: the response must still
  // describe the process answering the request.
  const stale = createDiscoveryRecord({
    instanceId: 'inst-replaced',
    userHome: home,
    endpoint: server.endpoint,
    tlsFingerprint: null,
    readiness: 'ready',
    creatorId: 'ctr_a',
    workspaceSlug: 'default',
    engineEpoch: 1,
  });
  await publishDiscovery(stale);

  const response = await request(`${server.url}/v1/daemon/runtime/discovery`);
  assert.equal(response.status, 200, response.text);
  assert.equal(response.payload.instance_id, server.service.instanceId);
  assert.notEqual(response.payload.instance_id, stale.instance_id);
  assert.equal(response.payload.pid, process.pid);

  await server.close();
});

test('the discovery route requires the same admission as the guarded stop path', async () => {
  const home = tempHome();
  const previous = process.env.NEXUS42_DAEMON_API_KEY;
  process.env.NEXUS42_DAEMON_API_KEY = 'discovery-secret';
  try {
    const server = await startServer(home);
    const keyless = await request(`${server.url}/v1/daemon/runtime/discovery`);
    assert.equal(keyless.status, 401, keyless.text);
    assert.equal(keyless.payload.error.code, 'auth_required');

    const wrongKey = await request(`${server.url}/v1/daemon/runtime/discovery`, {
      headers: { 'X-API-Key': 'not-the-key' },
    });
    assert.equal(wrongKey.status, 401, wrongKey.text);

    const admitted = await request(`${server.url}/v1/daemon/runtime/discovery`, {
      headers: { 'X-API-Key': 'discovery-secret' },
    });
    assert.equal(admitted.status, 200, admitted.text);
    assert.equal(admitted.payload.instance_id, server.service.instanceId);

    // The unguarded liveness route stays reachable without a key.
    const health = await request(`${server.url}/v1/daemon/runtime/health`);
    assert.equal(health.status, 200, health.text);

    await server.close();
  } finally {
    if (previous === undefined) delete process.env.NEXUS42_DAEMON_API_KEY;
    else process.env.NEXUS42_DAEMON_API_KEY = previous;
  }
});

test('the response carries no secret or extra fields', async () => {
  const home = tempHome();
  const server = await startServer(home);
  const response = await request(`${server.url}/v1/daemon/runtime/discovery`);
  assert.equal(response.status, 200, response.text);
  assert.deepEqual(Object.keys(response.payload).sort(), [
    'creator_id',
    'endpoint',
    'engine_epoch',
    'instance_id',
    'pid',
    'protocol_version',
    'readiness',
    'schema_version',
    'tls_fingerprint',
    'user_home',
    'workspace_slug',
  ]);
  assert.equal(response.payload.schema_version, 1);
  assert.equal(response.payload.protocol_version, 1);
  assert.equal(/api_key|apikey|token|secret|bearer/i.test(response.text), false);

  await server.close();
});
