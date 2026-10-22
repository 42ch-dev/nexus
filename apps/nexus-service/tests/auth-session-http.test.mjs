#!/usr/bin/env node
/**
 * Task 1 — browser access bootstrap (access spec §3.1 / §6.2).
 *
 * Covers the product behaviour of the tokenized `GET /__bootstrap` route:
 * one-time handoff to an `HttpOnly` session cookie, keyed-mode admission by
 * cookie with `X-API-Key` precedence, the verbatim replay/expiry 401 bodies,
 * CORS credentials for allowlisted origins, and the unchanged keyless
 * loopback path.
 */
import assert from 'node:assert/strict';
import http from 'node:http';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test, { after } from 'node:test';

const serviceRoot = join(dirname(fileURLToPath(import.meta.url)), '..');
const API_KEY = 'test-api-key';

/** §6.2 copy, verbatim. */
const REPLAY_MESSAGE = '访问链接已被使用。请回到服务终端复制最新打印的访问链接。';
const EXPIRED_MESSAGE = '访问链接已过期。请回到服务终端复制最新打印的访问链接。';

const tempDirs = [];

after(() => {
  for (const dir of tempDirs) rmSync(dir, { recursive: true, force: true });
});

function tempHome() {
  const home = mkdtempSync(join(tmpdir(), 'nexus-auth-session-'));
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
        let payload = null;
        try {
          payload = text.length > 0 ? JSON.parse(text) : null;
        } catch {
          payload = null;
        }
        resolveRequest({ status: res.statusCode, headers: res.headers, text, payload });
      });
    });
    req.on('error', rejectRequest);
    req.end();
  });
}

async function startServer({ keyed = false, clock } = {}) {
  const previousKey = process.env.NEXUS42_DAEMON_API_KEY;
  let server;
  try {
    if (keyed) process.env.NEXUS42_DAEMON_API_KEY = API_KEY;
    else delete process.env.NEXUS42_DAEMON_API_KEY;

    const { resolveServiceConfig } = await import(join(serviceRoot, 'dist/config.js'));
    const { createServiceServer, listenServer } = await import(join(serviceRoot, 'dist/server.js'));
    const { AuthSessionStore } = await import(join(serviceRoot, 'dist/auth-session.js'));

    const config = resolveServiceConfig({
      home: tempHome(),
      host: '127.0.0.1',
      port: 0,
      allowRemote: false,
    });
    // Guarded tier1 route the admission assertions call; the runtime family is
    // unguarded and would answer 200 with or without credentials.
    const service = {
      startedAt: new Date().toISOString(),
      tlsFingerprint: null,
      workspaceInitialized: false,
      core: {
        listCreators: async () => ({
          items: [],
          pagination: { limit: 50, has_more: false, next_cursor: null },
        }),
      },
    };
    const sessions = new AuthSessionStore(clock);
    const created = createServiceServer(config, service, async () => ({
      state: 'closed',
      cleanup_confirmed: true,
      pending_operations: [],
    }), sessions);
    const endpoint = await listenServer(created.server, config);
    server = created.server;
    return {
      url: endpoint.url,
      sessions,
      close: () =>
        new Promise((resolveClose) => {
          server.closeAllConnections();
          server.close(() => resolveClose());
        }),
    };
  } finally {
    if (previousKey === undefined) delete process.env.NEXUS42_DAEMON_API_KEY;
    else process.env.NEXUS42_DAEMON_API_KEY = previousKey;
  }
}

test('tokens are single-use, replay-detectable and expire after their TTL', async () => {
  const { AuthSessionStore, TOKEN_TTL_MS } = await import(join(serviceRoot, 'dist/auth-session.js'));
  let now = 1_000;
  const store = new AuthSessionStore(() => now);

  const token = store.mintToken();
  // 256 bits base64url: 43 characters, no padding.
  assert.match(token, /^[A-Za-z0-9_-]{43}$/);
  assert.equal(store.consumeToken(token), true);
  assert.equal(store.consumeToken(token), false);
  assert.equal(store.wasConsumed(token), true);

  const expired = store.mintToken();
  now += TOKEN_TTL_MS + 1;
  assert.equal(store.consumeToken(expired), false);
  assert.equal(store.wasConsumed(expired), false);
});

test('session cookies admit only issued, unexpired sessions', async () => {
  const { AuthSessionStore, SESSION_TTL_MS, SESSION_COOKIE } = await import(
    join(serviceRoot, 'dist/auth-session.js')
  );
  let now = 0;
  const store = new AuthSessionStore(() => now);
  const sessionId = store.issueSession();

  assert.equal(store.hasValidSession({ headers: { cookie: `${SESSION_COOKIE}=${sessionId}` } }), true);
  assert.equal(store.hasValidSession({ headers: { cookie: 'nexus_session=forged' } }), false);
  assert.equal(store.hasValidSession({ headers: {} }), false);

  now += SESSION_TTL_MS + 1;
  assert.equal(store.hasValidSession({ headers: { cookie: `${SESSION_COOKIE}=${sessionId}` } }), false);
});

test('bootstrap sets the session cookie and the cookie then admits keyed API calls', async () => {
  const server = await startServer({ keyed: true });
  const token = server.sessions.mintToken();

  const bootstrap = await request(`${server.url}/__bootstrap?token=${token}`);
  assert.equal(bootstrap.status, 302);
  assert.equal(bootstrap.headers.location, '/');
  const setCookie = bootstrap.headers['set-cookie']?.[0] ?? '';
  assert.match(
    setCookie,
    /^nexus_session=[A-Za-z0-9_-]{43}; HttpOnly; SameSite=Lax; Path=\/; Max-Age=28800$/,
  );

  const cookie = setCookie.split(';')[0];
  const admitted = await request(`${server.url}/v1/daemon/creators`, { headers: { Cookie: cookie } });
  assert.equal(admitted.status, 200, admitted.text);

  const anon = await request(`${server.url}/v1/daemon/creators`);
  assert.equal(anon.status, 401);
  assert.equal(anon.payload.error.code, 'auth_required');

  await server.close();
});

test('bootstrap replay and expiry answer the §6.2 bodies verbatim', async () => {
  let now = 0;
  const server = await startServer({ clock: () => now });

  const token = server.sessions.mintToken();
  assert.equal((await request(`${server.url}/__bootstrap?token=${token}`)).status, 302);

  const replay = await request(`${server.url}/__bootstrap?token=${token}`);
  assert.equal(replay.status, 401);
  assert.equal(replay.text, REPLAY_MESSAGE);

  const expiring = server.sessions.mintToken();
  now += 300_001;
  const expired = await request(`${server.url}/__bootstrap?token=${expiring}`);
  assert.equal(expired.status, 401);
  assert.equal(expired.text, EXPIRED_MESSAGE);

  const missing = await request(`${server.url}/__bootstrap`);
  assert.equal(missing.status, 401);
  assert.equal(missing.text, EXPIRED_MESSAGE);

  await server.close();
});

test('a present X-API-Key wins over a valid session cookie', async () => {
  const server = await startServer({ keyed: true });
  const token = server.sessions.mintToken();
  const bootstrap = await request(`${server.url}/__bootstrap?token=${token}`);
  const cookie = (bootstrap.headers['set-cookie']?.[0] ?? '').split(';')[0];

  const rejected = await request(`${server.url}/v1/daemon/creators`, {
    headers: { Cookie: cookie, 'X-API-Key': 'wrong-key' },
  });
  assert.equal(rejected.status, 401);

  const accepted = await request(`${server.url}/v1/daemon/creators`, {
    headers: { Cookie: cookie, 'X-API-Key': API_KEY },
  });
  assert.equal(accepted.status, 200, accepted.text);

  await server.close();
});

test('an allowlisted origin gets Access-Control-Allow-Credentials', async () => {
  const server = await startServer();
  const response = await request(`${server.url}/v1/daemon/runtime/health`, {
    headers: { Origin: 'nexus://app' },
  });
  assert.equal(response.status, 200, response.text);
  assert.equal(response.headers['access-control-allow-origin'], 'nexus://app');
  assert.equal(response.headers['access-control-allow-credentials'], 'true');
  await server.close();
});

test('keyless loopback mode is unchanged', async () => {
  const server = await startServer();
  const response = await request(`${server.url}/v1/daemon/creators`);
  assert.equal(response.status, 200, response.text);
  await server.close();
});
