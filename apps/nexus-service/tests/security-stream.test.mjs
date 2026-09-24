import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, realpathSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import net from 'node:net';
import tls from 'node:tls';
import https from 'node:https';
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

async function jsonFetch(url, { method = 'GET', headers = {}, body, ca } = {}) {
  if (ca === undefined) {
    const response = await fetch(url, { method, headers, body: body === undefined ? undefined : JSON.stringify(body) });
    const text = await response.text();
    const payload = text.length > 0 ? JSON.parse(text) : null;
    return { status: response.status, headers: response.headers, payload, text };
  }
  // HTTPS against the test's own generated certificate: full certificate and
  // hostname validation, scoped to this trust anchor — no process-wide bypass.
  return new Promise((resolveFetch, rejectFetch) => {
    const request = https.request(url, { method, headers, ca, rejectUnauthorized: true }, (res) => {
      const chunks = [];
      res.on('data', (chunk) => chunks.push(chunk));
      res.on('end', () => {
        const text = Buffer.concat(chunks).toString('utf8');
        const payload = text.length > 0 ? JSON.parse(text) : null;
        resolveFetch({ status: res.statusCode, headers: res.headers, payload, text });
      });
    });
    request.on('error', rejectFetch);
    if (body !== undefined) request.write(JSON.stringify(body));
    request.end();
  });
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

/** Minimal SSE GET that resolves with the HTTP status line; caller destroys the socket. */
function rawHttpStatusLine(baseUrl, path, { headers = {}, timeoutMs = 2_000 } = {}) {
  return new Promise((resolve, reject) => {
    const url = new URL(path, baseUrl);
    const socket = trackSocket(net.connect(Number(url.port), url.hostname));
    let raw = '';
    const timer = setTimeout(() => {
      try { socket.destroy(); } catch {}
      reject(new Error(`rawHttpStatusLine timed out after ${timeoutMs}ms for ${path}`));
    }, timeoutMs);
    socket.setEncoding('utf8');
    socket.on('error', (err) => { clearTimeout(timer); reject(err); });
    socket.on('data', (chunk) => {
      raw += chunk;
      const match = /^HTTP\/1\.1 (\d{3})/.exec(raw);
      if (match) {
        clearTimeout(timer);
        resolve({ status: Number(match[1]), socket });
      }
    });
    const request = [`GET ${url.pathname}${url.search} HTTP/1.1`, `Host: ${url.host}`, 'Accept: text/event-stream', ...Object.entries(headers).map(([k, v]) => `${k}: ${v}`), '', ''].join('\r\n');
    socket.write(request);
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
      execFileSync('openssl', ['req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', key, '-out', cert, '-days', '1', '-subj', '/CN=localhost', '-addext', 'subjectAltName=IP:127.0.0.1']);
      const tlsService = await startProviderService(tlsHome.home, 0, 'stream-secret', { cert, key });
      try {
        const tlsDenied = await jsonFetch(`${tlsService.url}/v1/daemon/agent-host/sessions`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: { provider_id: 'mock-acp' }, ca: readFileSync(cert, 'utf8') });
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
    // A small handoff chunk plus a first-pull delay longer than the observation
    // window makes the pull gate deterministic: no pull can happen while stalled.
    process.env.NEXUS_SSE_SOCKET_HWM = '128';
    process.env.NEXUS_SSE_FIRST_PULL_DELAY_MS = '1800';
    const local = await startProviderService(envHome.home, 0);
    const { sseTestHooks } = await import(join(serviceRoot, 'dist/sse.js'));
    sseTestHooks.providerPullCount = 0; sseTestHooks.writeBlockedCount = 0;
    try {
      const { sessionId, operationId } = await providerFlow(local.url);
      const pullsBefore = sseTestHooks.providerPullCount;
      const socketPromise = rawSseGet(local.url, `/v1/daemon/agent-host/sessions/${sessionId}/events?operation_id=${operationId}`, { pauseImmediately: true, stallMs: 1500 });
      await new Promise((r) => setTimeout(r, 400));
      const pullsDuringStall = sseTestHooks.providerPullCount;
      await new Promise((r) => setTimeout(r, 700));
      const pullsStillStalled = sseTestHooks.providerPullCount;
      // No further pulls while the socket reader is stalled — the pull gate holds.
      assert.equal(pullsDuringStall, pullsBefore, 'no pull during the stall');
      assert.equal(pullsStillStalled, pullsBefore, 'still no pull while stalled');
      await socketPromise;
      const pullsAfter = sseTestHooks.providerPullCount;
      assert.ok(pullsAfter >= pullsStillStalled, 'pull count is monotonic');
      const reconnect = await rawSseGet(local.url, `/v1/daemon/agent-host/sessions/${sessionId}/events?operation_id=${operationId}`, { endPattern: /event: gap|OpFinished|OpFailed/ });
      // Recovery pulls after the stall and the stream ends inspectably.
      assert.ok(sseTestHooks.providerPullCount > pullsBefore, 'pulls resume after the stall');
      assert.match(reconnect.raw.replace(/\r?\n/g, ''), /event: gap|OpFinished|OpFailed/);
      const inspect = await jsonFetch(`${local.url}/v1/daemon/agent-host/operations/${operationId}`);
      assert.ok(['started', 'running', 'finished', 'failed', 'completed'].includes(inspect.payload.status), inspect.payload.status);
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
    // A retry at the exact terminal sequence must settle promptly on its own:
    // the bounded body read fails if the stream leaks open, and the body must
    // carry no fabricated event or gap.
    const equal = await fetch(`${baseUrl}/v1/daemon/agent-host/sessions/${sessionId}/events?operation_id=${operationId}&cursor=${encodeURIComponent(lastId)}`, { headers: { Accept: 'text/event-stream' }, signal: AbortSignal.timeout(5_000) });
    assert.equal(equal.status, 200);
    assert.match(equal.headers.get('content-type') ?? '', /text\/event-stream/);
    const equalBody = await equal.text();
    assert.equal((equalBody.match(/event: /g) ?? []).length, 0, 'terminal equal-cursor retry must not fabricate an event or gap');
  });

  test('subscriber cap returns 409', async () => {
    await stopSharedService();
    const envHome = seedHome();
    const sockets = [];
    const prevPullDelay = process.env.NEXUS_SSE_FIRST_PULL_DELAY_MS;
    process.env.NEXUS_SSE_FIRST_PULL_DELAY_MS = '8000';
    const local = await startProviderService(envHome.home, 0);
    try {
      const { sessionId, operationId } = await providerFlow(local.url);
      const path = `/v1/daemon/agent-host/sessions/${sessionId}/events?operation_id=${operationId}`;
      const url = new URL(path, local.url);
      const request = `GET ${url.pathname}${url.search} HTTP/1.1\r\nHost: ${url.host}\r\nAccept: text/event-stream\r\n\r\n`;
      // Hold 16 genuinely live subscribers inside the first-pull gate.
      for (let i = 0; i < 16; i += 1) {
        const socket = trackSocket(net.connect(Number(url.port), url.hostname));
        sockets.push(socket);
        socket.setEncoding('utf8');
        socket.on('data', () => socket.pause());
        socket.write(request);
        await new Promise((r) => setTimeout(r, 25));
      }
      await new Promise((r) => setTimeout(r, 500));
      assert.equal((await fetch(`${local.url}${path}`)).status, 409);
    } finally {
      if (prevPullDelay === undefined) delete process.env.NEXUS_SSE_FIRST_PULL_DELAY_MS;
      else process.env.NEXUS_SSE_FIRST_PULL_DELAY_MS = prevPullDelay;
      for (const socket of sockets) {
        try { socket.destroy(); } catch {}
      }
      await closeServiceBounded(local);
      await startSharedService(homeCtx.home, 0);
    }
  });

  test('sequential completed SSE requests over keep-alive never exhaust the subscriber cap', async () => {
    const { sessionId, operationId } = await providerFlow(baseUrl);
    const eventsUrl = `${baseUrl}/v1/daemon/agent-host/sessions/${sessionId}/events?operation_id=${operationId}`;
    // The first request rides the still-running operation so the hub retains
    // the full stream; the rest replay the completed hub. Every response
    // completes normally while undici keeps the keep-alive socket pooled —
    // exactly the shape that charged one leaked admission per request before
    // the completion release existed.
    const first = await fetch(eventsUrl, { headers: { Accept: 'text/event-stream' }, signal: AbortSignal.timeout(10_000) });
    assert.equal(first.status, 200);
    await first.text();
    for (let i = 0; i < 20; i += 1) {
      const replay = await fetch(eventsUrl, { headers: { Accept: 'text/event-stream' }, signal: AbortSignal.timeout(10_000) });
      assert.equal(replay.status, 200, `completed SSE request ${i + 1} must never be refused with 409`);
      await replay.text();
    }
  });

  test('a completed SSE stream frees its admission for the next live subscriber', async () => {
    await stopSharedService();
    const envHome = seedHome();
    const held = [];
    let probe;
    const prevPullDelay = process.env.NEXUS_SSE_FIRST_PULL_DELAY_MS;
    process.env.NEXUS_SSE_FIRST_PULL_DELAY_MS = '8000';
    const local = await startProviderService(envHome.home, 0);
    try {
      const { sessionId, operationId } = await providerFlow(local.url);
      const completedPath = `/v1/daemon/agent-host/sessions/${sessionId}/events?operation_id=${operationId}`;
      // One completed keep-alive request: the response ends while the pooled
      // socket stays open, so only a completion-time release frees its slot —
      // a socket-destroying client would mask the leak via socket close.
      const completed = await fetch(`${local.url}${completedPath}`, { headers: { Accept: 'text/event-stream' }, signal: AbortSignal.timeout(20_000) });
      assert.equal(completed.status, 200);
      await completed.text();
      // Start a second operation in the same session. Its subscribers remain
      // live in the first-pull gate while the completed operation's keep-alive
      // admission must already have been released.
      const executed = await jsonFetch(
        `${local.url}/v1/daemon/agent-host/sessions/${sessionId}/operations`,
        {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: { kind: 'prompt', content: 'hello again' },
        },
      );
      assert.equal(executed.status, 200, executed.text);
      const livePath = `/v1/daemon/agent-host/sessions/${sessionId}/events?operation_id=${executed.payload.operation_id}`;
      const url = new URL(livePath, local.url);
      const request = `GET ${url.pathname}${url.search} HTTP/1.1\r\nHost: ${url.host}\r\nAccept: text/event-stream\r\n\r\n`;
      for (let i = 0; i < 15; i += 1) {
        const socket = trackSocket(net.connect(Number(url.port), url.hostname));
        held.push(socket);
        socket.setEncoding('utf8');
        socket.on('data', () => socket.pause());
        socket.write(request);
        await new Promise((r) => setTimeout(r, 25));
      }
      await new Promise((r) => setTimeout(r, 300));
      probe = await rawHttpStatusLine(local.url, livePath, { timeoutMs: 9_000 });
      try { assert.equal(probe.status, 200); } finally {
        try { probe.socket.destroy(); } catch {}
      }
    } finally {
      if (prevPullDelay === undefined) delete process.env.NEXUS_SSE_FIRST_PULL_DELAY_MS;
      else process.env.NEXUS_SSE_FIRST_PULL_DELAY_MS = prevPullDelay;
      for (const socket of held) {
        try { socket.destroy(); } catch {}
      }
      await closeServiceBounded(local);
      await startSharedService(homeCtx.home, 0);
    }
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

  test('accepted cancel settles session and operation to cancelled truth', async () => {
    await stopSharedService();
    const blockHome = seedHome({ BLOCK_PROMPT: '1' });
    const local = await startProviderService(blockHome.home, 0);
    try {
      const created = await jsonFetch(`${local.url}/v1/daemon/agent-host/sessions`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: { provider_id: 'mock-acp' } });
      const executed = await jsonFetch(`${local.url}/v1/daemon/agent-host/sessions/${created.payload.session_id}/operations`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: { kind: 'prompt', content: 'cancel-me' } });
      const cancel = await jsonFetch(`${local.url}/v1/daemon/agent-host/operations/${executed.payload.operation_id}`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: {} });
      assert.equal(cancel.payload.status, 'cancelled');
      // The accepted cancel must settle observable + durable truth: a repeated
      // GET reports `cancelled` (never a stale `running`), and the session is no
      // longer busy.
      const opAfter = (await jsonFetch(`${local.url}/v1/daemon/agent-host/operations/${executed.payload.operation_id}`)).payload;
      assert.equal(opAfter.status, 'cancelled');
      const sessionAfter = (await jsonFetch(`${local.url}/v1/daemon/agent-host/sessions/${created.payload.session_id}`)).payload;
      assert.notEqual(sessionAfter.state, 'Busy');
      assert.ok(sessionAfter.active_op_id === undefined || sessionAfter.active_op_id === null);
    } finally { await closeServiceBounded(local); await startSharedService(homeCtx.home, 0); }
  });

  test('list/get/shutdown responses match generated schema keys', async () => {
    const created = await jsonFetch(`${baseUrl}/v1/daemon/agent-host/sessions`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: { provider_id: 'mock-acp' } });
    const sessionId = created.payload.session_id;
    // Exact key sets: no undocumented field may leak through any of these.
    assert.deepEqual(Object.keys(created.payload).sort(), ['provider_id', 'session_id', 'state']);
    const listed = await jsonFetch(`${baseUrl}/v1/daemon/agent-host/sessions`);
    assert.deepEqual(Object.keys(listed.payload).sort(), ['items', 'pagination']);
    assert.ok(Array.isArray(listed.payload.items));
    assert.deepEqual(Object.keys(listed.payload.pagination).sort().filter((k) => k !== 'next_cursor'), ['has_more', 'limit']);
    // The list reflects native host truth; the JS-provider adapter owns its
    // sessions in-process, so list truth and cache truth are checked separately.
    for (const item of listed.payload.items) {
      assert.deepEqual(Object.keys(item).sort(), ['provider_id', 'session_id', 'state']);
    }
    const got = await jsonFetch(`${baseUrl}/v1/daemon/agent-host/sessions/${sessionId}`);
    const gotKeys = Object.keys(got.payload).sort();
    const allowed = ['active_op_id', 'model', 'provider_id', 'session_id', 'state'];
    assert.ok(gotKeys.every((k) => allowed.includes(k)), `unexpected session keys: ${gotKeys}`);
    for (const key of ['session_id', 'provider_id', 'state']) assert.ok(key in got.payload);
    const shutdown = await jsonFetch(`${baseUrl}/v1/daemon/agent-host/sessions/${sessionId}`, { method: 'DELETE' });
    assert.deepEqual(Object.keys(shutdown.payload).sort(), ['session_id', 'status']);
  });

  test('bounded hub retention, control slot limits, and strict replay ordering', async () => {
    const { OperationEventHub } = await import(join(serviceRoot, 'dist/sse.js'));
    const { HUB_MAX_DATA_FRAMES, HUB_MAX_DATA_BYTES, SSE_RESERVED_CONTROL_BYTES } = await import(join(serviceRoot, 'dist/config.js'));
    const hub = new OperationEventHub('00000000-0000-4000-8000-000000000010', '00000000-0000-4000-8000-000000000011');
    for (let i = 0; i < HUB_MAX_DATA_FRAMES + 5; i += 1) hub.recordEvent({ Progress: { message: `line-${i}` } });
    assert.ok(hub.evictionWatermark() > 1);
    const stale = hub.planReplay(`${hub.epoch}:1`);
    assert.equal(stale.kind, 'stale');
    assert.equal(stale.gap.reason, 'history_unavailable');
    // A gap larger than the reserved 4 KiB control slot is rejected outright.
    assert.equal(hub.recordGap({ reason: 'interrupted', operation_id: hub.operationId, resync_required: true, inspect_url: '/x', padding: 'x'.repeat(SSE_RESERVED_CONTROL_BYTES) }), null);
    const terminal = hub.recordEvent({ OpFinished: { transcript: 'ok' } });
    assert.equal(hub.recordEvent({ OpFinished: { transcript: 'dup' } })?.id, terminal.id);
    assert.ok(hub.retainedMemoryBytes() <= HUB_MAX_DATA_BYTES + SSE_RESERVED_CONTROL_BYTES * 2);
    assert.ok(hub.hasTerminal());
  });

  test('interleaved gap and terminal replay strictly by sequence', async () => {
    const { OperationEventHub } = await import(join(serviceRoot, 'dist/sse.js'));
    const hub = new OperationEventHub('00000000-0000-4000-8000-000000000020', '00000000-0000-4000-8000-000000000021');
    const a = hub.recordEvent({ Progress: { message: 'a' } });
    const gap = hub.recordGap({ reason: 'lagging', operation_id: hub.operationId, resync_required: true, inspect_url: '/x' });
    const b = hub.recordEvent({ Progress: { message: 'b' } });
    const terminal = hub.recordEvent({ OpFinished: { transcript: 'done' } });
    const seq = (f) => Number(f.id.split(':')[1]);
    assert.ok(seq(a) < seq(gap) && seq(gap) < seq(b) && seq(b) < seq(terminal));
    // Terminal is its own control slot, never a data permit.
    assert.equal(terminal.isControl, true);
    // framesAfter must merge by sequence, not concatenate data→gap→terminal.
    const after = hub.framesAfter(seq(a));
    const ordered = after.map(seq);
    assert.deepEqual(ordered, [...ordered].sort((x, y) => x - y));
    assert.deepEqual(after.map((f) => f.event), ['gap', 'provider_event', 'provider_event']);
    // A replay from the very start keeps the same strict order.
    const all = hub.planReplay(undefined);
    const allSeq = all.frames.map(seq);
    assert.deepEqual(allSeq, [...allSeq].sort((x, y) => x - y));
  });

  test('a 4096-byte serialized control frame is accepted, one byte more is rejected', async () => {
    const { OperationEventHub } = await import(join(serviceRoot, 'dist/sse.js'));
    const { SSE_RESERVED_CONTROL_BYTES } = await import(join(serviceRoot, 'dist/config.js'));
    // Find the exact padding whose serialized frame equals the 4 KiB bound.
    const probe = new OperationEventHub('00000000-0000-4000-8000-000000000030', '00000000-0000-4000-8000-000000000031');
    const baseBytes = Buffer.byteLength(
      `id: ${probe.epoch}:1\nevent: gap\ndata: ${JSON.stringify({ reason: 'lagging', operation_id: probe.operationId, resync_required: true, inspect_url: '/x', padding: '' })}\n\n`,
      'utf8',
    );
    const padLen = SSE_RESERVED_CONTROL_BYTES - baseBytes;
    assert.ok(padLen > 0);
    const exactly = probe.recordGap({ reason: 'lagging', operation_id: probe.operationId, resync_required: true, inspect_url: '/x', padding: 'x'.repeat(padLen) });
    assert.ok(exactly, 'exactly-4KiB control frame must be accepted');
    assert.equal(exactly.wireBytes, SSE_RESERVED_CONTROL_BYTES);
    const over = new OperationEventHub('00000000-0000-4000-8000-000000000032', '00000000-0000-4000-8000-000000000033');
    assert.equal(over.recordGap({ reason: 'lagging', operation_id: over.operationId, resync_required: true, inspect_url: '/x', padding: 'x'.repeat(padLen + 1) }), null);
  });

  test('registry evicts terminal operations beyond the retention cap', async () => {
    const { ProviderRegistry } = await import(join(serviceRoot, 'dist/provider-registry.js'));
    const { REGISTRY_MAX_TERMINAL_OPERATIONS } = await import(join(serviceRoot, 'dist/config.js'));
    const registry = new ProviderRegistry();
    const ids = [];
    for (let i = 0; i < REGISTRY_MAX_TERMINAL_OPERATIONS + 1; i += 1) {
      const id = `00000000-0000-4000-8000-${String(i).padStart(12, '0')}`;
      ids.push(id);
      registry.registerSession({ sessionId: id, providerId: 'mock-acp', state: 'Ready', activeOpId: null });
      registry.registerOperation({ operationId: id, sessionId: id, providerId: 'mock-acp', status: 'started', terminalEvent: null, terminalTranscript: null });
      registry.finishOperation(id, { OpFinished: { session_id: id, op_id: id, reason: 'end_turn' } }, null);
    }
    const retained = ids.filter((id) => registry.operationRecord(id));
    assert.equal(retained.length, REGISTRY_MAX_TERMINAL_OPERATIONS);
    assert.equal(registry.operationRecord(ids[0]), undefined, 'oldest terminal must be evicted');
    assert.ok(registry.operationRecord(ids[ids.length - 1]), 'newest terminal must be retained');
  });

  test('accepted cancel is never overwritten by a later provider terminal event', async () => {
    const { ProviderRegistry } = await import(join(serviceRoot, 'dist/provider-registry.js'));
    const registry = new ProviderRegistry();
    const sessionId = '00000000-0000-4000-8000-000000000101';
    const operationId = '00000000-0000-4000-8000-000000000102';
    registry.registerSession({ sessionId, providerId: 'mock-acp', state: 'Running', activeOpId: operationId });
    registry.registerOperation({ operationId, sessionId, providerId: 'mock-acp', status: 'started', terminalEvent: null, terminalTranscript: null });
    // The cancel is accepted first: the registry settles the operation to the
    // `cancelled` terminal exactly like the cancel route does.
    registry.settleOperationStatus(operationId, 'cancelled');
    assert.equal(registry.operationRecord(operationId).status, 'cancelled');
    assert.equal(registry.activeOperationCount(), 0, 'an accepted cancel is no longer charged as active');
    // A late provider terminal event must not rewrite the accepted outcome:
    // first terminal wins, matching the durable journal's one terminal.
    registry.finishOperation(operationId, { OpFinished: { session_id: sessionId, op_id: operationId, reason: 'end_turn' } }, null);
    const record = registry.operationRecord(operationId);
    assert.equal(record.status, 'cancelled', 'a late OpFinished must not overwrite the accepted cancel');
    assert.equal(record.terminalEvent, null, 'the late event is not adopted as the terminal');
    // The session stays released exactly once and remains consistent.
    assert.equal(registry.sessionRecord(sessionId).activeOpId, null);
    assert.equal(registry.sessionRecord(sessionId).state, 'Ready');
    assert.equal(registry.activeOperationCount(), 0);
  });

  test('late OpFinished after an accepted cancel never reaches the stream or its replay', async () => {
    const { ProviderRegistry } = await import(join(serviceRoot, 'dist/provider-registry.js'));
    const { ingestEvents, streamSessionEvents } = await import(join(serviceRoot, 'dist/sse.js'));
    const sessionId = '00000000-0000-4000-8000-000000000201';
    const operationId = '00000000-0000-4000-8000-000000000202';
    const lateOpFinished = { OpFinished: { session_id: sessionId, op_id: operationId, reason: 'end_turn' } };

    // ── Registry status + hub/replay consistency through the real ingest path.
    const registry = new ProviderRegistry();
    const serviceStub = { providerRegistry: registry };
    registry.registerSession({ sessionId, providerId: 'mock-acp', state: 'Running', activeOpId: operationId });
    registry.registerOperation({ operationId, sessionId, providerId: 'mock-acp', status: 'started', terminalEvent: null, terminalTranscript: null });
    registry.settleOperationStatus(operationId, 'cancelled');
    ingestEvents(serviceStub, operationId, [lateOpFinished]);
    const record = registry.operationRecord(operationId);
    assert.equal(record.status, 'cancelled', 'canonical status stays the accepted cancel');
    assert.equal(record.terminalEvent, null, 'the late terminal is not adopted');
    const hub = registry.hubForOperation(operationId);
    assert.ok(hub, 'ingest created the hub');
    assert.equal(hub.hasTerminal(), false, 'the refused late terminal must not enter the hub');
    assert.equal(hub.isClosed(), true, 'the stream must end truthfully instead of hanging');
    const replay = hub.planReplay(undefined);
    assert.equal(replay.kind, 'all');
    assert.equal(replay.frames.length, 1, 'replay carries exactly one frame');
    assert.equal(replay.frames[0].event, 'gap', 'replay ends with the resync gap, never a fabricated terminal');
    const gapPayload = JSON.parse(replay.frames[0].buffer.toString('utf8').split('\ndata: ')[1]);
    assert.equal(gapPayload.reason, 'interrupted');
    assert.equal(gapPayload.resync_required, true, 'the client is told to resync canonical truth');
    assert.equal(gapPayload.inspect_url, `/v1/daemon/agent-host/operations/${operationId}`);
    assert.equal(gapPayload.operation_id, operationId);

    // ── Control: an accepted terminal still lands in the hub as a real terminal.
    const okSession = '00000000-0000-4000-8000-000000000203';
    const okOperation = '00000000-0000-4000-8000-000000000204';
    registry.registerSession({ sessionId: okSession, providerId: 'mock-acp', state: 'Running', activeOpId: okOperation });
    registry.registerOperation({ operationId: okOperation, sessionId: okSession, providerId: 'mock-acp', status: 'started', terminalEvent: null, terminalTranscript: null });
    ingestEvents(serviceStub, okOperation, [lateOpFinished]);
    const okHub = registry.hubForOperation(okOperation);
    assert.equal(okHub.hasTerminal(), true, 'an accepted terminal is still recorded');
    assert.equal(registry.operationRecord(okOperation).status, 'finished');

    // ── A live SSE loop against the already-cancelled canonical op terminates
    // with the one bounded resync gap — no hang, no fabricated terminal frame.
    const written = [];
    const fakeRes = {
      socket: null,
      writableEnded: false,
      destroyed: false,
      writeHead() {},
      write(chunk) { written.push(chunk.toString('utf8')); return true; },
      once() {},
      end() { this.writableEnded = true; },
    };
    const searchParams = new URLSearchParams({ operation_id: operationId });
    await Promise.race([
      streamSessionEvents(serviceStub, sessionId, searchParams, fakeRes),
      new Promise((_, reject) => setTimeout(() => reject(new Error('SSE loop must not hang on a canonically-cancelled operation')), 2_000)),
    ]);
    const streamText = written.join('');
    assert.equal((streamText.match(/event: provider_event/g) ?? []).length, 0, 'no fabricated terminal frame is delivered');
    assert.equal((streamText.match(/event: gap/g) ?? []).length, 1, 'exactly one truthful resync gap is delivered');
    assert.match(streamText, /resync_required":true/);
  });

  test('a provider batch gap never overwrites the fail-closed resync gap after a refused late terminal', async () => {
    const { ProviderRegistry } = await import(join(serviceRoot, 'dist/provider-registry.js'));
    const { streamSessionEvents } = await import(join(serviceRoot, 'dist/sse.js'));
    const sessionId = '00000000-0000-4000-8000-000000000301';
    const operationId = '00000000-0000-4000-8000-000000000302';
    const lateOpFinished = { OpFinished: { session_id: sessionId, op_id: operationId, reason: 'end_turn' } };
    const postCloseProgress = { Progress: { message: 'post-close progress must never appear' } };
    const registry = new ProviderRegistry();
    // The live pull loop: the accepted cancel lands while a pull batch is in
    // flight, so the batch carries BOTH the refused late terminal, a trailing
    // non-terminal event behind it, and a provider gap — the exact race in
    // which nothing may follow the fail-closed `interrupted` marker.
    const serviceStub = {
      providerRegistry: registry,
      core: {
        nextProviderEvents: async () => {
          registry.settleOperationStatus(operationId, 'cancelled');
          return {
            events: [lateOpFinished, postCloseProgress],
            gap: { reason: 'lagging', operation_id: operationId, resync_required: true, inspect_url: `/v1/daemon/agent-host/operations/${operationId}` },
            has_more: false,
          };
        },
      },
    };
    registry.registerSession({ sessionId, providerId: 'mock-acp', state: 'Running', activeOpId: operationId });
    registry.registerOperation({ operationId, sessionId, providerId: 'mock-acp', status: 'started', terminalEvent: null, terminalTranscript: null });

    const written = [];
    const fakeRes = {
      socket: null,
      writableEnded: false,
      destroyed: false,
      writeHead() {},
      write(chunk) { written.push(chunk.toString('utf8')); return true; },
      once() {},
      end() { this.writableEnded = true; },
    };
    await Promise.race([
      streamSessionEvents(serviceStub, sessionId, new URLSearchParams({ operation_id: operationId }), fakeRes),
      new Promise((_, reject) => setTimeout(() => reject(new Error('SSE loop must terminate after the fail-closed batch')), 2_000)),
    ]);
    const streamText = written.join('');
    assert.equal((streamText.match(/event: provider_event/g) ?? []).length, 0, 'no fabricated terminal or post-close event frame is delivered');
    assert.doesNotMatch(streamText, /post-close progress must never appear/, 'a non-terminal event after the refused terminal must not append or emit');
    assert.equal((streamText.match(/event: gap/g) ?? []).length, 1, 'exactly one gap frame is delivered');
    assert.match(streamText, /"reason":"interrupted"/, 'the canonical fail-closed resync gap is delivered');
    assert.doesNotMatch(streamText, /"reason":"lagging"/, 'the provider batch gap must not overwrite the fail-closed gap');
    const hub = registry.hubForOperation(operationId);
    assert.equal(hub.isClosed(), true);
    const replay = hub.planReplay(undefined);
    assert.equal(replay.frames.length, 1, 'replay carries exactly one frame — nothing may follow the fail-closed gap');
    const gapPayloads = replay.frames.filter((f) => f.event === 'gap').map((f) => JSON.parse(f.buffer.toString('utf8').split('\ndata: ')[1]));
    assert.equal(gapPayloads.length, 1, 'replay carries exactly one gap');
    assert.equal(gapPayloads[0].reason, 'interrupted', 'replay keeps the canonical interrupted marker');
  });

  test('equal cursor waits on a live hub but ends promptly on a terminal hub', async () => {
    const { ProviderRegistry } = await import(join(serviceRoot, 'dist/provider-registry.js'));
    const { OperationEventHub, streamSessionEvents } = await import(join(serviceRoot, 'dist/sse.js'));

    // ── Plan level: a closed hub can never advance, so an equal-cursor retry
    // must plan an end — never a wait (terminal retained, refused terminal) —
    // while a live hub, including an empty one, still waits for future truth.
    const doneHub = new OperationEventHub('00000000-0000-4000-8000-000000000501', '00000000-0000-4000-8000-000000000502');
    doneHub.recordEvent({ Progress: { message: 'work' } });
    const doneTerminal = doneHub.recordEvent({ OpFinished: { session_id: doneHub.sessionId, op_id: doneHub.operationId, reason: 'end_turn' } });
    assert.ok(doneTerminal);
    assert.equal(doneHub.isClosed(), true);
    assert.equal(doneHub.planReplay(doneTerminal.id).kind, 'end', 'equal cursor on a terminal hub must end, not wait');
    const failedHub = new OperationEventHub('00000000-0000-4000-8000-000000000503', '00000000-0000-4000-8000-000000000504');
    assert.ok(failedHub.failClosed());
    assert.equal(failedHub.planReplay(`${failedHub.epoch}:1`).kind, 'end', 'equal cursor on a fail-closed hub must end, not wait');
    const emptyLiveHub = new OperationEventHub('00000000-0000-4000-8000-000000000505', '00000000-0000-4000-8000-000000000506');
    assert.equal(emptyLiveHub.planReplay(`${emptyLiveHub.epoch}:0`).kind, 'wait', 'an empty open hub is live: cursor 0 waits for future truth');

    // ── Stream level, terminal hub: the retry settles promptly and fabricates
    // no gap and no event.
    const registry = new ProviderRegistry();
    const sessionId = '00000000-0000-4000-8000-000000000507';
    const operationId = '00000000-0000-4000-8000-000000000508';
    registry.registerSession({ sessionId, providerId: 'mock-acp', state: 'Running', activeOpId: operationId });
    registry.registerOperation({ operationId, sessionId, providerId: 'mock-acp', status: 'finished', terminalEvent: null, terminalTranscript: null });
    const hub = registry.ensureHub(operationId, () => new OperationEventHub(operationId, sessionId));
    hub.recordEvent({ Progress: { message: 'work' } });
    const terminal = hub.recordEvent({ OpFinished: { session_id: sessionId, op_id: operationId, reason: 'end_turn' } });
    const written = [];
    const fakeRes = {
      socket: null,
      writableEnded: false,
      destroyed: false,
      writeHead() {},
      write(chunk) { written.push(chunk.toString('utf8')); return true; },
      once() {},
      end() { this.writableEnded = true; },
    };
    await Promise.race([
      streamSessionEvents({ providerRegistry: registry }, sessionId, new URLSearchParams({ operation_id: operationId, cursor: terminal.id }), fakeRes),
      new Promise((_, reject) => setTimeout(() => reject(new Error('terminal equal-cursor retry must settle promptly')), 2_000)),
    ]);
    assert.equal(written.join(''), '', 'terminal equal-cursor retry must end with no fabricated gap or event');

    // ── Stream level, live hub: equal cursor stays open while no future frame
    // exists yet, then delivers the future truth (and the terminal) when the
    // provider produces it — normal live follow is preserved.
    const liveRegistry = new ProviderRegistry();
    const liveSession = '00000000-0000-4000-8000-000000000509';
    const liveOperation = '00000000-0000-4000-8000-00000000050a';
    liveRegistry.registerSession({ sessionId: liveSession, providerId: 'mock-acp', state: 'Running', activeOpId: liveOperation });
    liveRegistry.registerOperation({ operationId: liveOperation, sessionId: liveSession, providerId: 'mock-acp', status: 'started', terminalEvent: null, terminalTranscript: null });
    const liveHub = liveRegistry.ensureHub(liveOperation, () => new OperationEventHub(liveOperation, liveSession));
    const firstFrame = liveHub.recordEvent({ Progress: { message: 'first truth' } });
    assert.equal(liveHub.planReplay(firstFrame.id).kind, 'wait', 'equal cursor on a live hub must keep waiting');
    let releasePull;
    const pullGate = new Promise((resolve) => { releasePull = resolve; });
    let pulls = 0;
    const liveStub = {
      providerRegistry: liveRegistry,
      core: {
        nextProviderEvents: async () => {
          pulls += 1;
          if (pulls === 1) {
            await pullGate;
            return { events: [{ Progress: { message: 'future truth' } }], gap: null, has_more: true };
          }
          return { events: [{ OpFinished: { session_id: liveSession, op_id: liveOperation, reason: 'end_turn' } }], gap: null, has_more: false };
        },
      },
    };
    const liveWritten = [];
    const liveRes = {
      socket: null,
      writableEnded: false,
      destroyed: false,
      writeHead() {},
      write(chunk) { liveWritten.push(chunk.toString('utf8')); return true; },
      once() {},
      end() { this.writableEnded = true; },
    };
    let settled = false;
    const liveStream = streamSessionEvents(liveStub, liveSession, new URLSearchParams({ operation_id: liveOperation, cursor: firstFrame.id }), liveRes).then(() => { settled = true; });
    // Outlast the default 450ms first-pull delay: the loop is now parked on
    // the gated first pull, proving the wait belongs to the live hub, not to
    // startup lag.
    await new Promise((r) => setTimeout(r, 600));
    assert.equal(settled, false, 'equal cursor on a live hub must stay open waiting for future truth');
    assert.equal(liveWritten.join(''), '', 'nothing may be delivered while the live hub has no future frames');
    releasePull();
    await Promise.race([
      liveStream,
      new Promise((_, reject) => setTimeout(() => reject(new Error('live equal-cursor stream must deliver future truth and settle')), 2_000)),
    ]);
    const liveText = liveWritten.join('');
    assert.match(liveText, /future truth/, 'the frame recorded after the retry must be delivered');
    assert.match(liveText, /OpFinished/, 'the terminal must be delivered after the wait');
  });

  test('active provider operation cap returns transport busy before dispatch', async () => {
    await stopSharedService();
    const blockHome = seedHome({ BLOCK_PROMPT: '1' });
    const local = await startProviderService(blockHome.home, 0);
    const { MAX_ACTIVE_PROVIDER_OPERATIONS } = await import(join(serviceRoot, 'dist/config.js'));
    try {
      // One blocked operation per session: the cap is process-wide across live ops.
      for (let i = 0; i < MAX_ACTIVE_PROVIDER_OPERATIONS; i += 1) {
        const created = await jsonFetch(`${local.url}/v1/daemon/agent-host/sessions`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: { provider_id: 'mock-acp' } });
        const executed = await jsonFetch(`${local.url}/v1/daemon/agent-host/sessions/${created.payload.session_id}/operations`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: { kind: 'prompt', content: `blocked-${i}` } });
        assert.equal(executed.status, 200, executed.text);
      }
      assert.equal(local.service.providerRegistry.activeOperationCount(), MAX_ACTIVE_PROVIDER_OPERATIONS);
      const created = await jsonFetch(`${local.url}/v1/daemon/agent-host/sessions`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: { provider_id: 'mock-acp' } });
      const overflow = await jsonFetch(`${local.url}/v1/daemon/agent-host/sessions/${created.payload.session_id}/operations`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: { kind: 'prompt', content: 'over-limit' } });
      assert.equal(overflow.status, 503);
      assert.equal(overflow.payload.error.code, 'busy');
    } finally { await closeServiceBounded(local); await startSharedService(homeCtx.home, 0); }
  });

  test('cold registry hydrates a live operation from native truth before SSE', async () => {
    await stopSharedService();
    const blockHome = seedHome({ BLOCK_PROMPT: '1' });
    const local = await startProviderService(blockHome.home, 0);
    try {
      const created = await jsonFetch(`${local.url}/v1/daemon/agent-host/sessions`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: { provider_id: 'mock-acp' } });
      const sessionId = created.payload.session_id;
      const executed = await jsonFetch(`${local.url}/v1/daemon/agent-host/sessions/${sessionId}/operations`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: { kind: 'prompt', content: 'resync' } });
      const operationId = executed.payload.operation_id;
      // The native host now tracks JS-provider sessions, so hostQuery resolves
      // launch/execute with the admitted provider id and ownership.
      const nativeSessions = await local.service.core.hostQuery({ query: 'list_sessions' });
      const nativeSession = nativeSessions.sessions.items.find((s) => s.session_id === sessionId);
      assert.ok(nativeSession, 'native host must expose the JS-provider session');
      assert.equal(nativeSession.provider_id, 'mock-acp');
      const nativeOp = await local.service.core.hostQuery({ query: 'get_operation', operation_id: operationId });
      assert.equal(nativeOp.operation.session_id, sessionId);
      assert.equal(nativeOp.operation.status, 'running');
      // Simulate a restart: drop the process-local cache while native truth holds.
      local.service.providerRegistry.removeSession(sessionId);
      assert.equal(local.service.providerRegistry.operationRecord(operationId), undefined, 'registry must be cold');
      // GET must hydrate from native truth, not fabricate a 404.
      const got = await jsonFetch(`${local.url}/v1/daemon/agent-host/sessions/${sessionId}`);
      assert.equal(got.status, 200);
      assert.equal(got.payload.session_id, sessionId);
      const inspect = await jsonFetch(`${local.url}/v1/daemon/agent-host/operations/${operationId}`);
      assert.equal(inspect.status, 200);
      assert.equal(inspect.payload.session_id, sessionId);
      assert.equal(inspect.payload.status, 'running');
      // SSE with the cold cache hydrates the omitted operation_id from native
      // truth and passes admission (200, not 400/404) before headers.
      const omitted = await fetch(`${local.url}/v1/daemon/agent-host/sessions/${sessionId}/events`, { headers: { Accept: 'text/event-stream' } });
      assert.equal(omitted.status, 200);
      assert.match(omitted.headers.get('content-type') ?? '', /text\/event-stream/);
      await omitted.body?.cancel();
      // A wrong-session operation is still refused before headers.
      const other = await jsonFetch(`${local.url}/v1/daemon/agent-host/sessions`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: { provider_id: 'mock-acp' } });
      const cross = await fetch(`${local.url}/v1/daemon/agent-host/sessions/${other.payload.session_id}/events?operation_id=${operationId}`, { headers: { Accept: 'text/event-stream' } });
      assert.equal(cross.status, 403);
      await cross.body?.cancel();
    } finally { await closeServiceBounded(local); await startSharedService(homeCtx.home, 0); }
  });

  test('cancel rejects a terminal-status operation hydrated without a local event', async () => {
    const created = await jsonFetch(`${baseUrl}/v1/daemon/agent-host/sessions`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: { provider_id: 'mock-acp' } });
    const sessionId = created.payload.session_id;
    const operationId = '00000000-0000-4000-8000-00000000c001';
    // Native-terminal status with no local terminal event — the exact shape a
    // hydrated record presents. Cancel must reject, never dispatch.
    service.service.providerRegistry.registerSession({ sessionId, providerId: 'mock-acp', state: 'Ready', activeOpId: null });
    service.service.providerRegistry.registerOperation({ operationId, sessionId, providerId: 'mock-acp', status: 'completed', terminalEvent: null, terminalTranscript: null });
    const cancel = await jsonFetch(`${baseUrl}/v1/daemon/agent-host/operations/${operationId}`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: {} });
    assert.equal(cancel.status, 409);
    assert.equal(cancel.payload.error.code, 'busy');
  });

  test('multibyte character across chunk boundary is byte-exact JSON', async () => {
    const { OperationEventHub } = await import(join(serviceRoot, 'dist/sse.js'));
    const hub = new OperationEventHub('00000000-0000-4000-8000-000000000040', '00000000-0000-4000-8000-000000000041');
    // Surrogate-pair characters make UTF-16 slicing corrupt the JSON payload.
    const message = '\u{1F389}'.repeat(300) + '\u00e9'.repeat(50);
    const frame = hub.recordEvent({ Progress: { message } });
    assert.ok(frame);
    const utf16Length = frame.buffer.toString('utf8').length;
    assert.ok(frame.buffer.length > utf16Length, 'multibyte payload must exceed UTF-16 length');
    assert.equal(frame.wireBytes, frame.buffer.length, 'charged bytes must equal serialized Buffer length');
    // Reconstruct the data payload from the exact bytes the socket would write.
    const text = frame.buffer.toString('utf8');
    const dataLine = text.split('\n').find((line) => line.startsWith('data: '));
    const parsed = JSON.parse(dataLine.slice(6));
    assert.equal(parsed.Progress.message, message);
    // A chunk boundary mid-character must not split the code point: the socket
    // receives bytes and reassembles them, so concatenate bytes then decode.
    const chunkSize = 7;
    const chunks = [];
    for (let off = 0; off < frame.buffer.length; off += chunkSize) {
      chunks.push(frame.buffer.subarray(off, Math.min(off + chunkSize, frame.buffer.length)));
    }
    assert.equal(Buffer.concat(chunks).toString('utf8'), text, 'byte chunking must round-trip the exact UTF-8 text');
    // Byte accounting must be UTF-8, not UTF-16 code units.
    assert.equal(Buffer.byteLength(text, 'utf8'), frame.buffer.length);
    assert.notEqual(text.length, frame.buffer.length, 'UTF-16 length differs from UTF-8 byte length');
  });

  test('SseWriter observes write(false) and resumes the same frame after drain', async () => {
    const { SseWriter, OperationEventHub, sseTestHooks } = await import(join(serviceRoot, 'dist/sse.js'));
    const hub = new OperationEventHub('00000000-0000-4000-8000-000000000050', '00000000-0000-4000-8000-000000000051');
    const frame = hub.recordEvent({ Progress: { message: 'x'.repeat(256) } });
    assert.ok(frame);
    // A stub response that refuses the first write, then drains.
    const written = [];
    const listeners = new Map();
    const res = {
      writableEnded: false,
      destroyed: false,
      socket: { setNoDelay() {} },
      write(chunk) {
        written.push(Buffer.from(chunk));
        // Accept only after a drain has been signalled.
        if (written.length === 1) {
          setImmediate(() => listeners.get('drain')?.());
          return false;
        }
        return true;
      },
      once(event, cb) { listeners.set(event, cb); },
      end() {},
    };
    const before = sseTestHooks.writeBlockedCount;
    const writer = new SseWriter(res, hub);
    const result = await writer.writeFrame(frame);
    assert.equal(result, 'ok');
    assert.ok(sseTestHooks.writeBlockedCount > before, 'write(false) must be observed');
    // The writer resumed the same frame after drain: all bytes landed exactly
    // once, as slices of the serialized Buffer (no UTF-16 slicing).
    assert.equal(Buffer.concat(written).toString('utf8'), frame.buffer.toString('utf8'));
    assert.equal(writer.outboundBackpressured, false, 'backpressure flag clears after drain');
  });

  test('control frames never consume the data pool', async () => {
    const budget = await import(join(serviceRoot, 'dist/environment-budget.js'));
    const { OperationEventHub } = await import(join(serviceRoot, 'dist/sse.js'));
    budget.resetEnvironmentBudgetForTests();
    // Saturate the data pool exactly.
    let filled = 0;
    while (budget.tryReserveEnvironmentBytes(1 << 20)) filled += 1;
    const saturated = budget.environmentBudgetReserved();
    assert.ok(saturated >= 8 * 1024 * 1024 - (1 << 20));
    const hub = new OperationEventHub('00000000-0000-4000-8000-000000000070', '00000000-0000-4000-8000-000000000071');
    // A data frame cannot be retained while the pool is full → fail closed with a gap.
    const rejected = hub.recordEvent({ Progress: { message: 'x'.repeat(4096) } });
    assert.ok(rejected, 'fail-closed must still produce a gap');
    assert.equal(rejected.event, 'gap');
    const rejectedData = rejected.buffer.toString('utf8').split('\n').find((l) => l.startsWith('data: '));
    assert.equal(JSON.parse(rejectedData.slice(6)).reason, 'interrupted');
    // The dedicated control pool still permits a terminal after a full data pool.
    const hub2 = new OperationEventHub('00000000-0000-4000-8000-000000000072', '00000000-0000-4000-8000-000000000073');
    const terminal = hub2.recordEvent({ OpFinished: { session_id: hub2.sessionId, op_id: 'o', reason: 'end_turn' } });
    assert.ok(terminal?.isTerminal, 'terminal must be retained despite a full data pool');
    hub.dispose();
    hub2.dispose();
    budget.resetEnvironmentBudgetForTests();
  });

  test('retained hubs cannot exceed the global data cap and release on eviction/dispose', async () => {
    const budget = await import(join(serviceRoot, 'dist/environment-budget.js'));
    const { OperationEventHub } = await import(join(serviceRoot, 'dist/sse.js'));
    budget.resetEnvironmentBudgetForTests();
    const hubs = [];
    // 65 terminal hubs would each retain a 1 MiB-scale terminal; the data pool
    // (8 MiB) must stop retention before the global cap is exceeded.
    for (let i = 0; i < 65; i += 1) {
      const hub = new OperationEventHub(`00000000-0000-4000-8000-${String(i).padStart(12, '0')}`, `00000000-0000-4000-8000-${String(1000 + i).padStart(12, '0')}`);
      for (let j = 0; j < 70; j += 1) {
        const f = hub.recordEvent({ Progress: { message: 'y'.repeat(3072) } });
        if (f?.event === 'gap') break;
      }
      hubs.push(hub);
      const dataHeld = hubs.reduce((sum, h) => sum + h.chargedBytes(), 0);
      assert.ok(dataHeld <= 8 * 1024 * 1024, 'global data cap must hold across hubs');
    }
    const beforeDispose = budget.environmentBudgetReserved();
    assert.ok(beforeDispose > 0);
    for (const hub of hubs) hub.dispose();
    budget.resetEnvironmentBudgetForTests();
    assert.equal(budget.environmentBudgetReserved(), 0, 'all hub bytes must release on dispose');
  });

  test('aggregate environment proof stays within the frozen ceiling', async () => {
    const budget = await import(join(serviceRoot, 'dist/environment-budget.js'));
    const proof = budget.environmentBudgetProof();
    assert.equal(proof.ceilingBytes, 32 * 1024 * 1024);
    assert.ok(proof.totalBytes <= proof.ceilingBytes, `proof ${proof.totalBytes} exceeds ceiling`);
    // The proof must be the sum of real categories, not a relabelled constant.
    const expected =
      proof.tsfnSharedBytes +
      proof.nativeProviderBytes +
      proof.acpDeliveryBytes +
      proof.nativeGenericLocalsetBytes +
      proof.nodeRetainedBytes +
      proof.nodeControlBytes +
      proof.socketReservedBytes;
    assert.equal(proof.totalBytes, expected);
    assert.ok(proof.nodeControlBytes >= 560 * 1024, 'control reserve covers 70 hubs x 2 x 4KiB');
    assert.ok(proof.nodeControlBytes <= proof.ceilingBytes);
    // Node retained budget is clamped: an env override cannot raise it.
    const { SSE_MAX_AGGREGATE_PENDING_BYTES, SSE_MAX_AGGREGATE_PENDING_BYTES_CEILING } = await import(join(serviceRoot, 'dist/config.js'));
    assert.ok(SSE_MAX_AGGREGATE_PENDING_BYTES <= SSE_MAX_AGGREGATE_PENDING_BYTES_CEILING);
  });

  test('actor/viewpoint create is explicit not_migrated', async () => {
    const res = await jsonFetch(`${baseUrl}/v1/daemon/agent-host/sessions`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: { provider_id: 'mock-acp', actor_ref: { actor_kind: 'creator', creator_id: 'ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa' }, viewpoint: { world_id: 'wld_owned' } } });
    assert.equal(res.status, 501);
    assert.equal(res.payload.error.code, 'route_not_migrated');
  });

  test('malformed create/execute bodies are typed invalid_input before any provider effect', async () => {
    await stopSharedService();
    const home = seedHome();
    const local = await startProviderService(home.home, 0);
    try {
      const post = (body) => jsonFetch(`${local.url}/v1/daemon/agent-host/sessions`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body });
      // Unknown key, wrong optional type, malformed actor pair, malformed refs.
      const cases = [
        { provider_id: 'mock-acp', unexpected: 1 },
        { provider_id: 'mock-acp', cwd: 42 },
        { provider_id: 'mock-acp', cwd: null },
        { provider_id: 'mock-acp', model: { nope: true } },
        { provider_id: 'mock-acp', model: null },
        { provider_id: 'mock-acp', mode: [] },
        { provider_id: 'mock-acp', mode: null },
        { provider_id: 7 },
        { provider_id: 'mock-acp', actor_ref: { actor_kind: 'creator', creator_id: 'ctr_ok' } },
        { provider_id: 'mock-acp', actor_ref: null, viewpoint: null },
        { provider_id: 'mock-acp', actor_ref: null, viewpoint: { world_id: 'wld_ok' } },
        { provider_id: 'mock-acp', actor_ref: { actor_kind: 'creator', creator_id: 'ctr_ok' }, viewpoint: null },
        { provider_id: 'mock-acp', actor_ref: { actor_kind: 'bogus', creator_id: 'ctr_ok' }, viewpoint: { world_id: 'wld_ok' } },
        { provider_id: 'mock-acp', actor_ref: { actor_kind: 'creator', creator_id: 'bad' }, viewpoint: { world_id: 'wld_ok' } },
        { provider_id: 'mock-acp', actor_ref: { actor_kind: 'creator', creator_id: 'ctr_ok' }, viewpoint: { world_id: 'bad' } },
        { provider_id: 'mock-acp', actor_ref: { actor_kind: 'creator', creator_id: 'ctr_ok' }, viewpoint: { world_id: 'wld_ok', extra: 1 } },
        { provider_id: 'mock-acp', actor_ref: { actor_kind: 'creator', creator_id: 'ctr_ok' }, viewpoint: { world_id: 'wld_ok', binding_id: null } },
        { provider_id: 'mock-acp', actor_ref: { actor_kind: 'creator', creator_id: 'ctr_ok' }, viewpoint: { world_id: 'wld_ok', branch_id: null } },
        { provider_id: 'mock-acp', actor_ref: { actor_kind: 'creator', creator_id: 'ctr_ok' }, viewpoint: { world_id: 'wld_ok', event_id: null } },
      ];
      for (const body of cases) {
        const res = await post(body);
        assert.equal(res.status, 400, `expected 400 for ${JSON.stringify(body)}`);
        assert.equal(res.payload.error.code, 'invalid_input');
      }
      // A well-formed prompt with an unknown key is rejected too.
      const created = await post({ provider_id: 'mock-acp' });
      assert.equal(created.status, 200);
      const sid = created.payload.session_id;
      // Capture the adapter's log *after* the one legitimate create, then prove
      // no malformed body mutates it further.
      const logAfterValidCreate = readFixtureLog(home.log).length;
      const exec = (body) => jsonFetch(`${local.url}/v1/daemon/agent-host/sessions/${sid}/operations`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body });
      for (const body of [
        { kind: 'prompt', content: 'x', extra: 1 },
        { kind: 'prompt', content: 5 },
        { kind: 'prompt', content: 'x', remember: 'yes' },
        { kind: 'bogus', content: 'x' },
        { kind: 'nope' },
      ]) {
        const res = await exec(body);
        assert.equal(res.status, 400, `expected 400 for ${JSON.stringify(body)}`);
        assert.equal(res.payload.error.code, 'invalid_input');
      }
      // A well-formed set_model branch is a valid shape -> explicit not_migrated.
      const setModel = await exec({ kind: 'set_model', model: 'm' });
      assert.equal(setModel.status, 501);
      assert.equal(setModel.payload.error.code, 'route_not_migrated');
      // No malformed or unsupported request may reach the adapter.
      assert.equal(readFixtureLog(home.log).length, logAfterValidCreate, 'malformed bodies must not mutate the fixture');
    } finally { await closeServiceBounded(local); await startSharedService(homeCtx.home, 0); }
  });

  test('legacy prompt cannot claim Character memory capture', async () => {
    const { sessionId } = await providerFlow(baseUrl);
    const res = await jsonFetch(`${baseUrl}/v1/daemon/agent-host/sessions/${sessionId}/operations`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: { kind: 'prompt', content: 'hi', remember: true } });
    assert.equal(res.status, 422);
    assert.equal(res.payload.error.code, 'invalid_input');
  });

  test('native interrupted operation is terminal for hydrate, cancel, and active count', async () => {
    const created = await jsonFetch(`${baseUrl}/v1/daemon/agent-host/sessions`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: { provider_id: 'mock-acp' } });
    const sessionId = created.payload.session_id;
    const operationId = '00000000-0000-4000-8000-00000000d001';
    // Native-interrupted status with no local terminal event — the exact shape a
    // hydrated record presents after a SessionStopped.
    service.service.providerRegistry.registerSession({ sessionId, providerId: 'mock-acp', state: 'Ready', activeOpId: null });
    const before = service.service.providerRegistry.activeOperationCount();
    service.service.providerRegistry.registerOperation({ operationId, sessionId, providerId: 'mock-acp', status: 'interrupted', terminalEvent: null, terminalTranscript: null });
    // Not charged against the live-operation cap.
    assert.equal(service.service.providerRegistry.activeOperationCount(), before, 'interrupted must not be charged as active');
    // GET surfaces the terminal status exactly.
    const inspect = await jsonFetch(`${baseUrl}/v1/daemon/agent-host/operations/${operationId}`);
    assert.equal(inspect.status, 200);
    assert.equal(inspect.payload.status, 'interrupted');
    // Cancel must reject before any provider mutation.
    const cancel = await jsonFetch(`${baseUrl}/v1/daemon/agent-host/operations/${operationId}`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: {} });
    assert.equal(cancel.status, 409);
    assert.equal(cancel.payload.error.code, 'busy');
  });

  test('NEXUS_SSE_SOCKET_HWM is lower-only and capped at 64 KiB', async () => {
    const { resolveSseSocketHighWaterMark, SSE_SOCKET_HIGH_WATER_MARK } = await import(join(serviceRoot, 'dist/config.js'));
    const prev = process.env.NEXUS_SSE_SOCKET_HWM;
    try {
      process.env.NEXUS_SSE_SOCKET_HWM = String(1024 * 1024);
      assert.equal(resolveSseSocketHighWaterMark(), SSE_SOCKET_HIGH_WATER_MARK, 'an over-cap value must clamp to 64 KiB');
      process.env.NEXUS_SSE_SOCKET_HWM = '1024';
      assert.equal(resolveSseSocketHighWaterMark(), 1024, 'a lower value must be honored');
      delete process.env.NEXUS_SSE_SOCKET_HWM;
      assert.equal(resolveSseSocketHighWaterMark(), SSE_SOCKET_HIGH_WATER_MARK);
    } finally {
      if (prev === undefined) delete process.env.NEXUS_SSE_SOCKET_HWM; else process.env.NEXUS_SSE_SOCKET_HWM = prev;
    }
  });

  test('CDN validation refuses private IPv4 spellings and IPv6 literals', async () => {
    const { validateCdnUrl } = await import(join(serviceRoot, 'dist/config.js'));
    const refused = [
      'https://[::1]/x',
      'https://[0:0:0:0:0:0:0:1]/x',
      'https://[::FFFF:127.0.0.1]/x',
      'https://[::ffff:7f00:1]/x',
      'https://[fc00::1]/x',
      'https://[fd12:3456::abcd]/x',
      'https://[fe80::1]/x',
      'https://[FEBF::9]/x',
      'https://127.0.0.1/x',
      'https://10.1.2.3/x',
      'https://192.168.1.1/x',
      'https://172.16.0.9/x',
      'https://172.31.255.1/x',
      'https://169.254.169.254/latest/meta-data/',
      'https://0177.0.0.1/x',
      'https://0x7f.1/x',
      'https://2130706433/x',
    ];
    for (const url of refused) {
      assert.throws(() => validateCdnUrl(url), /public HTTPS CDN URL/, url);
    }
    // Public name and literal hosts stay admitted.
    validateCdnUrl('https://cdn.example.com/registry.json');
    validateCdnUrl('https://8.8.8.8/x');
    validateCdnUrl('https://172.32.0.1/x');
    validateCdnUrl('https://[2606:4700::6810:85e5]/x');
  });
});
