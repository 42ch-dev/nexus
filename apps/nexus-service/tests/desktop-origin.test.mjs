#!/usr/bin/env node
/**
 * P0-T4 — desktop origin admission.
 *
 * The standalone service replaces the obsolete Tauri-only origins with exactly
 * the frozen desktop app origin `nexus://app`, keeps loopback/Vite/browser
 * origins and the explicit env escape hatch, and relaxes nothing: a foreign
 * origin, a near-miss loopback origin and a literal `null` origin are denied
 * before any handler runs.
 */
import assert from 'node:assert/strict';
import { mkdtempSync, rmSync } from 'node:fs';
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
  const home = mkdtempSync(join(tmpdir(), 'nexus-origin-'));
  tempDirs.push(home);
  return home;
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

async function startServer() {
  const { resolveServiceConfig } = await import(join(serviceRoot, 'dist/config.js'));
  const { createServiceServer, listenServer } = await import(join(serviceRoot, 'dist/server.js'));
  const config = resolveServiceConfig({
    home: tempHome(),
    host: '127.0.0.1',
    port: 0,
    allowRemote: false,
  });
  const service = { startedAt: new Date().toISOString(), tlsFingerprint: null, workspaceInitialized: false };
  const created = createServiceServer(config, service, async () => ({
    state: 'closed',
    cleanup_confirmed: true,
    pending_operations: [],
  }));
  const endpoint = await listenServer(created.server, config);
  return {
    config,
    url: endpoint.url,
    close: () => new Promise((resolve) => {
      created.server.closeAllConnections();
      created.server.close(() => resolve());
    }),
  };
}

test('the allowlist is exactly the desktop origin plus loopback/Vite origins', async () => {
  const { resolveAllowedOrigins, DESKTOP_APP_ORIGIN } = await import(join(serviceRoot, 'dist/config.js'));
  assert.equal(DESKTOP_APP_ORIGIN, 'nexus://app');
  const origins = resolveAllowedOrigins(8420, '127.0.0.1');
  assert.deepEqual(
    [...origins].sort((a, b) => (a > b ? 1 : a < b ? -1 : 0)),
    [
      'http://127.0.0.1:5173',
      'http://127.0.0.1:8420',
      'http://localhost:5173',
      'http://localhost:8420',
      'nexus://app',
    ],
  );
  assert.equal(origins.includes('tauri://localhost'), false, 'the obsolete Tauri origin is gone');
  assert.equal(origins.includes('http://tauri.localhost'), false, 'the obsolete Tauri origin is gone');
  assert.equal(origins.includes('*'), false);
  assert.equal(origins.includes('null'), false);

  // The explicit env escape hatch stays available for non-listed browser origins.
  const previous = process.env.NEXUS_DAEMON_ALLOWED_ORIGINS;
  process.env.NEXUS_DAEMON_ALLOWED_ORIGINS = 'https://studio.example, not-a-url';
  try {
    const extended = resolveAllowedOrigins(8420, '127.0.0.1');
    assert.equal(extended.includes('https://studio.example'), true);
    assert.equal(extended.includes('not-a-url'), false);
  } finally {
    if (previous === undefined) delete process.env.NEXUS_DAEMON_ALLOWED_ORIGINS;
    else process.env.NEXUS_DAEMON_ALLOWED_ORIGINS = previous;
  }
});

test('exactly the desktop app origin is admitted, and the response is CORS-scoped to it', async () => {
  const server = await startServer();
  const allowed = await request(`${server.url}/v1/daemon/runtime/health`, {
    headers: { Origin: 'nexus://app' },
  });
  assert.equal(allowed.status, 200, allowed.text);
  assert.equal(allowed.headers['access-control-allow-origin'], 'nexus://app');
  assert.equal(allowed.headers.vary, 'Origin');
  await server.close();
});

test('foreign, near-miss and null origins are denied before the handler', async () => {
  const server = await startServer();
  const denied = [
    'tauri://localhost',
    'http://tauri.localhost',
    'https://evil.example',
    'http://localhost:9999',
    'nexus://app:8080',
    'null',
  ];
  for (const origin of denied) {
    const response = await request(`${server.url}/v1/daemon/runtime/health`, { headers: { Origin: origin } });
    assert.equal(response.status, 403, `${origin}: ${response.text}`);
    assert.equal(response.payload.error.code, 'forbidden');
    assert.deepEqual(response.payload.error.details, { resource: 'origin' });
    assert.equal(response.headers['access-control-allow-origin'], undefined);
  }
  await server.close();
});

test('a non-browser client without an Origin header keeps its retained access', async () => {
  const server = await startServer();
  const response = await request(`${server.url}/v1/daemon/runtime/health`);
  assert.equal(response.status, 200, response.text);
  assert.equal(response.headers['access-control-allow-origin'], undefined);
  await server.close();
});
