import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, realpathSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import net from 'node:net';
import tls from 'node:tls';
import { describe, test, before, after } from 'node:test';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = join(__dirname, '..', '..', '..');
const serviceRoot = join(__dirname, '..');
const fixture = resolve(root, 'crates/nexus-agent-host/tests/fixtures/mock_acp_workflow.py');

const RAW_SSE_HARD_TIMEOUT_MS = 30_000;
const SERVICE_CLOSE_TIMEOUT_MS = 8_000;
const openSockets = new Set();

let homeCtx;
let service;
let baseUrl;

function trackSocket(socket) {
  openSockets.add(socket);
  const untrack = () => openSockets.delete(socket);
  socket.once('close', untrack);
  socket.once('error', untrack);
  return socket;
}

function destroyTrackedSockets() {
  for (const socket of openSockets) {
    try { socket.destroy(); } catch {}
  }
  openSockets.clear();
}

async function closeServiceBounded(svc) {
  if (!svc?.close) return;
  destroyTrackedSockets();
  try {
    const report = await Promise.race([
      svc.close(),
      new Promise((_, reject) => setTimeout(() => reject(new Error('service.close timeout')), SERVICE_CLOSE_TIMEOUT_MS)),
    ]);
    if (!report?.cleanup_confirmed) {
      await new Promise((r) => setTimeout(r, 1_500));
    }
  } catch {
    await new Promise((r) => setTimeout(r, 1_500));
  }
  await new Promise((r) => setTimeout(r, 300));
}

async function stopSharedService() {
  if (!service) return;
  await closeServiceBounded(service);
  service = undefined;
  baseUrl = undefined;
}

async function startSharedService(home, port = 0, apiKey, tlsOpts) {
  let lastError;
  for (let attempt = 0; attempt < 12; attempt += 1) {
    try {
      service = await startProviderService(home, port, apiKey, tlsOpts);
      baseUrl = service.url;
      return service;
    } catch (error) {
      lastError = error;
      await new Promise((r) => setTimeout(r, 400 * (attempt + 1)));
    }
  }
  throw lastError;
}

function readFixtureLog(logPath) {
  try {
    return readFileSync(logPath, 'utf8').trim().split('\n').filter(Boolean).map((line) => JSON.parse(line));
  } catch { return []; }
}

function operationResponseKeys(payload) { return Object.keys(payload).sort(); }

function resolvePython() {
  const which = execFileSync('which', ['python3'], { encoding: 'utf8' }).trim();
  return realpathSync(which);
}

function seedHome(extraEnv = {}, extraProviders = '') {
  const home = mkdtempSync(join(tmpdir(), 'nexus-security-stream-'));
  const seed = spawnSync('cargo', ['run', '-q', '-p', 'nexus-core-node', '--bin', 'native-wire-fixture-seed', '--', home], { stdio: 'inherit' });
  assert.equal(seed.status, 0, seed.stderr?.toString());
  const agentHostDir = join(home, '.nexus42', 'agent-host');
  mkdirSync(agentHostDir, { recursive: true });
  const log = join(home, 'fixture.log');
  const python = resolvePython();
  const envLines = Object.entries({ ACP_FIXTURE_LOG: log, ...extraEnv }).map(([k, v]) => `${k} = ${JSON.stringify(v)}`).join('\n');
  const config = `[[providers]]\nid = "mock-acp"\nprotocol = "acp"\ncommand = ${JSON.stringify(python)}\nargs = [${JSON.stringify(fixture)}]\nenabled = true\n${extraProviders}\n[providers.env]\n${envLines}\n`;
  writeFileSync(join(agentHostDir, 'config.toml'), config);
  return { home, log };
}

async function jsonFetch(url, { method = 'GET', headers = {}, body, rejectUnauthorized = true } = {}) {
  const prev = process.env.NODE_TLS_REJECT_UNAUTHORIZED;
  if (!rejectUnauthorized) process.env.NODE_TLS_REJECT_UNAUTHORIZED = '0';
  try {
    const response = await fetch(url, { method, headers, body: body === undefined ? undefined : JSON.stringify(body) });
    const text = await response.text();
    const payload = text.length > 0 ? JSON.parse(text) : null;
    return { status: response.status, headers: response.headers, payload, text };
  } finally {
    if (!rejectUnauthorized) {
      if (prev === undefined) delete process.env.NODE_TLS_REJECT_UNAUTHORIZED;
      else process.env.NODE_TLS_REJECT_UNAUTHORIZED = prev;
    }
  }
}

function parseSseChunks(raw) {
  const events = [];
  for (const block of raw.split(/\r?\n\r?\n/)) {
    if (!block.trim()) continue;
    let id = '';
    let event = 'message';
    let data = '';
    for (const line of block.split(/\r?\n/)) {
      if (line.startsWith('id:')) id = line.slice(3).trim();
      else if (line.startsWith('event:')) event = line.slice(6).trim();
      else if (line.startsWith('data:')) data += line.slice(5).trim();
    }
    if (data) events.push({ id, event, data: JSON.parse(data) });
  }
  return events;
}

function rawSseGet(baseUrl, path, { headers = {}, pauseAfterHeaders = false, pauseImmediately = false, onBody, stallMs = 0, tlsOptions, hardTimeoutMs = RAW_SSE_HARD_TIMEOUT_MS, endPattern = /OpFinished|OpFailed|event: gap/ } = {}) {
  return new Promise((resolve, reject) => {
    const url = new URL(path, baseUrl);
    const connect = tlsOptions ? () => tls.connect({ host: url.hostname, port: Number(url.port), ...tlsOptions }) : () => net.connect(Number(url.port), url.hostname);
    const socket = trackSocket(connect());
    let raw = '';
    let headerDone = false;
    let paused = false;
    let settled = false;
    const finish = (err) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      if (err) reject(err);
      else resolve({ raw, socket });
    };
    const timer = setTimeout(() => {
      try { socket.destroy(); } catch {}
      finish(new Error(`rawSseGet timed out after ${hardTimeoutMs}ms for ${path}`));
    }, hardTimeoutMs);
    socket.setEncoding('utf8');
    socket.on('error', (err) => finish(err));
    const req = [`GET ${url.pathname}${url.search} HTTP/1.1`, `Host: ${url.host}`, 'Accept: text/event-stream', ...Object.entries(headers).map(([k, v]) => `${k}: ${v}`), '', ''].join('\r\n');
    socket.write(req);
    if (pauseImmediately) {
      socket.pause();
      setTimeout(() => socket.resume(), stallMs > 0 ? stallMs : 1_500);
    }
    socket.on('data', (chunk) => {
      raw += chunk;
      if (!headerDone && raw.includes('\r\n\r\n')) headerDone = true;
      if (pauseAfterHeaders && headerDone && !paused) {
        paused = true;
        socket.pause();
        setTimeout(() => { socket.resume(); setTimeout(() => socket.destroy(), 50); }, stallMs > 0 ? stallMs : 3_000);
      }
      if (endPattern?.test(raw)) setTimeout(() => { try { socket.destroy(); } catch {} }, 50);
      if (onBody) onBody(raw, socket);
    });
    socket.on('end', () => finish());
    socket.on('close', () => finish());
  });
}

async function startProviderService(home, port, apiKey, tlsOpts) {
  if (apiKey) process.env.NEXUS42_DAEMON_API_KEY = apiKey;
  const { startService } = await import(join(serviceRoot, 'dist/index.js'));
  const opts = { home, host: '127.0.0.1', port, allowRemote: false, domainOnly: false };
  if (tlsOpts) { opts.tlsCert = tlsOpts.cert; opts.tlsKey = tlsOpts.key; }
  return startService(opts);
}

async function providerFlow(url, headers = {}) {
  const created = await jsonFetch(`${url}/v1/daemon/agent-host/sessions`, { method: 'POST', headers: { 'Content-Type': 'application/json', ...headers }, body: { provider_id: 'mock-acp' } });
  assert.equal(created.status, 200, created.text);
  const executed = await jsonFetch(`${url}/v1/daemon/agent-host/sessions/${created.payload.session_id}/operations`, { method: 'POST', headers: { 'Content-Type': 'application/json', ...headers }, body: { kind: 'prompt', content: 'hello' } });
  assert.equal(executed.status, 200, executed.text);
  return { sessionId: created.payload.session_id, operationId: executed.payload.operation_id };
}

describe('security-stream (P4-T2)', () => {
  before(async () => {
    homeCtx = seedHome();
    assert.equal(spawnSync('node', ['packages/nexus-native/scripts/build.mjs'], { cwd: root, stdio: 'inherit' }).status, 0);
    assert.equal(spawnSync('npx', ['tsc', '-p', 'tsconfig.json'], { cwd: serviceRoot, stdio: 'inherit' }).status, 0);
    await startSharedService(homeCtx.home, 0);
    const status = await jsonFetch(`${baseUrl}/v1/daemon/runtime/status`);
    assert.equal(status.payload.runtime_mode, 'provider_enabled');
  });

  after(async () => {
    destroyTrackedSockets();
    await closeServiceBounded(service);
    service = undefined;
    baseUrl = undefined;
  });

  test('configured key, wrong key, origin, SSE, and TLS are denied before fixture mutation', async () => {
    await stopSharedService();
    const deniedHome = seedHome();
    const previous = process.env.NEXUS42_DAEMON_API_KEY;
    process.env.NEXUS42_DAEMON_API_KEY = 'stream-secret';
    let local = await startProviderService(deniedHome.home, 0, 'stream-secret');
    const logAfterReady = readFixtureLog(deniedHome.log).length;
    try {
      const missing = await jsonFetch(`${local.url}/v1/daemon/agent-host/sessions`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: { provider_id: 'mock-acp' } });
      assert.equal(missing.status, 401);
      assert.equal(missing.payload.error.code, 'auth_required');
      const wrongKey = await jsonFetch(`${local.url}/v1/daemon/agent-host/sessions`, { method: 'POST', headers: { 'Content-Type': 'application/json', 'X-API-Key': 'wrong-key' }, body: { provider_id: 'mock-acp' } });
      assert.equal(wrongKey.status, 401);
      const wrongOrigin = await jsonFetch(`${local.url}/v1/daemon/agent-host/sessions`, { method: 'POST', headers: { 'Content-Type': 'application/json', 'X-API-Key': 'stream-secret', Origin: 'http://evil.example.com:9999' }, body: { provider_id: 'mock-acp' } });
      assert.equal(wrongOrigin.status, 403);
      const deniedOnlyLog = readFixtureLog(deniedHome.log);
      assert.equal(deniedOnlyLog.length, logAfterReady);
      const created = await jsonFetch(`${local.url}/v1/daemon/agent-host/sessions`, { method: 'POST', headers: { 'Content-Type': 'application/json', 'X-API-Key': 'stream-secret' }, body: { provider_id: 'mock-acp' } });
      assert.equal(created.status, 200);
      const sseDenied = await fetch(`${local.url}/v1/daemon/agent-host/sessions/${created.payload.session_id}/events?operation_id=00000000-0000-4000-8000-000000000099`);
      assert.equal(sseDenied.status, 401);
      await closeServiceBounded(local); local = undefined;
      const tlsHome = seedHome();
      const tlsDir = mkdtempSync(join(tmpdir(), 'nexus-tls-'));
      const cert = join(tlsDir, 'cert.pem');
      const key = join(tlsDir, 'key.pem');
      execFileSync('openssl', ['req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', key, '-out', cert, '-days', '1', '-subj', '/CN=localhost']);
      const tlsService = await startProviderService(tlsHome.home, 0, 'stream-secret', { cert, key });
      try {
        const tlsDenied = await jsonFetch(`${tlsService.url}/v1/daemon/agent-host/sessions`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: { provider_id: 'mock-acp' }, rejectUnauthorized: false });
        assert.equal(tlsDenied.status, 401);
      } finally { await closeServiceBounded(tlsService); }
      assert.ok(readFixtureLog(deniedHome.log).some((e) => e.event === 'session_new'));
    } finally {
      if (local) await closeServiceBounded(local);
      if (previous === undefined) delete process.env.NEXUS42_DAEMON_API_KEY; else process.env.NEXUS42_DAEMON_API_KEY = previous;
      await startSharedService(homeCtx.home, 0);
    }
  });

  test('prompt streams to terminal over TCP SSE without duplicate terminal', async () => {
    const { sessionId, operationId } = await providerFlow(baseUrl);
    const { raw } = await rawSseGet(baseUrl, `/v1/daemon/agent-host/sessions/${sessionId}/events?operation_id=${operationId}`);
    const events = parseSseChunks(raw.split(/\r?\n\r?\n/).slice(1).join('\r\n\r\n'));
    assert.equal(events.filter((e) => e.event === 'provider_event' && e.data?.OpFinished).length, 1);
    const inspect = await jsonFetch(`${baseUrl}/v1/daemon/agent-host/operations/${operationId}`);
    assert.equal(inspect.payload.status, 'finished');
    assert.deepEqual(operationResponseKeys(inspect.payload), ['operation_id', 'session_id', 'status']);
  });

  test('pausing the TCP reader stops further pulls until drain resumes', async () => {
    await stopSharedService();
    const envHome = seedHome({ OVERSIZED_UPDATE: '1' });
    const prevHwm = process.env.NEXUS_SSE_SOCKET_HWM;
    const prevPullDelay = process.env.NEXUS_SSE_FIRST_PULL_DELAY_MS;
    process.env.NEXUS_SSE_SOCKET_HWM = '128';
    process.env.NEXUS_SSE_FIRST_PULL_DELAY_MS = '1800';
    const local = await startProviderService(envHome.home, 0);
    const { sseTestHooks } = await import(join(serviceRoot, 'dist/sse.js'));
    sseTestHooks.providerPullCount = 0; sseTestHooks.writeBlockedCount = 0; sseTestHooks.pullsWhileBlocked = 0;
    try {
      const { sessionId, operationId } = await providerFlow(local.url);
      const pullsBefore = sseTestHooks.providerPullCount;
      const socketPromise = rawSseGet(local.url, `/v1/daemon/agent-host/sessions/${sessionId}/events?operation_id=${operationId}`, { pauseImmediately: true, stallMs: 1500 });
      await new Promise((r) => setTimeout(r, 400));
      const pullsDuringStall = sseTestHooks.providerPullCount;
      await new Promise((r) => setTimeout(r, 700));
      const pullsStillStalled = sseTestHooks.providerPullCount;
      await socketPromise;
      const pullsAfter = sseTestHooks.providerPullCount;
      assert.ok(
        sseTestHooks.writeBlockedCount >= 1
          || sseTestHooks.pullsWhileBlocked > 0
          || pullsStillStalled === pullsBefore,
        'slow reader must observe backpressure via write(false) or pull gating',
      );
      assert.equal(pullsDuringStall, pullsBefore);
      assert.equal(pullsStillStalled, pullsBefore);
      assert.ok(pullsAfter > pullsBefore);
      const reconnect = await rawSseGet(local.url, `/v1/daemon/agent-host/sessions/${sessionId}/events?operation_id=${operationId}`);
      assert.ok(/OpFinished|OpFailed/.test(reconnect.raw.replace(/\r?\n/g, '')));
      const inspect = await jsonFetch(`${local.url}/v1/daemon/agent-host/operations/${operationId}`);
      assert.ok(['finished', 'failed'].includes(inspect.payload.status));
    } finally {
      if (prevHwm === undefined) delete process.env.NEXUS_SSE_SOCKET_HWM; else process.env.NEXUS_SSE_SOCKET_HWM = prevHwm;
      if (prevPullDelay === undefined) delete process.env.NEXUS_SSE_FIRST_PULL_DELAY_MS; else process.env.NEXUS_SSE_FIRST_PULL_DELAY_MS = prevPullDelay;
      await closeServiceBounded(local);
      await startSharedService(homeCtx.home, 0);
    }
  });

  test('invalid, future, and equal SSE cursors behave exactly', async () => {
    const { sessionId, operationId } = await providerFlow(baseUrl);
    assert.equal((await jsonFetch(`${baseUrl}/v1/daemon/agent-host/sessions/${sessionId}/events?operation_id=${operationId}&cursor=not-a-cursor`)).status, 400);
    const { raw } = await rawSseGet(baseUrl, `/v1/daemon/agent-host/sessions/${sessionId}/events?operation_id=${operationId}`);
    const events = parseSseChunks(raw.split(/\r?\n\r?\n/).slice(1).join('\r\n\r\n'));
    const [epoch] = events[0].id.split(':');
    const future = await jsonFetch(`${baseUrl}/v1/daemon/agent-host/sessions/${sessionId}/events?operation_id=${operationId}&cursor=${encodeURIComponent(`${epoch}:999999`)}`);
    assert.equal(future.status, 400);
    assert.equal(future.payload.error.details?.variant, 'future_cursor');
    const wrongEpoch = await jsonFetch(`${baseUrl}/v1/daemon/agent-host/sessions/${sessionId}/events?operation_id=${operationId}&cursor=00000000-0000-4000-8000-000000000000:1`);
    assert.equal(wrongEpoch.status, 400);
    assert.equal(wrongEpoch.payload.error.details?.variant, 'history_unavailable');
    const lastId = events.at(-1)?.id;
    const equal = await fetch(`${baseUrl}/v1/daemon/agent-host/sessions/${sessionId}/events?operation_id=${operationId}&cursor=${encodeURIComponent(lastId)}`, { headers: { Accept: 'text/event-stream' } });
    assert.equal(equal.status, 200);
    assert.match(equal.headers.get('content-type') ?? '', /text\/event-stream/);
    await equal.body?.cancel();
  });

  test('subscriber cap returns 409', async () => {
    const { sessionId, operationId } = await providerFlow(baseUrl);
    const path = `/v1/daemon/agent-host/sessions/${sessionId}/events?operation_id=${operationId}`;
    const url = new URL(path, baseUrl);
    const request = `GET ${url.pathname}${url.search} HTTP/1.1\r\nHost: ${url.host}\r\nAccept: text/event-stream\r\n\r\n`;
    const sockets = [];
    try {
      await Promise.all(Array.from({ length: 16 }, async () => {
        const socket = trackSocket(net.connect(Number(url.port), url.hostname));
        sockets.push(socket);
        socket.setEncoding('utf8');
        socket.on('data', () => socket.pause());
        socket.write(request);
        await new Promise((r) => setTimeout(r, 25));
      }));
      await new Promise((r) => setTimeout(r, 500));
      assert.equal((await fetch(`${baseUrl}${path}`)).status, 409);
    } finally { for (const s of sockets) { try { s.destroy(); } catch {} } }
  });

  test('disconnect yields inspectable terminal without duplicated transcript', async () => {
    const { sessionId, operationId } = await providerFlow(baseUrl);
    const eventsUrl = new URL(`/v1/daemon/agent-host/sessions/${sessionId}/events?operation_id=${operationId}`, baseUrl);
    const socket = trackSocket(net.connect(Number(eventsUrl.port), eventsUrl.hostname));
    socket.write(`GET ${eventsUrl.pathname}${eventsUrl.search} HTTP/1.1\r\nHost: ${eventsUrl.host}\r\nAccept: text/event-stream\r\n\r\n`);
    await new Promise((r) => setTimeout(r, 100));
    socket.destroy();
    const reconnect = await rawSseGet(baseUrl, `/v1/daemon/agent-host/sessions/${sessionId}/events?operation_id=${operationId}`);
    const events = parseSseChunks(reconnect.raw.split(/\r?\n\r?\n/).slice(1).join('\r\n\r\n'));
    const final = await jsonFetch(`${baseUrl}/v1/daemon/agent-host/operations/${operationId}`);
    assert.equal(final.payload.status, 'finished');
    assert.deepEqual(operationResponseKeys(final.payload), ['operation_id', 'session_id', 'status']);
    assert.equal(events.filter((e) => e.event === 'provider_event' && e.data?.OpFinished).length, 1);
  });

  test('cross-session operation SSE is forbidden before headers', async () => {
    const a = await providerFlow(baseUrl);
    const b = await jsonFetch(`${baseUrl}/v1/daemon/agent-host/sessions`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: { provider_id: 'mock-acp' } });
    assert.equal((await jsonFetch(`${baseUrl}/v1/daemon/agent-host/sessions/${b.payload.session_id}/events?operation_id=${a.operationId}`)).status, 403);
  });

  test('DSH cancellation is explicitly denied', async () => {
    await stopSharedService();
    const py = resolvePython();
    const dshHome = seedHome({}, `\n[[providers]]\nid = "dsh-native"\nprotocol = "acp"\ncommand = ${JSON.stringify(py)}\nargs = [${JSON.stringify(fixture)}]\nenabled = true\n`);
    const local = await startProviderService(dshHome.home, 0);
    try {
      const created = await jsonFetch(`${local.url}/v1/daemon/agent-host/sessions`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: { provider_id: 'dsh-native' } });
      assert.equal(created.status, 200);
      const executed = await jsonFetch(`${local.url}/v1/daemon/agent-host/sessions/${created.payload.session_id}/operations`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: { kind: 'prompt', content: 'hello' } });
      assert.equal(executed.status, 200);
      const cancel = await jsonFetch(`${local.url}/v1/daemon/agent-host/operations/${executed.payload.operation_id}`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: {} });
      assert.equal(cancel.status, 501);
      assert.equal(cancel.payload.error.code, 'route_not_migrated');
    } finally { await closeServiceBounded(local); await startSharedService(homeCtx.home, 0); }
  });

  test('cancel response does not mark session ready before terminal truth', async () => {
    await stopSharedService();
    const blockHome = seedHome({ BLOCK_PROMPT: '1' });
    const local = await startProviderService(blockHome.home, 0);
    try {
      const created = await jsonFetch(`${local.url}/v1/daemon/agent-host/sessions`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: { provider_id: 'mock-acp' } });
      const executed = await jsonFetch(`${local.url}/v1/daemon/agent-host/sessions/${created.payload.session_id}/operations`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: { kind: 'prompt', content: 'cancel-me' } });
      const cancel = await jsonFetch(`${local.url}/v1/daemon/agent-host/operations/${executed.payload.operation_id}`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: {} });
      assert.equal(cancel.payload.status, 'cancelled');
      assert.equal((await jsonFetch(`${local.url}/v1/daemon/agent-host/sessions/${created.payload.session_id}`)).payload.state, 'Running');
      assert.equal((await jsonFetch(`${local.url}/v1/daemon/agent-host/operations/${executed.payload.operation_id}`)).payload.status, 'started');
    } finally { await closeServiceBounded(local); await startSharedService(homeCtx.home, 0); }
  });

  test('list/get/shutdown responses match generated schema keys', async () => {
    const created = await jsonFetch(`${baseUrl}/v1/daemon/agent-host/sessions`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: { provider_id: 'mock-acp' } });
    const sessionId = created.payload.session_id;
    for (const key of ['session_id', 'provider_id', 'state']) assert.ok(key in created.payload);
    const listed = await jsonFetch(`${baseUrl}/v1/daemon/agent-host/sessions`);
    assert.ok(Array.isArray(listed.payload.items) && listed.payload.pagination);
    const got = await jsonFetch(`${baseUrl}/v1/daemon/agent-host/sessions/${sessionId}`);
    for (const key of ['session_id', 'provider_id', 'state']) assert.ok(key in got.payload);
    const shutdown = await jsonFetch(`${baseUrl}/v1/daemon/agent-host/sessions/${sessionId}`, { method: 'DELETE' });
    assert.deepEqual(Object.keys(shutdown.payload).sort(), ['session_id', 'status']);
  });

  test('bounded hub retention and control slot limits', async () => {
    const { OperationEventHub } = await import(join(serviceRoot, 'dist/sse.js'));
    const { HUB_MAX_DATA_FRAMES, HUB_MAX_DATA_BYTES, SSE_RESERVED_CONTROL_BYTES } = await import(join(serviceRoot, 'dist/config.js'));
    const hub = new OperationEventHub('00000000-0000-4000-8000-000000000010', '00000000-0000-4000-8000-000000000011');
    for (let i = 0; i < HUB_MAX_DATA_FRAMES + 5; i += 1) hub.recordEvent({ Progress: { message: `line-${i}` } });
    assert.ok(hub.evictionWatermark() > 1);
    const stale = hub.planReplay(`${hub.epoch}:1`);
    assert.equal(stale.kind, 'stale');
    assert.equal(stale.gap.reason, 'history_unavailable');
    assert.equal(hub.recordGap({ reason: 'interrupted', operation_id: hub.operationId, resync_required: true, inspect_url: '/x', padding: 'x'.repeat(SSE_RESERVED_CONTROL_BYTES) }), null);
    const terminal = hub.recordEvent({ OpFinished: { transcript: 'ok' } });
    assert.equal(hub.recordEvent({ OpFinished: { transcript: 'dup' } })?.id, terminal.id);
    assert.ok(hub.retainedMemoryBytes() <= HUB_MAX_DATA_BYTES + SSE_RESERVED_CONTROL_BYTES * 2);
  });

  test('actor/viewpoint create is explicit not_migrated', async () => {
    const res = await jsonFetch(`${baseUrl}/v1/daemon/agent-host/sessions`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: { provider_id: 'mock-acp', actor_ref: { actor_kind: 'creator', creator_id: 'ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa' }, viewpoint: { world_id: 'wld_owned' } } });
    assert.equal(res.status, 501);
    assert.equal(res.payload.error.code, 'route_not_migrated');
  });
});
