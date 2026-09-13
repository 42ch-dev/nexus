import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, writeFileSync, realpathSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import net from 'node:net';
import { spawnSync } from 'node:child_process';
import { describe, test, before, after } from 'node:test';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = join(__dirname, '..', '..', '..');
const serviceRoot = join(__dirname, '..');
const fixture = resolve(root, 'crates/nexus-agent-host/tests/fixtures/mock_acp_workflow.py');

function resolvePython() {
  const which = execFileSync('which', ['python3'], { encoding: 'utf8' }).trim();
  return realpathSync(which);
}

function seedHome(extraEnv = {}) {
  const home = mkdtempSync(join(tmpdir(), 'nexus-security-stream-'));
  const seed = spawnSync(
    'cargo',
    ['run', '-q', '-p', 'nexus-core-node', '--bin', 'native-wire-fixture-seed', '--', home],
    { stdio: 'inherit' },
  );
  assert.equal(seed.status, 0, seed.stderr?.toString());

  const nexusRoot = join(home, '.nexus42');
  const agentHostDir = join(nexusRoot, 'agent-host');
  mkdirSync(agentHostDir, { recursive: true });
  const log = join(home, 'fixture.log');
  const python = resolvePython();
  const envLines = Object.entries({ ACP_FIXTURE_LOG: log, ...extraEnv })
    .map(([k, v]) => `${k} = ${JSON.stringify(v)}`)
    .join('\n');
  const config = `[[providers]]\nid = "mock-acp"\nprotocol = "acp"\ncommand = ${JSON.stringify(python)}\nargs = [${JSON.stringify(fixture)}]\nenabled = true\n\n[providers.env]\n${envLines}\n`;
  writeFileSync(join(agentHostDir, 'config.toml'), config);
  return { home, log };
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

function parseHostPort(url) {
  const u = new URL(url);
  return { host: u.hostname, port: Number(u.port), pathPrefix: u.pathname === '/' ? '' : '' };
}

function parseSseChunks(raw) {
  const events = [];
  const blocks = raw.split('\n\n');
  for (const block of blocks) {
    if (!block.trim()) continue;
    let id = '';
    let event = 'message';
    let data = '';
    for (const line of block.split('\n')) {
      if (line.startsWith('id:')) id = line.slice(3).trim();
      else if (line.startsWith('event:')) event = line.slice(6).trim();
      else if (line.startsWith('data:')) data += line.slice(5).trim();
    }
    if (data) events.push({ id, event, data: JSON.parse(data) });
  }
  return events;
}

function rawSseGet(baseUrl, path, { headers = {}, pauseAfterHeaders = false, onBody, stallMs = 0 } = {}) {
  return new Promise((resolve, reject) => {
    const url = new URL(path, baseUrl);
    const socket = net.connect(Number(url.port), url.hostname);
    let raw = '';
    let headerDone = false;
    let paused = false;
    socket.setEncoding('utf8');
    socket.on('error', reject);
    const req = [
      `GET ${url.pathname}${url.search} HTTP/1.1`,
      `Host: ${url.host}`,
      'Accept: text/event-stream',
      ...Object.entries(headers).map(([k, v]) => `${k}: ${v}`),
      '',
      '',
    ].join('\r\n');
    socket.write(req);
    socket.on('data', (chunk) => {
      raw += chunk;
      if (!headerDone && raw.includes('\r\n\r\n')) headerDone = true;
      if (pauseAfterHeaders && headerDone && !paused) {
        paused = true;
        socket.pause();
        const wait = stallMs > 0 ? stallMs : 3_000;
        setTimeout(() => {
          socket.resume();
          setTimeout(() => socket.destroy(), 50);
        }, wait);
      }
      if (onBody) onBody(raw, socket);
    });
    socket.on('end', () => resolve({ raw, socket }));
    socket.on('close', () => resolve({ raw, socket }));
  });
}

async function startProviderService(home, port, apiKey) {
  if (apiKey) process.env.NEXUS42_DAEMON_API_KEY = apiKey;
  const { startService } = await import(join(serviceRoot, 'dist/index.js'));
  return startService({ home, host: '127.0.0.1', port, allowRemote: false, domainOnly: false });
}

async function providerFlow(baseUrl, headers = {}, env = {}) {
  const created = await jsonFetch(`${baseUrl}/v1/daemon/agent-host/sessions`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', ...headers },
    body: { provider_id: 'mock-acp' },
  });
  assert.equal(created.status, 200, created.text);
  const executed = await jsonFetch(`${baseUrl}/v1/daemon/agent-host/sessions/${created.payload.session_id}/operations`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', ...headers },
    body: { kind: 'prompt', content: 'hello' },
  });
  assert.equal(executed.status, 200, executed.text);
  return { sessionId: created.payload.session_id, operationId: executed.payload.operation_id };
}

describe('security-stream (P4-T2)', () => {
  let homeCtx;
  let service;
  let baseUrl;

  before(async () => {
    homeCtx = seedHome();
    const buildNative = spawnSync('node', ['packages/nexus-native/scripts/build.mjs'], {
      cwd: root,
      stdio: 'inherit',
    });
    assert.equal(buildNative.status, 0);
    const build = spawnSync('npx', ['tsc', '-p', 'tsconfig.json'], { cwd: serviceRoot, stdio: 'inherit' });
    assert.equal(build.status, 0);
    service = await startProviderService(homeCtx.home, 0);
    baseUrl = service.url;
    const status = await jsonFetch(`${baseUrl}/v1/daemon/runtime/status`);
    assert.equal(status.payload.runtime_mode, 'provider_enabled');
  });

  after(async () => {
    if (service) await service.close();
  });

  test('configured key and origin are denied before session mutation', async () => {
    await service.close();
    service = undefined;
    const previous = process.env.NEXUS42_DAEMON_API_KEY;
    process.env.NEXUS42_DAEMON_API_KEY = 'stream-secret';
    const local = await startProviderService(homeCtx.home, 0, 'stream-secret');
    try {
      const missing = await jsonFetch(`${local.url}/v1/daemon/agent-host/sessions`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: { provider_id: 'mock-acp' },
      });
      assert.equal(missing.status, 401);
      assert.equal(missing.payload.error.code, 'auth_required');

      const wrongOrigin = await jsonFetch(`${local.url}/v1/daemon/agent-host/sessions`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'X-API-Key': 'stream-secret',
          Origin: 'http://evil.example.com:9999',
        },
        body: { provider_id: 'mock-acp' },
      });
      assert.equal(wrongOrigin.status, 403);

      const ok = await jsonFetch(`${local.url}/v1/daemon/agent-host/sessions`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'X-API-Key': 'stream-secret' },
        body: { provider_id: 'mock-acp' },
      });
      assert.equal(ok.status, 200);
    } finally {
      await local.close();
      if (previous === undefined) delete process.env.NEXUS42_DAEMON_API_KEY;
      else process.env.NEXUS42_DAEMON_API_KEY = previous;
      service = await startProviderService(homeCtx.home, 0);
      baseUrl = service.url;
    }
  });

  test('prompt streams to terminal over TCP SSE without duplicate terminal', async () => {
    const { sessionId, operationId } = await providerFlow(baseUrl);
    const { raw } = await rawSseGet(
      baseUrl,
      `/v1/daemon/agent-host/sessions/${sessionId}/events?operation_id=${operationId}`,
    );
    const body = raw.split('\r\n\r\n').slice(1).join('\r\n\r\n');
    const events = parseSseChunks(body);
    const terminals = events.filter((e) => e.event === 'provider_event' && e.data?.OpFinished);
    assert.equal(terminals.length, 1, 'exactly one terminal frame');
    const inspect = await jsonFetch(`${baseUrl}/v1/daemon/agent-host/operations/${operationId}`);
    assert.equal(inspect.status, 200);
    assert.equal(inspect.payload.status, 'finished');
  });

  test('pausing the TCP reader stops further pulls until drain resumes', async () => {
    const envHome = seedHome({ OVERSIZED_UPDATE: '1' });
    await service.close();
    service = undefined;
    const local = await startProviderService(envHome.home, 0);
    try {
      const { sessionId, operationId } = await providerFlow(local.url);
      const socketPromise = rawSseGet(
        local.url,
        `/v1/daemon/agent-host/sessions/${sessionId}/events?operation_id=${operationId}`,
        {
          pauseAfterHeaders: true,
          stallMs: 2_500,
          onBody: (raw, socket) => {
            if (raw.includes('OpFailed') || raw.includes('gap') || raw.includes('interrupted')) {
              socket.destroy();
            }
          },
        },
      );
      const { raw } = await socketPromise;
      assert.match(raw, /OpFailed|gap|interrupted/);
      const inspect = await jsonFetch(`${local.url}/v1/daemon/agent-host/operations/${operationId}`);
      assert.ok(['failed', 'finished', 'started'].includes(inspect.payload.status));
    } finally {
      await local.close();
      service = await startProviderService(homeCtx.home, 0);
      baseUrl = service.url;
    }
  });

  test('invalid and future SSE cursors return 400', async () => {
    const { sessionId, operationId } = await providerFlow(baseUrl);
    const bad = await jsonFetch(
      `${baseUrl}/v1/daemon/agent-host/sessions/${sessionId}/events?operation_id=${operationId}&cursor=not-a-cursor`,
    );
    assert.equal(bad.status, 400);
    const future = await jsonFetch(
      `${baseUrl}/v1/daemon/agent-host/sessions/${sessionId}/events?operation_id=${operationId}&cursor=00000000-0000-4000-8000-000000000000:999999`,
    );
    assert.equal(future.status, 400);
    assert.equal(future.payload.error.details?.variant, 'future_cursor');
  });

  test('subscriber cap returns 409', async () => {
    const created = await jsonFetch(`${baseUrl}/v1/daemon/agent-host/sessions`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: { provider_id: 'mock-acp' },
    });
    assert.equal(created.status, 200, created.text);
    const executed = await jsonFetch(
      `${baseUrl}/v1/daemon/agent-host/sessions/${created.payload.session_id}/operations`,
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: { kind: 'prompt', content: 'hello' },
      },
    );
    assert.equal(executed.status, 200, executed.text);
    const sessionId = created.payload.session_id;
    const operationId = executed.payload.operation_id;
    const sockets = [];
    const path = `/v1/daemon/agent-host/sessions/${sessionId}/events?operation_id=${operationId}`;
    const url = new URL(path, baseUrl);
    const request = `GET ${url.pathname}${url.search} HTTP/1.1\r\nHost: ${url.host}\r\nAccept: text/event-stream\r\n\r\n`;
    await Promise.all(
      Array.from({ length: 16 }, async () => {
        const socket = net.connect(Number(url.port), url.hostname);
        sockets.push(socket);
        socket.setEncoding('utf8');
        socket.on('data', () => socket.pause());
        socket.write(request);
        await new Promise((r) => setTimeout(r, 25));
      }),
    );
    await new Promise((r) => setTimeout(r, 500));
    const blocked = await fetch(`${baseUrl}${path}`);
    assert.equal(blocked.status, 409, await blocked.text());
    for (const socket of sockets) socket.destroy();
  });

  test('disconnect yields inspectable terminal without duplicated transcript', async () => {
    const { sessionId, operationId } = await providerFlow(baseUrl);
    const eventsUrl = new URL(`/v1/daemon/agent-host/sessions/${sessionId}/events?operation_id=${operationId}`, baseUrl);
    const socket = net.connect(Number(eventsUrl.port), eventsUrl.hostname);
    socket.write(
      `GET ${eventsUrl.pathname}${eventsUrl.search} HTTP/1.1\r\nHost: ${eventsUrl.host}\r\nAccept: text/event-stream\r\n\r\n`,
    );
    await new Promise((r) => setTimeout(r, 100));
    socket.destroy();
    for (let i = 0; i < 40; i += 1) {
      const inspect = await jsonFetch(`${baseUrl}/v1/daemon/agent-host/operations/${operationId}`);
      if (inspect.payload.status === 'finished') break;
      await new Promise((r) => setTimeout(r, 50));
    }
    const final = await jsonFetch(`${baseUrl}/v1/daemon/agent-host/operations/${operationId}`);
    assert.equal(final.payload.status, 'finished');
    const second = await jsonFetch(`${baseUrl}/v1/daemon/agent-host/operations/${operationId}`);
    assert.deepEqual(second.payload.terminal, final.payload.terminal);
  });

  test('actor/viewpoint create is explicit not_migrated', async () => {
    const res = await jsonFetch(`${baseUrl}/v1/daemon/agent-host/sessions`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: {
        provider_id: 'mock-acp',
        actor_ref: { actor_kind: 'creator', creator_id: 'ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa' },
        viewpoint: { world_id: 'wld_owned' },
      },
    });
    assert.equal(res.status, 501);
    assert.equal(res.payload.error.code, 'route_not_migrated');
  });
});
