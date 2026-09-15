import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import {
  chmodSync,
  existsSync,
  lstatSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  statSync,
  symlinkSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import http from 'node:http';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { after, before, describe, test } from 'node:test';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = join(__dirname, '..', '..', '..');
const serviceRoot = join(__dirname, '..');

const SERVICE_CLOSE_TIMEOUT_MS = 8_000;
const tempDirs = [];

function track(dir) {
  tempDirs.push(dir);
  return dir;
}

function seedHome() {
  const home = track(mkdtempSync(join(tmpdir(), 'nexus-discovery-')));
  const seed = spawnSync(
    'cargo',
    ['run', '-q', '-p', 'nexus-core-node', '--bin', 'native-wire-fixture-seed', '--', home],
    { cwd: root, stdio: 'pipe', encoding: 'utf8' },
  );
  assert.equal(seed.status, 0, seed.stderr?.toString());
  return home;
}

function emptyHome() {
  // Exists (validateServiceHome requires it) but holds no profile, so the
  // service opens the explicit uninitialized shell.
  return track(mkdtempSync(join(tmpdir(), 'nexus-shell-')));
}

async function startServiceAt(home, extra = {}) {
  const { startService } = await import(join(serviceRoot, 'dist', 'index.js'));
  return startService({ home, host: '127.0.0.1', port: 0, allowRemote: false, ...extra });
}

async function closeServiceBounded(service) {
  return Promise.race([
    service.close(),
    new Promise((_, reject) =>
      setTimeout(() => reject(new Error('service.close timeout')), SERVICE_CLOSE_TIMEOUT_MS),
    ),
  ]);
}

/** `target` is an absolute URL for TCP requests, an absolute path for unix. */
function jsonRequest(target, { method = 'GET', body, socketPath, headers: extraHeaders } = {}) {
  return new Promise((resolveRequest, rejectRequest) => {
    const payload = body === undefined ? null : JSON.stringify(body);
    const headers = {
      ...extraHeaders,
      ...(payload === null
        ? {}
        : { 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(payload) }),
    };
    const onResponse = (response) => {
      const chunks = [];
      response.on('data', (chunk) => chunks.push(chunk));
      response.on('end', () => {
        const text = Buffer.concat(chunks).toString('utf8');
        resolveRequest({
          status: response.statusCode,
          payload: text.length > 0 ? JSON.parse(text) : null,
        });
      });
    };
    const request = socketPath
      ? http.request({ socketPath, path: target, method, headers }, onResponse)
      : http.request(target, { method, headers }, onResponse);
    request.on('error', rejectRequest);
    if (payload !== null) request.write(payload);
    request.end();
  });
}

const STOP_PATH = '/v1/daemon/runtime/stop';

/** Stop over the service's own transport: TCP URL, or path + unix socket. */
function stopWith(service, identity) {
  const target = service.url !== null ? `${service.url}${STOP_PATH}` : STOP_PATH;
  return jsonRequest(target, { method: 'POST', body: identity });
}

function stopOverSocket(socketPath, identity) {
  return jsonRequest(STOP_PATH, { method: 'POST', socketPath, body: identity });
}

function readRecord(home) {
  const path = join(home, '.nexus42', 'run', 'service.json');
  return existsSync(path) ? JSON.parse(readFileSync(path, 'utf8')) : null;
}

/** Poll until `predicate` holds or the budget expires (drain is async). */
async function waitFor(predicate, timeoutMs = 8_000, stepMs = 100) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (predicate()) return true;
    await new Promise((resolve) => setTimeout(resolve, stepMs));
  }
  return predicate();
}


describe('discovery-lifecycle (P5-T4)', () => {
  before(async () => {
    assert.equal(
      spawnSync('node', ['packages/nexus-native/scripts/build.mjs'], { cwd: root, stdio: 'inherit' })
        .status,
      0,
    );
    assert.equal(
      spawnSync('npx', ['tsc', '-p', 'tsconfig.json'], { cwd: serviceRoot, stdio: 'inherit' }).status,
      0,
    );
  });

  after(() => {
    for (const dir of tempDirs) rmSync(dir, { recursive: true, force: true });
  });

  test('stale discovery cannot stop replacement', async () => {
    const home = seedHome();
    const first = await startServiceAt(home);
    const staleRecord = first.discovery;
    assert.equal(staleRecord.readiness, 'ready');
    assert.ok(staleRecord.instance_id.length > 0);
    assert.equal(readRecord(home).instance_id, staleRecord.instance_id);

    // The confirmed owner removes its record; a replacement publishes its own.
    const report = await closeServiceBounded(first);
    assert.equal(report.cleanup_confirmed, true);
    assert.equal(readRecord(home), null, 'confirmed close removes the owned record');

    const replacement = await startServiceAt(home);
    const currentRecord = replacement.discovery;
    assert.notEqual(currentRecord.instance_id, staleRecord.instance_id, 'replacement mints a fresh instance');

    // Stale instance id: conflict, no stop, replacement record preserved.
    const staleStop = await stopWith(replacement, {
      expected_instance_id: staleRecord.instance_id,
      expected_engine_epoch: staleRecord.engine_epoch,
    });
    assert.equal(staleStop.status, 409);
    assert.equal(staleStop.payload.error.code, 'instance_conflict');
    assert.equal(readRecord(home).instance_id, currentRecord.instance_id, 'replacement record preserved');
    const health = await jsonRequest(`${replacement.url}/v1/daemon/runtime/health`);
    assert.equal(health.payload.status, 'ok', 'replacement still serves');

    // Current instance but wrong epoch: conflict again.
    const wrongEpoch = await stopWith(replacement, {
      expected_instance_id: currentRecord.instance_id,
      expected_engine_epoch: currentRecord.engine_epoch + 1,
    });
    assert.equal(wrongEpoch.status, 409);

    // Null epoch targets an uninitialized service; this one is ready.
    const nullEpoch = await stopWith(replacement, {
      expected_instance_id: currentRecord.instance_id,
      expected_engine_epoch: null,
    });
    assert.equal(nullEpoch.status, 409);

    // Matching identity stops the replacement and removes its record.
    const owned = await stopWith(replacement, {
      expected_instance_id: currentRecord.instance_id,
      expected_engine_epoch: currentRecord.engine_epoch,
    });
    assert.equal(owned.status, 200);
    assert.equal(owned.payload.status, 'stopping');
    assert.equal(
      await waitFor(() => readRecord(home) === null),
      true,
      'owning service removed its record while draining',
    );
  });

  test('failed open never publishes ready', async () => {
    // A regular file in place of the socket's parent directory: the core
    // opens, the listener bind is then refused (unsafe parent), and nothing
    // may be published or left behind.
    const home = seedHome();
    const blocker = join(home, 'blocker');
    writeFileSync(blocker, 'not a directory');
    const socketPath = join(blocker, 'service.sock');
    await assert.rejects(
      () => startServiceAt(home, { transport: 'unix', socketPath }),
      /not a real directory/,
    );
    assert.equal(readRecord(home), null, 'failed start published no record');
    const lockPath = join(home, '.nexus42', 'run', 'service.start.lock');
    assert.equal(existsSync(lockPath), false, 'failed start released the start lock');

    // The home stays usable: a normal start afterwards publishes cleanly.
    const recovery = await startServiceAt(home);
    try {
      assert.equal(recovery.discovery.readiness, 'ready');
      assert.equal(readRecord(home).instance_id, recovery.discovery.instance_id);
    } finally {
      await closeServiceBounded(recovery);
    }
    assert.equal(readRecord(home), null);
  });

  test('read-only attach never deletes or kills an unowned service', async () => {
    const home = seedHome();
    const service = await startServiceAt(home);
    try {
      const refused = await stopWith(service, {
        expected_instance_id: 'inst-not-the-owner',
        expected_engine_epoch: 7,
      });
      assert.equal(refused.status, 409);
      const health = await jsonRequest(`${service.url}/v1/daemon/runtime/health`);
      assert.equal(health.payload.status, 'ok', 'unowned service was not killed');
      assert.equal(readRecord(home).instance_id, service.discovery.instance_id);

      // The removal seam refuses foreign ids even in-process.
      const { removeOwnedDiscovery } = await import(join(serviceRoot, 'dist', 'discovery.js'));
      const removal = await removeOwnedDiscovery('inst-not-the-owner');
      assert.equal(removal.removed, false);
      assert.equal(readRecord(home).instance_id, service.discovery.instance_id, 'record not deleted');
    } finally {
      await closeServiceBounded(service);
    }
    assert.equal(readRecord(home), null, 'only the owner removes the record');
  });

  test('uninitialized shell publishes an explicit shell record', async () => {
    const home = emptyHome();
    const service = await startServiceAt(home);
    let stoppedThroughStopRoute = false;
    try {
      const record = service.discovery;
      assert.equal(record.readiness, 'uninitialized');
      assert.equal(record.creator_id, null);
      assert.equal(record.workspace_slug, null);
      assert.equal(record.engine_epoch, null);
      assert.equal(record.schema_version, 1);
      assert.equal(record.protocol_version, 1);
      assert.equal(readRecord(home).instance_id, record.instance_id);

      // Null epoch matches the uninitialized service.
      const owned = await stopWith(service, {
        expected_instance_id: record.instance_id,
        expected_engine_epoch: null,
      });
      assert.equal(owned.status, 200);
      assert.equal(owned.payload.status, 'stopping');
      stoppedThroughStopRoute = true;
      assert.equal(
        await waitFor(() => readRecord(home) === null),
        true,
        'shell stop removed the record',
      );
    } finally {
      if (!stoppedThroughStopRoute) await closeServiceBounded(service);
    }
    assert.equal(readRecord(home), null);
  });

  test('unix transport serves an equivalent surface and socket endpoint', async () => {
    const home = seedHome();
    const socketDir = track(mkdtempSync(join(tmpdir(), 'nexus-sock-')));
    const socketPath = join(socketDir, 'service.sock');
    const service = await startServiceAt(home, { transport: 'unix', socketPath });
    let stoppedThroughStopRoute = false;
    try {
      const endpoint = service.discovery.endpoint;
      assert.equal(endpoint.transport, 'unix');
      assert.equal(endpoint.path, socketPath);
      assert.equal(service.url, null);
      const onDisk = readRecord(home);
      assert.equal(onDisk.endpoint.transport, 'unix');
      assert.equal(onDisk.endpoint.path, socketPath);

      const health = await jsonRequest('/v1/daemon/runtime/health', { socketPath });
      assert.equal(health.payload.status, 'ok', 'unix transport serves the same liveness surface');

      const refused = await stopOverSocket(socketPath, {
        expected_instance_id: 'inst-foreign',
        expected_engine_epoch: 1,
      });
      assert.equal(refused.status, 409, 'unix transport enforces the same instance binding');

      const owned = await stopOverSocket(socketPath, {
        expected_instance_id: service.discovery.instance_id,
        expected_engine_epoch: service.discovery.engine_epoch,
      });
      assert.equal(owned.status, 200);
      assert.equal(owned.payload.status, 'stopping');
      stoppedThroughStopRoute = true;
      // Discovery removal is the last step of a confirmed close — poll it,
      // then the socket file is guaranteed gone with the listener.
      assert.equal(
        await waitFor(() => readRecord(home) === null),
        true,
        'owning service removed its record while draining',
      );
      assert.equal(
        await waitFor(() => !existsSync(socketPath)),
        true,
        'listener teardown removes the socket file',
      );
    } finally {
      if (!stoppedThroughStopRoute) await closeServiceBounded(service);
    }
    assert.equal(readRecord(home), null);
  });

  test('the service never imports Electron', async () => {
    const distMain = readFileSync(join(serviceRoot, 'dist', 'main.js'), 'utf8');
    const distIndex = readFileSync(join(serviceRoot, 'dist', 'index.js'), 'utf8');
    assert.equal(/electron/i.test(distMain), false, 'no Electron references in the entry');
    assert.equal(/electron/i.test(distIndex), false, 'no Electron references in the service');
  });

  test('a planted .nexus42 symlink is refused, never followed', async () => {
    const home = emptyHome();
    const outside = track(mkdtempSync(join(tmpdir(), 'nexus-outside-')));
    symlinkSync(outside, join(home, '.nexus42'), 'dir');
    await assert.rejects(() => startServiceAt(home), /not a real directory/);
    assert.equal(
      existsSync(join(outside, 'run')),
      false,
      'nothing may be created through the planted parent symlink',
    );
  });

  test('a legacy permissive run directory is forced back to 0700', async () => {
    const home = emptyHome();
    const nexusDir = join(home, '.nexus42');
    const runDir = join(nexusDir, 'run');
    mkdirSync(nexusDir, { mode: 0o755 });
    mkdirSync(runDir, { mode: 0o755 });
    const service = await startServiceAt(home);
    try {
      assert.equal(statSync(nexusDir).mode & 0o777, 0o700, 'parent chain is private');
      assert.equal(statSync(runDir).mode & 0o777, 0o700, 'existing run dir is repaired to 0700');
      assert.ok(service.discovery.instance_id.length > 0, 'start succeeds after repair');
    } finally {
      await closeServiceBounded(service);
    }
  });

  test('a symlinked discovery record is refused and replaced on republish', async () => {
    const { readPublishedDiscovery } = await import(join(serviceRoot, 'dist', 'discovery.js'));
    const home = emptyHome();
    const outside = track(mkdtempSync(join(tmpdir(), 'nexus-outside-')));
    const recordPath = join(home, '.nexus42', 'run', 'service.json');
    const first = await startServiceAt(home);
    try {
      assert.equal(first.discovery.instance_id.length > 0, true);
    } finally {
      await closeServiceBounded(first);
    }
    // Plant a leaf symlink whose target holds an attacker-chosen record.
    const planted = join(outside, 'fake-record.json');
    writeFileSync(planted, JSON.stringify({ instance_id: 'inst-attacker' }), { mode: 0o600 });
    symlinkSync(planted, recordPath);
    assert.equal(readPublishedDiscovery(home), null, 'a symlinked record is not read');
    const again = await startServiceAt(home);
    try {
      assert.equal(lstatSync(recordPath).isSymbolicLink(), false, 'publish replaced the leaf symlink');
      assert.equal(readPublishedDiscovery(home).instance_id, again.discovery.instance_id);
    } finally {
      await closeServiceBounded(again);
    }
  });

  function writeLockFile(path, token, pid) {
    writeFileSync(
      path,
      JSON.stringify({ pid, acquired_at: new Date().toISOString(), token }),
      { mode: 0o600 },
    );
  }

  test('a stale lock with a dead owner is still taken over', async () => {
    const home = emptyHome();
    const runDir = join(home, '.nexus42', 'run');
    mkdirSync(join(home, '.nexus42'), { mode: 0o700 });
    mkdirSync(runDir, { mode: 0o700 });
    const lockPath = join(runDir, 'service.start.lock');
    const dead = spawnSync('true');
    assert.ok(Number.isInteger(dead.pid), 'dead-owner pid fixture spawned');
    writeLockFile(lockPath, 'stale-token-0001', dead.pid);
    const service = await startServiceAt(home);
    try {
      assert.ok(service.discovery.instance_id.length > 0, 'dead-owner lock does not block the start');
    } finally {
      await closeServiceBounded(service);
    }
  });

  test('a live lock planted during stale recovery is never deleted', async () => {
    const home = emptyHome();
    const runDir = join(home, '.nexus42', 'run');
    mkdirSync(join(home, '.nexus42'), { mode: 0o700 });
    mkdirSync(runDir, { mode: 0o700 });
    const lockPath = join(runDir, 'service.start.lock');
    const dead = spawnSync('true');
    writeLockFile(lockPath, 'stale-token-0002', dead.pid);
    const attempt = startServiceAt(home);
    const successorBytes = JSON.stringify({
      pid: process.pid,
      acquired_at: new Date().toISOString(),
      token: 'successor-live-token',
    });
    // The clearer removes the stale lock between retry attempts; plant a
    // live lock the moment the stale one disappears — exactly what a racing
    // successor does in the read-then-unlink window this suite pins.
    let planted = false;
    for (let i = 0; i < 4_000 && !planted; i += 1) {
      if (!existsSync(lockPath)) {
        writeFileSync(lockPath, successorBytes, { mode: 0o600 });
        planted = true;
      } else {
        await new Promise((resolve) => setTimeout(resolve, 1));
      }
    }
    if (!planted) {
      // Event-loop stall swallowed the 25ms clear window: nothing about the
      // guarantee is exercised on this run; the start proceeds legitimately.
      await closeServiceBounded(await attempt);
      return;
    }
    await assert.rejects(attempt, (error) => {
      assert.equal(error.status, 503, 'the planted live lock holds the start off');
      return true;
    });
    assert.equal(
      readFileSync(lockPath, 'utf8'),
      successorBytes,
      'the successor lock survives stale recovery byte-for-byte',
    );
  });

  test('unix socket refuses unsafe parents and locks down the socket file', async () => {
    const home = emptyHome();
    // A shared-mode parent (0755) is refused before anything is bound.
    const openDir = track(mkdtempSync(join(tmpdir(), 'nexus-open-sock-')));
    chmodSync(openDir, 0o755);
    await assert.rejects(
      () => startServiceAt(home, { transport: 'unix', socketPath: join(openDir, 's.sock') }),
      /must be private \(0700\)/,
    );
    assert.equal(existsSync(join(openDir, 's.sock')), false, 'no socket in a shared parent');
    assert.equal(readRecord(home), null, 'refused binds publish no record');
    // A symlinked parent is a redirect, not a private directory.
    const realDir = track(mkdtempSync(join(tmpdir(), 'nexus-real-sock-')));
    const linkPath = join(track(mkdtempSync(join(tmpdir(), 'nexus-linkparent-'))), 'link');
    symlinkSync(realDir, linkPath, 'dir');
    await assert.rejects(
      () => startServiceAt(home, { transport: 'unix', socketPath: join(linkPath, 's.sock') }),
      /not a real directory/,
    );
    // A real 0700 parent binds, and the socket file itself is 0600.
    const safeDir = track(mkdtempSync(join(tmpdir(), 'nexus-safe-sock-')));
    const socketPath = join(safeDir, 'service.sock');
    const service = await startServiceAt(home, { transport: 'unix', socketPath });
    try {
      const socketStats = statSync(socketPath);
      assert.ok(socketStats.isSocket(), 'a filesystem socket was bound');
      assert.equal(socketStats.mode & 0o777, 0o600, 'socket file is owner-only');
      const health = await jsonRequest('/v1/daemon/runtime/health', { socketPath });
      assert.equal(health.payload.status, 'ok');
    } finally {
      await closeServiceBounded(service);
    }
  });

  test('port 0 admits requests via its actually-bound origin', async () => {
    const home = emptyHome();
    const service = await startServiceAt(home);
    try {
      const boundPort = new URL(service.url).port;
      assert.notEqual(boundPort, '0', 'the endpoint reports the real bound port');
      const admitted = await jsonRequest(`${service.url}/v1/daemon/runtime/health`, {
        headers: { Origin: `http://127.0.0.1:${boundPort}` },
      });
      assert.equal(admitted.status, 200, 'the actually-bound origin is admitted');
      const denied = await jsonRequest(`${service.url}/v1/daemon/runtime/health`, {
        headers: { Origin: 'http://evil.example:9931' },
      });
      assert.equal(denied.status, 403, 'foreign origins stay denied');
    } finally {
      await closeServiceBounded(service);
    }
  });
});
