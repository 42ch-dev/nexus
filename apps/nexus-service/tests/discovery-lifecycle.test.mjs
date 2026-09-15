import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
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
function jsonRequest(target, { method = 'GET', body, socketPath } = {}) {
  return new Promise((resolveRequest, rejectRequest) => {
    const payload = body === undefined ? null : JSON.stringify(body);
    const headers =
      payload === null
        ? {}
        : { 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(payload) };
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
    // A regular file blocks the socket path: the core opens, the listener bind
    // fails after it, and nothing may be published or left behind.
    const home = seedHome();
    const blocker = join(home, 'blocker');
    writeFileSync(blocker, 'not a directory');
    const socketPath = join(blocker, 'service.sock');
    await assert.rejects(
      () => startServiceAt(home, { transport: 'unix', socketPath }),
      /ENOTDIR/,
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
});
