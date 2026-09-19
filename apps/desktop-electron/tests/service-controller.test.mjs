#!/usr/bin/env node
/**
 * P0-T4 controller tests.
 *
 * The controller drives a real HTTP service boundary (an in-process stub that
 * answers the guarded discovery/health/stop routes exactly as the standalone
 * service does) and a deterministic stub utility child (no process fork, no
 * native binding, no E2E). The binding itself is covered by P0-T3R's own tests
 * and the service-side routes by `apps/nexus-service/tests/runtime-discovery.test.mjs`;
 * here the child is stubbed at the controller boundary.
 */
import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import http from 'node:http';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test, { after } from 'node:test';
import { DESKTOP_STATUS_CHANNEL, assertDesktopStatusFrame } from '../dist/desktop-contract.js';
import { DESKTOP_SERVICE_PORT, buildDesktopServiceOptions, resolveDesktopServicePort } from '../dist/env.js';
import { DesktopServiceController, SERVICE_CLOSE_BUDGET_MS } from '../dist/service-controller.js';

const tempDirs = [];
after(() => {
  for (const dir of tempDirs) rmSync(dir, { recursive: true, force: true });
});

function tempHome() {
  const home = mkdtempSync(join(tmpdir(), 'nexus-controller-'));
  tempDirs.push(home);
  return home;
}

function recordPath(home) {
  return join(home, '.nexus42', 'run', 'service.json');
}

function writeRecord(home, record) {
  mkdirSync(dirname(recordPath(home)), { recursive: true, mode: 0o700 });
  writeFileSync(recordPath(home), JSON.stringify(record), { mode: 0o600 });
}

function makeRecord({ home, origin, instanceId, readiness = 'uninitialized', engineEpoch = null }) {
  const ready = readiness === 'ready';
  return {
    schema_version: 1,
    instance_id: instanceId,
    pid: process.pid,
    user_home: home,
    creator_id: ready ? 'ctr_local_test' : null,
    workspace_slug: ready ? 'default' : null,
    engine_epoch: ready ? (engineEpoch ?? 1) : null,
    endpoint: { transport: 'http', url: origin },
    tls_fingerprint: null,
    readiness,
    protocol_version: 1,
  };
}

/** In-process service boundary: guarded discovery, unguarded health, operator stop. */
async function startStubService() {
  const requests = [];
  let record = null;
  let requireKey = null;
  let stopping = false;
  let port = 0;
  let teardown = null;
  let stopMode = 'release';
  const server = http.createServer((req, res) => {
    const chunks = [];
    req.on('data', (chunk) => chunks.push(chunk));
    req.on('end', () => {
      const body = chunks.length > 0 ? JSON.parse(Buffer.concat(chunks).toString('utf8')) : null;
      requests.push({ method: req.method, url: req.url, key: req.headers['x-api-key'], body });
      const send = (status, payload) => {
        res.writeHead(status, { 'content-type': 'application/json' });
        res.end(JSON.stringify(payload));
      };
      if (requireKey && req.headers['x-api-key'] !== requireKey) {
        send(401, { error: { code: 'auth_required', message: 'Authentication required' } });
        return;
      }
      if (req.method === 'GET' && req.url === '/v1/daemon/runtime/discovery') {
        if (!record) {
          send(503, { error: { code: 'busy', message: 'no discovery record' } });
          return;
        }
        send(200, record);
        return;
      }
      if (req.method === 'GET' && req.url === '/v1/daemon/runtime/health') {
        send(200, { status: 'ok', version: '0.1.0' });
        return;
      }
      if (req.method === 'POST' && req.url === '/v1/daemon/runtime/stop') {
        send(200, { status: 'stopping' });
        if (stopMode === 'release') setImmediate(() => void close());
        return;
      }
      send(404, { error: { code: 'not_found', message: 'not found' } });
    });
  });

  function close() {
    if (teardown) return teardown;
    stopping = true;
    server.closeAllConnections();
    teardown = new Promise((resolve) => server.close(() => resolve()));
    return teardown;
  }

  async function listen() {
    if (teardown) {
      const pending = teardown;
      teardown = null;
      await pending;
    }
    if (!server.listening) {
      await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
      port = server.address().port;
    }
    stopping = false;
    return `http://127.0.0.1:${port}`;
  }

  await listen();
  return {
    requests,
    get url() {
      return `http://127.0.0.1:${port}`;
    },
    get port() {
      return port;
    },
    get stopping() {
      return stopping;
    },
    get record() {
      return record;
    },
    setRecord(next) {
      record = next;
    },
    setRequireKey(key) {
      requireKey = key;
    },
    setStopMode(mode) {
      stopMode = mode;
    },
    listen,
    close,
  };
}

class StubUtility extends EventEmitter {
  constructor(onRequest) {
    super();
    this.pid = 9000;
    this.requests = [];
    this.kills = 0;
    this.onRequest = onRequest;
  }

  postMessage(message) {
    this.requests.push(message);
    queueMicrotask(() => {
      this.onRequest(this, message);
    });
  }

  kill() {
    this.kills += 1;
    return true;
  }
}

function reply(child, message, ok, payload) {
  child.emit('message', {
    generation: message.generation,
    request_id: message.request_id,
    ok,
    ...(ok ? { result: payload ?? null } : { error: payload }),
  });
}

/**
 * Utility behaviour mirroring `startService`: publish the record, serve, close
 * the handle, reset. Replies are emitted on the request's own microtask turn;
 * listener/teardown work that needs real I/O runs behind it, exactly as a real
 * process boundary would.
 */
function healthyUtility({ home, service, counter }) {
  return (child, message) => {
    if (message.operation === 'start') {
      counter.n += 1;
      const instanceId = `inst-${counter.n}`;
      void (async () => {
        const origin = await service.listen();
        const record = makeRecord({ home, origin, instanceId });
        service.setRecord(record);
        writeRecord(home, record);
        child.emit('message', { type: 'service-ready', generation: message.generation, discovery: record });
        reply(child, message, true, { discovery: record });
      })();
      return;
    }
    if (message.operation === 'close') {
      service.setRecord(null);
      rmSync(recordPath(home), { force: true });
      void service.close();
      reply(child, message, true, { state: 'closed', cleanup_confirmed: true, pending_operations: [] });
      return;
    }
    if (message.operation === 'reset-local-state') {
      reply(child, message, true, { removed: 2 });
    }
  };
}

/** Time seam: sleeps advance a virtual clock; `holdNext` parks one delay for cancellation tests. */
function testClock() {
  const sleeps = [];
  const parked = [];
  let now = 0;
  let heldMs = null;
  return {
    now: () => now,
    sleeps,
    holdNext(ms) {
      heldMs = ms;
    },
    release() {
      for (const waiter of parked.splice(0)) waiter.resolve();
    },
    pendingSleeps: () => parked.length,
    sleep(ms) {
      sleeps.push(ms);
      const { promise, resolve } = Promise.withResolvers();
      if (heldMs !== null && ms === heldMs) {
        heldMs = null;
        parked.push({ resolve });
      } else {
        now += ms;
        // One virtual tick per macrotask: the timeout cannot overtake a reply
        // that is already queued, and real I/O still progresses.
        setImmediate(() => resolve());
      }
      return promise;
    },
  };
}

async function waitForStatus(controller, state, ticks = 256) {
  for (let i = 0; i < ticks; i += 1) {
    if (controller.getStatus().state === state) return true;
    await new Promise((resolve) => setImmediate(resolve));
  }
  return false;
}

async function waitForDetail(controller, pattern, ticks = 256) {
  for (let i = 0; i < ticks; i += 1) {
    const detail = controller.getStatus().detail;
    if (typeof detail === 'string' && pattern.test(detail)) return true;
    await new Promise((resolve) => setImmediate(resolve));
  }
  return false;
}

function statuses() {
  const frames = [];
  return {
    frames,
    emit: (channel, status) => frames.push({ channel, status }),
    last: () => frames[frames.length - 1]?.status,
  };
}

function makeController({ home, port, spawnUtility, clock, status, apiKey, handoff }) {
  return new DesktopServiceController({
    home,
    resolvedPort: port,
    ...(apiKey === undefined ? {} : { apiKey }),
    spawnUtility,
    ...(clock === undefined ? {} : { clock }),
    ...(status === undefined ? {} : { emitStatus: status.emit }),
    ...(handoff === undefined ? {} : { handoff }),
  });
}

function spawnRecorder(behaviour) {
  const children = [];
  return {
    children,
    spawn: () => {
      const child = new StubUtility(behaviour);
      children.push(child);
      return child;
    },
  };
}

test('port resolution: explicit → NEXUS_DAEMON_PORT → 8420, invalid explicit is an error', () => {
  assert.equal(DESKTOP_SERVICE_PORT, 8420);
  assert.equal(resolveDesktopServicePort(undefined, {}), 8420);
  assert.equal(resolveDesktopServicePort(9000, {}), 9000);
  assert.equal(resolveDesktopServicePort(undefined, { NEXUS_DAEMON_PORT: '9100' }), 9100);
  assert.equal(resolveDesktopServicePort(undefined, { NEXUS_DAEMON_PORT: 'not-a-port' }), 8420);
  assert.equal(resolveDesktopServicePort(undefined, { NEXUS_DAEMON_PORT: '70000' }), 8420);
  assert.throws(() => resolveDesktopServicePort(0), /invalid desktop service port/);
  assert.throws(() => resolveDesktopServicePort(65_536), /invalid desktop service port/);
  assert.deepEqual(buildDesktopServiceOptions({ home: '/tmp/home', port: 8420 }), {
    home: '/tmp/home',
    host: '127.0.0.1',
    port: 8420,
    allowRemote: false,
    domainOnly: false,
  });
});

test('start spawns one owner and reports running from authenticated discovery + health', async () => {
  const home = tempHome();
  const service = await startStubService();
  const clock = testClock();
  const status = statuses();
  const counter = { n: 0 };
  const recorded = spawnRecorder(healthyUtility({ home, service, counter }));
  const controller = makeController({
    home,
    port: 18_420,
    spawnUtility: recorded.spawn,
    clock,
    status,
  });

  const initial = controller.getStatus();
  assert.deepEqual(initial, { state: 'stopped', port: 18_420 });

  await controller.start();
  const running = controller.getStatus();
  assert.equal(running.state, 'running');
  assert.equal(running.port, service.port);
  assert.equal(running.version, '0.1.0');
  assert.deepEqual(Object.keys(running).sort(), ['port', 'state', 'version']);
  assert.equal(JSON.stringify(running).includes('inst-'), false, 'status never leaks the instance id');
  assert.equal(JSON.stringify(running).includes(home), false, 'status never leaks home');

  assert.equal(recorded.children.length, 1);
  const startRequest = recorded.children[0].requests[0];
  assert.equal(startRequest.operation, 'start');
  assert.deepEqual(startRequest.payload, {
    home,
    host: '127.0.0.1',
    port: 18_420,
    allowRemote: false,
    domainOnly: false,
  });

  // Every emitted frame is the frozen status shape, on the frozen channel.
  for (const frame of status.frames) {
    assert.equal(frame.channel, DESKTOP_STATUS_CHANNEL);
    assertDesktopStatusFrame(frame.status);
  }
  assert.deepEqual(
    status.frames.map((frame) => frame.status.state),
    ['starting', 'running'],
  );

  await service.close();
});

test('a clean uninitialized home is ready, not bootstrap-gated', async () => {
  const home = tempHome();
  const service = await startStubService();
  const clock = testClock();
  const counter = { n: 0 };
  const recorded = spawnRecorder(healthyUtility({ home, service, counter }));
  const controller = makeController({ home, port: 18_421, spawnUtility: recorded.spawn, clock });

  await controller.start();
  const record = service.record;
  assert.equal(record.readiness, 'uninitialized');
  assert.equal(record.creator_id, null);
  assert.equal(record.engine_epoch, null);
  assert.equal(controller.getStatus().state, 'running');

  await service.close();
});

test('attach: a matching independent owner is adopted without spawning', async () => {
  const home = tempHome();
  const service = await startStubService();
  const record = makeRecord({ home, origin: service.url, instanceId: 'inst-independent', readiness: 'ready' });
  service.setRecord(record);
  writeRecord(home, record);
  const clock = testClock();
  const recorded = spawnRecorder(() => {});
  const controller = makeController({
    home,
    port: service.port,
    spawnUtility: recorded.spawn,
    clock,
  });

  await controller.start();
  const status = controller.getStatus();
  assert.equal(status.state, 'running');
  assert.equal(status.port, service.port);
  assert.equal(status.detail, 'attached to an independent service');
  assert.equal(recorded.children.length, 0, 'no second engine owner is spawned');

  await service.close();
});

test('a stale record never attaches: identity must match the live listener', async () => {
  const home = tempHome();
  const service = await startStubService();
  // The published record names one instance; the listener answers as another.
  const stale = makeRecord({ home, origin: service.url, instanceId: 'inst-replaced' });
  writeRecord(home, stale);
  service.setRecord(makeRecord({ home, origin: service.url, instanceId: 'inst-live' }));
  const clock = testClock();
  const recorded = spawnRecorder(() => {});
  const controller = makeController({
    home,
    port: service.port,
    spawnUtility: recorded.spawn,
    clock,
  });

  await assert.rejects(controller.start(), /does not provide matching authenticated service discovery/);
  assert.equal(controller.getStatus().state, 'error');
  assert.equal(recorded.children.length, 0, 'a healthy unrelated listener is never adopted');
  assert.equal(service.requests.some((entry) => entry.url === '/v1/daemon/runtime/stop'), false);

  await service.close();
});

test('a legacy listener without matching discovery is a conflict, never killed (D-7)', async () => {
  const home = tempHome();
  const service = await startStubService();
  service.setRecord(makeRecord({ home, origin: service.url, instanceId: 'inst-other' }));
  // No record for this home: nothing identifies the listener as our service.
  const clock = testClock();
  const recorded = spawnRecorder(() => {});
  const controller = makeController({
    home,
    port: service.port,
    spawnUtility: recorded.spawn,
    clock,
  });

  await assert.rejects(controller.start(), /a listener on 127\.0\.0\.1:\d+ does not provide matching/);
  assert.equal(controller.getStatus().state, 'error');
  assert.equal(recorded.children.length, 0);
  assert.equal(service.stopping, false, 'the unidentified listener is never signalled');

  await service.close();
});

test('the guarded probes use the configured API key; a keyed service without it conflicts', async () => {
  const home = tempHome();
  const service = await startStubService();
  const record = makeRecord({ home, origin: service.url, instanceId: 'inst-keyed' });
  service.setRecord(record);
  writeRecord(home, record);
  service.setRequireKey('desktop-secret');

  const clock = testClock();
  const keyed = spawnRecorder(() => {});
  const withKey = makeController({
    home,
    port: service.port,
    spawnUtility: keyed.spawn,
    clock,
    apiKey: 'desktop-secret',
  });
  await withKey.start();
  assert.equal(withKey.getStatus().state, 'running');
  assert.equal(
    service.requests.filter((entry) => entry.key === 'desktop-secret').length >= 2,
    true,
    'discovery and health are probed with the key',
  );

  const keyless = makeController({ home, port: service.port, spawnUtility: keyed.spawn, clock });
  await assert.rejects(keyless.start(), /does not provide matching authenticated service discovery/);

  await service.close();
});

test('stop closes the owned service cooperatively and the retained owner restarts', async () => {
  const home = tempHome();
  const service = await startStubService();
  const clock = testClock();
  const counter = { n: 0 };
  const recorded = spawnRecorder(healthyUtility({ home, service, counter }));
  const controller = makeController({ home, port: 18_422, spawnUtility: recorded.spawn, clock });

  await controller.start();
  assert.equal(controller.getStatus().state, 'running');
  await controller.stop();
  assert.equal(controller.getStatus().state, 'stopped');
  const child = recorded.children[0];
  assert.deepEqual(
    child.requests.map((request) => request.operation),
    ['start', 'close'],
  );

  // A confirmed close releases the record, so the next start is a real start.
  await controller.start();
  assert.equal(controller.getStatus().state, 'running');
  assert.equal(child.requests.filter((request) => request.operation === 'start').length, 2);
  assert.equal(recorded.children.length, 1, 'the app-managed owner process is reused');

  await controller.stop();
  await service.close();
});

test('an unconfirmed close is error/interrupted, blocks restart, and a retry recovers', async () => {
  const home = tempHome();
  const service = await startStubService();
  let closeAttempts = 0;
  const behaviour = (child, message) => {
    void (async () => {
      if (message.operation === 'start') {
        const record = makeRecord({ home, origin: service.url, instanceId: 'inst-unconfirmed' });
        service.setRecord(record);
        writeRecord(home, record);
        child.emit('message', { type: 'service-ready', generation: message.generation, discovery: record });
        reply(child, message, true, { discovery: record });
        return;
      }
      if (message.operation === 'close') {
        closeAttempts += 1;
        if (closeAttempts === 1) {
          reply(child, message, true, { state: 'interrupted', cleanup_confirmed: false, pending_operations: ['op-1'] });
          return;
        }
        rmSync(recordPath(home), { force: true });
        service.setRecord(null);
        await service.close();
        reply(child, message, true, { state: 'closed', cleanup_confirmed: true, pending_operations: [] });
      }
    })();
  };
  const clock = testClock();
  const recorded = spawnRecorder(behaviour);
  const controller = makeController({ home, port: 18_423, spawnUtility: recorded.spawn, clock });

  await controller.start();
  await assert.rejects(controller.stop(), /cleanup_confirmed:false/);
  const failed = controller.getStatus();
  assert.equal(failed.state, 'error');
  assert.match(failed.detail, /blocks restart and quit/);
  await assert.rejects(controller.restart(), /retained service owner must be released/);
  await assert.rejects(controller.keepForQuit(), /retained service owner must be released/);

  await controller.stop();
  assert.equal(controller.getStatus().state, 'stopped');
  assert.equal(closeAttempts, 2);
});

test('a close that exceeds the 5s budget never reports success', async () => {
  const home = tempHome();
  const service = await startStubService();
  const behaviour = (child, message) => {
    if (message.operation === 'start') {
      const record = makeRecord({ home, origin: service.url, instanceId: 'inst-timeout' });
      service.setRecord(record);
      writeRecord(home, record);
      child.emit('message', { type: 'service-ready', generation: message.generation, discovery: record });
      reply(child, message, true, { discovery: record });
      return;
    }
    // close is never answered
  };
  const clock = testClock();
  const recorded = spawnRecorder(behaviour);
  const controller = makeController({ home, port: 18_424, spawnUtility: recorded.spawn, clock });

  await controller.start();
  await assert.rejects(controller.stop(), /close was not confirmed/);
  assert.equal(controller.getStatus().state, 'error');
  assert.equal(clock.sleeps.includes(SERVICE_CLOSE_BUDGET_MS), true);

  await service.close();
});

test('unexpected owner exits back off 500/1000/2000/4000/8000 and stop at exhaustion', async () => {
  const home = tempHome();
  const clock = testClock();
  const recorded = spawnRecorder((child) => {
    queueMicrotask(() => child.emit('exit', 7));
  });
  const controller = makeController({ home, port: 18_425, spawnUtility: recorded.spawn, clock });

  await assert.rejects(controller.start(), /exited/);
  // Let the recovery loop run to exhaustion.
  for (let i = 0; i < 64; i += 1) await new Promise((resolve) => setImmediate(resolve));

  const backoff = clock.sleeps.filter((ms) => [500, 1_000, 2_000, 4_000, 8_000].includes(ms));
  assert.deepEqual(backoff, [500, 1_000, 2_000, 4_000, 8_000]);
  assert.equal(recorded.children.length, 6, 'the failed start plus five recovery attempts');
  const exhausted = controller.getStatus();
  assert.equal(exhausted.state, 'stopped');
  assert.match(exhausted.detail, /start the service manually/);
});

test('an unexpected exit recovers automatically, and a manual start recovers after exhaustion', async () => {
  const home = tempHome();
  const service = await startStubService();
  const clock = testClock();
  const counter = { n: 0 };
  const healthy = healthyUtility({ home, service, counter });
  let attempts = 0;
  const behaviour = {
    current: (child, message) => {
      attempts += 1;
      if (attempts <= 1) {
        queueMicrotask(() => child.emit('exit', 3));
        return;
      }
      healthy(child, message);
    },
  };
  const recorded = spawnRecorder((child, message) => behaviour.current(child, message));
  const controller = makeController({ home, port: 18_426, spawnUtility: recorded.spawn, clock });

  // First attempt dies; the automatic restart (attempt 1) comes up healthy.
  await assert.rejects(controller.start(), /exited/);
  assert.equal(await waitForStatus(controller, 'running'), true, 'automatic recovery reaches running');
  assert.equal(recorded.children.length, 2);

  // Exhaustion: every attempt fails, so the schedule ends in stopped.
  await controller.stop();
  behaviour.current = (child) => queueMicrotask(() => child.emit('exit', 5));
  await assert.rejects(controller.start(), /exited/);
  assert.equal(await waitForStatus(controller, 'stopped'), true, 'exhausted recovery stops the service');
  assert.match(controller.getStatus().detail, /start the service manually/);

  // Manual start resets the budget and recovers for real.
  behaviour.current = healthy;
  await controller.start();
  assert.equal(controller.getStatus().state, 'running');
  await service.close();
});

test('stop during backoff cancels the pending restart', async () => {
  const home = tempHome();
  const clock = testClock();
  const recorded = spawnRecorder((child) => {
    queueMicrotask(() => child.emit('exit', 9));
  });
  const controller = makeController({ home, port: 18_427, spawnUtility: recorded.spawn, clock });
  clock.holdNext(500);

  await assert.rejects(controller.start(), /exited/);
  for (let i = 0; i < 8; i += 1) await new Promise((resolve) => setImmediate(resolve));
  assert.equal(controller.getStatus().state, 'degraded');
  assert.equal(clock.pendingSleeps(), 1, 'the first backoff delay is parked');
  assert.equal(recorded.children.length, 1);

  await controller.stop();
  assert.equal(controller.getStatus().state, 'stopped');
  clock.release();
  for (let i = 0; i < 8; i += 1) await new Promise((resolve) => setImmediate(resolve));
  assert.equal(recorded.children.length, 1, 'no restart after an intentional stop');
});

test('restart is single-flight and stops an attached service by instance/epoch, not by pid', async () => {
  const home = tempHome();
  const service = await startStubService();
  const record = makeRecord({ home, origin: service.url, instanceId: 'inst-restart', engineEpoch: 4, readiness: 'ready' });
  service.setRecord(record);
  writeRecord(home, record);
  const clock = testClock();
  const counter = { n: 0 };
  const recorded = spawnRecorder(healthyUtility({ home, service, counter }));
  const controller = makeController({ home, port: service.port, spawnUtility: recorded.spawn, clock });

  await controller.start();
  assert.equal(controller.getStatus().detail, 'attached to an independent service');

  const first = controller.restart();
  const second = controller.restart();
  assert.equal(first, second, 'concurrent callers join the one restart');
  await first;

  const stops = service.requests.filter((entry) => entry.url === '/v1/daemon/runtime/stop');
  assert.equal(stops.length, 1);
  assert.deepEqual(stops[0].body, { expected_instance_id: 'inst-restart', expected_engine_epoch: 4 });
  assert.equal(controller.getStatus().state, 'running');
  assert.equal(recorded.children.length, 1, 'the restarted service is now app-owned');
  assert.notEqual(service.record.instance_id, 'inst-restart', 'a replacement instance is started');

  await controller.stop();
  await service.close();
});

test('an attached service that acknowledges without releasing keeps the fence', async () => {
  const home = tempHome();
  const service = await startStubService();
  const record = makeRecord({ home, origin: service.url, instanceId: 'inst-stubborn', readiness: 'ready' });
  service.setRecord(record);
  writeRecord(home, record);
  const clock = testClock();
  const recorded = spawnRecorder(() => {});
  const controller = makeController({ home, port: service.port, spawnUtility: recorded.spawn, clock });

  await controller.start();
  service.setStopMode('ack_only');
  await assert.rejects(controller.restart(), /did not release/);
  assert.equal(controller.getStatus().state, 'error');
  // A running independent owner is retained: no second owner may be spawned.
  await assert.rejects(controller.start(), /retained service owner must be released/);
  assert.equal(recorded.children.length, 0);

  service.setStopMode('release');
  await controller.stop({ explicit: true });
  assert.equal(controller.getStatus().state, 'stopped');

  await service.close();
});

test('an owner lost after an unconfirmed close keeps the interrupted diagnostic', async () => {
  const home = tempHome();
  const service = await startStubService();
  const behaviour = (child, message) => {
    if (message.operation === 'start') {
      const record = makeRecord({ home, origin: service.url, instanceId: 'inst-lost' });
      service.setRecord(record);
      writeRecord(home, record);
      child.emit('message', { type: 'service-ready', generation: message.generation, discovery: record });
      reply(child, message, true, { discovery: record });
      return;
    }
    if (message.operation === 'close') {
      // Unconfirmed close, then the whole owner process dies with its service.
      service.setRecord(null);
      rmSync(recordPath(home), { force: true });
      void service.close();
      reply(child, message, true, { state: 'interrupted', cleanup_confirmed: false, pending_operations: [] });
      queueMicrotask(() => child.emit('exit', 11));
    }
  };
  const clock = testClock();
  const recorded = spawnRecorder(behaviour);
  const controller = makeController({ home, port: 18_433, spawnUtility: recorded.spawn, clock });

  await controller.start();
  await assert.rejects(controller.stop(), /blocks restart and quit/);
  assert.equal(
    await waitForDetail(controller, /interrupted cleanup is diagnostic only/),
    true,
    'the lost owner keeps an interrupted diagnostic',
  );

  // The owner is gone, so nothing is retained: a later start is attempted for
  // real instead of being refused by the retained-close fence.
  const failure = await controller.start().then(
    () => null,
    (error) => error,
  );
  assert.notEqual(failure, null);
  assert.doesNotMatch(failure.message, /retained service owner must be released/);

  await service.close();
});

test('resetLocalState closes, resets through the trusted owner, then recovers the service', async () => {
  const home = tempHome();
  const service = await startStubService();
  const clock = testClock();
  const counter = { n: 0 };
  const recorded = spawnRecorder(healthyUtility({ home, service, counter }));
  const controller = makeController({ home, port: 18_428, spawnUtility: recorded.spawn, clock });

  await controller.start();
  await controller.resetLocalState();
  assert.equal(controller.getStatus().state, 'running');
  const child = recorded.children[0];
  assert.deepEqual(
    child.requests.map((request) => request.operation),
    ['start', 'close', 'reset-local-state', 'start'],
  );
  assert.deepEqual(child.requests[2].payload, { home });
  assert.equal(recorded.children.length, 1);

  await controller.stop();
  await service.close();
});

test('a failed reset shows the concrete error and never claims recovery', async () => {
  const home = tempHome();
  const service = await startStubService();
  const behaviour = (child, message) => {
    void (async () => {
      if (message.operation === 'start') {
        const record = makeRecord({ home, origin: service.url, instanceId: `inst-${child.requests.length}` });
        service.setRecord(record);
        writeRecord(home, record);
        child.emit('message', { type: 'service-ready', generation: message.generation, discovery: record });
        reply(child, message, true, { discovery: record });
        return;
      }
      if (message.operation === 'close') {
        rmSync(recordPath(home), { force: true });
        await service.close();
        reply(child, message, true, { state: 'closed', cleanup_confirmed: true, pending_operations: [] });
        return;
      }
      if (message.operation === 'reset-local-state') {
        reply(child, message, false, { code: 'owner_busy', message: 'a live writer holds the store fence' });
      }
    })();
  };
  const clock = testClock();
  const recorded = spawnRecorder(behaviour);
  const controller = makeController({ home, port: 18_429, spawnUtility: recorded.spawn, clock });

  await controller.start();
  await assert.rejects(controller.resetLocalState(), /owner_busy|live writer/);
  const status = controller.getStatus();
  assert.equal(status.state, 'stopped', 'no service is claimed running after a failed reset');
  assert.equal(
    recorded.children[0].requests.filter((request) => request.operation === 'start').length,
    1,
    'no restart is attempted after a failed reset',
  );
});

test('keepForQuit hands an owned service to an independent owner after a confirmed close', async () => {
  const home = tempHome();
  const service = await startStubService();
  const clock = testClock();
  const counter = { n: 0 };
  const recorded = spawnRecorder(healthyUtility({ home, service, counter }));
  const handoff = {
    prepared: 0,
    started: 0,
    prepare() {
      this.prepared += 1;
      return Promise.resolve();
    },
    async start({ home: handoffHome }) {
      this.started += 1;
      const origin = await service.listen();
      const record = makeRecord({
        home: handoffHome,
        origin,
        instanceId: 'inst-detached',
        readiness: 'ready',
      });
      service.setRecord(record);
      writeRecord(handoffHome, record);
    },
  };
  const controller = makeController({
    home,
    port: 18_430,
    spawnUtility: recorded.spawn,
    clock,
    handoff,
  });

  await controller.start();
  await controller.keepForQuit();
  assert.equal(handoff.prepared, 1);
  assert.equal(handoff.started, 1);
  const status = controller.getStatus();
  assert.equal(status.state, 'running');
  assert.equal(status.detail, 'independent service (detached handoff)');

  await service.close();
});

test('a failed handoff keeps the app-owned service instead of forking a second owner', async () => {
  const home = tempHome();
  const service = await startStubService();
  const clock = testClock();
  const counter = { n: 0 };
  const recorded = spawnRecorder(healthyUtility({ home, service, counter }));
  const handoff = {
    prepare: () => Promise.reject(new Error('Node >= 22.22 is not installed')),
    start: () => Promise.resolve(),
  };
  const controller = makeController({
    home,
    port: 18_431,
    spawnUtility: recorded.spawn,
    clock,
    handoff,
  });

  await controller.start();
  await assert.rejects(controller.keepForQuit(), /Node >= 22\.22/);
  const child = recorded.children[0];
  assert.deepEqual(
    child.requests.map((request) => request.operation),
    ['start'],
    'the service stays up when the handoff cannot proceed',
  );
  assert.equal(controller.getStatus().state, 'running');

  await controller.stop();
  await service.close();
});

test('subscribe registers first, receives the current snapshot and every transition', async () => {
  const home = tempHome();
  const service = await startStubService();
  const clock = testClock();
  const counter = { n: 0 };
  const recorded = spawnRecorder(healthyUtility({ home, service, counter }));
  const controller = makeController({ home, port: 18_432, spawnUtility: recorded.spawn, clock });

  const seen = [];
  const unsubscribe = controller.subscribe((status) => seen.push(status));
  assert.deepEqual(seen, [{ state: 'stopped', port: 18_432 }], 'the current snapshot arrives on subscribe');
  await controller.start();
  assert.deepEqual(
    seen.map((status) => status.state),
    ['stopped', 'starting', 'running'],
  );
  unsubscribe();
  await controller.stop();
  assert.equal(seen.length, 3, 'an unsubscribed listener stops receiving transitions');
  assertDesktopStatusFrame(seen[2]);

  await service.close();
});
