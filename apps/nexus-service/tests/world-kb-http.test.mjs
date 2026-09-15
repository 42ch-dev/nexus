import assert from 'node:assert/strict';
import { createHash, X509Certificate } from 'node:crypto';
import { mkdtempSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';
import https from 'node:https';
import net from 'node:net';
import { describe, test, before, after } from 'node:test';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = join(__dirname, '..', '..', '..');
const serviceRoot = join(__dirname, '..');
const require = createRequire(import.meta.url);
const OWNED_WORLD = 'wld_owned';
const FOREIGN_WORLD = 'wld_foreign';

function seedHome() {
  const home = mkdtempSync(join(tmpdir(), 'nexus-service-http-'));
  const seed = spawnSync(
    'cargo',
    ['run', '-q', '-p', 'nexus-core-node', '--bin', 'native-wire-fixture-seed', '--', home],
    { stdio: 'inherit' },
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
  return { status: response.status, headers: response.headers, payload, text };
}

async function startDomainService(home, port) {
  const { startService } = await import(join(serviceRoot, 'dist/index.js'));
  return startService({
    home,
    host: '127.0.0.1',
    port,
    allowRemote: false,
    domainOnly: true,
  });
}

async function startService(options) {
  const mod = await import(join(serviceRoot, 'dist/index.js'));
  return mod.startService(options);
}

/** DER SHA-256 fingerprint in the frozen `SHA256:<colon-hex>` wire format. */
function expectedDerFingerprint(pem) {
  const cert = new X509Certificate(pem);
  const digest = createHash('sha256').update(cert.raw).digest();
  return `SHA256:${[...digest].map((byte) => byte.toString(16).padStart(2, '0')).join(':')}`;
}

function httpsGet(url, ca) {
  return new Promise((resolve, reject) => {
    https
      .get(url, { ca, rejectUnauthorized: true }, (res) => {
        let body = '';
        res.setEncoding('utf8');
        res.on('data', (chunk) => {
          body += chunk;
        });
        res.on('end', () => resolve({ status: res.statusCode, body }));
      })
      .on('error', reject);
  });
}

function seedTlsMaterial() {
  const dir = mkdtempSync(join(tmpdir(), 'nexus-service-tls-'));
  const keyPath = join(dir, 'key.pem');
  const certPath = join(dir, 'cert.pem');
  const gen = spawnSync(
    'openssl',
    [
      'req',
      '-x509',
      '-newkey',
      'rsa:2048',
      '-nodes',
      '-keyout',
      keyPath,
      '-out',
      certPath,
      '-days',
      '1',
      '-subj',
      '/CN=127.0.0.1',
      '-addext',
      'subjectAltName=IP:127.0.0.1',
    ],
    { stdio: 'ignore' },
  );
  assert.equal(gen.status, 0, 'openssl must generate the local test certificate');
  return { keyPath, certPath, pem: readFileSync(certPath, 'utf8') };
}

describe('world-kb-http (P4-T1)', () => {
  let home;
  let service;
  let baseUrl;

  before(async () => {
    home = seedHome();
    const build = spawnSync('npx', ['tsc', '-p', 'tsconfig.json'], {
      cwd: serviceRoot,
      stdio: 'inherit',
    });
    assert.equal(build.status, 0);
    service = await startDomainService(home, 18_421);
    baseUrl = service.url;

    const graph = await jsonFetch(`${baseUrl}/v1/daemon/worlds/${OWNED_WORLD}/kb/graph`);
    assert.equal(graph.status, 200, graph.text);
    assert.ok(Array.isArray(graph.payload.entities));

    const candidates = await jsonFetch(`${baseUrl}/v1/daemon/worlds/${OWNED_WORLD}/kb/candidates`);
    assert.equal(candidates.status, 200, candidates.text);
    assert.equal(candidates.payload.items.length, 2, 'fixture must expose two pending candidates');
  });

  after(async () => {
    if (service) {
      await service.close();
    }
  });

  test('keyless loopback allows runtime health with request id', async () => {
    const res = await jsonFetch(`${baseUrl}/v1/daemon/runtime/health`, {
      headers: { 'X-Request-Id': 'req-health-1' },
    });
    assert.equal(res.status, 200);
    assert.equal(res.payload.status, 'ok');
    assert.equal(res.headers.get('x-request-id'), 'req-health-1');
  });

  test('runtime status reflects initialized workspace', async () => {
    const res = await jsonFetch(`${baseUrl}/v1/daemon/runtime/status`);
    assert.equal(res.status, 200);
    assert.equal(res.payload.workspace_initialized, true);
    assert.equal(res.payload.runtime_mode, 'domain_only');
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
      const missing = await jsonFetch(`${local.url}/v1/daemon/worlds/${OWNED_WORLD}/kb/graph`);
      assert.equal(missing.status, 401);
      assert.equal(missing.payload.error.code, 'auth_required');
      assert.ok(missing.payload.error.request_id);

      const wrong = await jsonFetch(`${local.url}/v1/daemon/worlds/${OWNED_WORLD}/kb/graph`, {
        headers: { 'X-API-Key': 'wrong' },
      });
      assert.equal(wrong.status, 401);

      const ok = await jsonFetch(`${local.url}/v1/daemon/worlds/${OWNED_WORLD}/kb/graph`, {
        headers: { 'X-API-Key': 'test-secret-key' },
      });
      assert.equal(ok.status, 200);
    } finally {
      await local.close();
      if (previous === undefined) delete process.env.NEXUS42_DAEMON_API_KEY;
      else process.env.NEXUS42_DAEMON_API_KEY = previous;
    }
    service = await startDomainService(home, 18_421);
    baseUrl = service.url;
  });

  test('origin outside allowlist is denied before mutation', async () => {
    const res = await jsonFetch(`${baseUrl}/v1/daemon/worlds/${OWNED_WORLD}/kb/graph`, {
      headers: { Origin: 'http://evil.example.com:9999' },
    });
    assert.equal(res.status, 403);
    assert.equal(res.payload.error.code, 'forbidden');
  });

  test('malformed candidate limit suffix is rejected', async () => {
    const res = await jsonFetch(`${baseUrl}/v1/daemon/worlds/${OWNED_WORLD}/kb/candidates?limit=2junk`);
    assert.equal(res.status, 400);
    assert.equal(res.payload.error.code, 'invalid_input');
  });

  test('candidate limit clamps high values instead of rejecting', async () => {
    const res = await jsonFetch(`${baseUrl}/v1/daemon/worlds/${OWNED_WORLD}/kb/candidates?limit=999`);
    assert.equal(res.status, 200);
    assert.ok(res.payload.items.length <= 250);
  });

  test('foreign and missing worlds preserve ordering', async () => {
    const foreign = await jsonFetch(`${baseUrl}/v1/daemon/worlds/${FOREIGN_WORLD}/kb/graph`);
    assert.equal(foreign.status, 403);
    assert.equal(foreign.payload.error.code, 'forbidden');

    const missing = await jsonFetch(`${baseUrl}/v1/daemon/worlds/wld_does_not_exist/kb/graph`);
    assert.equal(missing.status, 404);
    assert.equal(missing.payload.error.code, 'not_found');
  });

  test('patch create/update/stale CAS against real fixture DB', async () => {
    const create = await jsonFetch(`${baseUrl}/v1/daemon/worlds/${OWNED_WORLD}/kb/patch-entity`, {
      method: 'POST',
      body: {
        entity_id: 'kb_abc124',
        expected_version: 0,
        patch: { title: 'Created', block_type: 'character' },
      },
    });
    assert.equal(create.status, 200, create.text);
    assert.equal(create.payload.version, 1);

    const update = await jsonFetch(`${baseUrl}/v1/daemon/worlds/${OWNED_WORLD}/kb/patch-entity`, {
      method: 'POST',
      body: {
        entity_id: 'kb_mod',
        expected_version: 0,
        patch: { title: 'Updated Mod' },
      },
    });
    assert.equal(update.status, 200, update.text);
    assert.equal(update.payload.version, 1);

    const stale = await jsonFetch(`${baseUrl}/v1/daemon/worlds/${OWNED_WORLD}/kb/patch-entity`, {
      method: 'POST',
      body: {
        entity_id: 'kb_cas',
        expected_version: 1,
        patch: { title: 'Stale' },
      },
    });
    assert.equal(stale.status, 409, stale.text);
    assert.equal(stale.payload.error.code, 'world_kb_conflict');
  });

  test('world kb conflict surfaces structured details', async () => {
    const res = await jsonFetch(`${baseUrl}/v1/daemon/worlds/${OWNED_WORLD}/kb/patch-entity`, {
      method: 'POST',
      body: {
        entity_id: 'kb_cas',
        expected_version: 1,
        patch: { title: 'Stale Again' },
      },
    });
    assert.equal(res.status, 409, res.text);
    assert.equal(res.payload.error.code, 'world_kb_conflict');
    assert.equal(res.payload.error.details.entity_id, 'kb_cas');
    assert.equal(res.payload.error.details.current_version, 2);
    assert.equal(typeof res.payload.error.details.conflicting_path, 'string');
    assert.equal(typeof res.payload.error.details.recovery_hint, 'string');
  });

  test('world kb validation surfaces validation_summary', async () => {
    const res = await jsonFetch(`${baseUrl}/v1/daemon/worlds/${OWNED_WORLD}/kb/patch-entity`, {
      method: 'POST',
      body: {
        entity_id: 'kb_def456',
        expected_version: 0,
        patch: { title: '   ', block_type: 'character' },
      },
    });
    assert.equal(res.status, 422, res.text);
    assert.equal(res.payload.error.code, 'world_kb_validation');
    const errors = res.payload.error.details.validation_summary.errors;
    assert.ok(Array.isArray(errors));
    assert.ok(errors.length > 0);
  });

  test('oversized request body is rejected before native effects', async () => {
    const padding = 'x'.repeat(1024 * 1024);
    const body = `{"entity_id":"kb_mod","expected_version":0,"patch":{"title":"${padding}"}}`;
    const response = await fetch(`${baseUrl}/v1/daemon/worlds/${OWNED_WORLD}/kb/patch-entity`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body,
    });
    assert.equal(response.status, 413);
    const payload = await response.json();
    assert.equal(payload.error.code, 'input_too_large');
  });

  test('host session not found maps to 404', async () => {
    const res = await jsonFetch(`${baseUrl}/v1/daemon/agent-host/sessions/00000000-0000-4000-8000-000000000000`);
    assert.equal(res.status, 404);
    assert.equal(res.payload.error.code, 'not_found');
  });

  test('non-loopback bind is rejected before listen', async () => {
    const { startService } = await import(join(serviceRoot, 'dist/index.js'));
    await assert.rejects(
      () =>
        startService({
          home,
          host: '0.0.0.0',
          port: 18_423,
          allowRemote: false,
          domainOnly: true,
        }),
      (error) => {
        assert.match(String(error), /non-loopback|forbidden|allow-remote/i);
        return true;
      },
    );
  });

  test('unported daemon route returns route_not_migrated', async () => {
    const res = await jsonFetch(`${baseUrl}/v1/daemon/workspace/init`, { method: 'POST', body: {} });
    assert.equal(res.status, 501);
    assert.equal(res.payload.error.code, 'route_not_migrated');
  });

  test('close returns native cleanup report', async () => {
    const report = await service.close();
    service = undefined;
    assert.ok(['closed', 'interrupted'].includes(report.state));
    assert.equal(typeof report.cleanup_confirmed, 'boolean');
  });
});

describe('world-kb-http provider readiness (P4-T1)', () => {
  let home;
  let service;

  before(async () => {
    home = seedHome();
    // Provider-enabled (EngineOwner): the native environment admits one core at
    // a time, so this runs after the domain-only fixture service has closed.
    service = await startService({
      home,
      host: '127.0.0.1',
      port: 18_425,
      allowRemote: false,
    });
  });

  after(async () => {
    if (service) {
      await service.close();
    }
  });

  test('reports degraded readiness when no provider is available', async () => {
    const status = await jsonFetch(`${service.url}/v1/daemon/runtime/status`);
    assert.equal(status.status, 200);
    assert.equal(status.payload.workspace_initialized, true);
    // Host liveness is not readiness: the fixture has no provider whose bounded
    // availability probe succeeded, so the profile is degraded.
    assert.equal(status.payload.runtime_mode, 'provider_degraded');
    assert.equal(status.payload.acp.tool_execution_enabled, false);

    const daemon = await jsonFetch(`${service.url}/v1/daemon/daemon/status`);
    assert.equal(daemon.payload.subsystems.engine.status, 'down');
    assert.ok(daemon.payload.degraded.subsystems.includes('engine'));

    const health = await jsonFetch(`${service.url}/v1/daemon/agent-host/health`);
    assert.equal(health.status, 200);
    assert.equal(health.payload.running, true);
  });

  test('close confirms provider-enabled cleanup', async () => {
    const report = await service.close();
    service = undefined;
    assert.equal(report.state, 'closed');
    assert.equal(report.cleanup_confirmed, true);
  });
});

describe('world-kb-http uninitialized profile (P4-T1)', () => {
  let home;
  let service;

  before(async () => {
    home = mkdtempSync(join(tmpdir(), 'nexus-service-uninit-'));
    service = await startDomainService(home, 18_430);
  });

  after(async () => {
    if (service) {
      await service.close();
    }
  });

  test('publishes uninitialized status instead of failing startup', async () => {
    const health = await jsonFetch(`${service.url}/v1/daemon/runtime/health`);
    assert.equal(health.status, 200);

    const status = await jsonFetch(`${service.url}/v1/daemon/runtime/status`);
    assert.equal(status.status, 200);
    assert.equal(status.payload.workspace_initialized, false);
    assert.equal(status.payload.runtime_mode, 'uninitialized');
    assert.equal(status.payload.acp.tool_execution_enabled, false);

    const daemon = await jsonFetch(`${service.url}/v1/daemon/daemon/status`);
    assert.equal(daemon.status, 200);
    assert.equal(daemon.payload.subsystems.db.status, 'down');
    assert.equal(daemon.payload.subsystems.engine.status, 'down');
    assert.deepEqual(daemon.payload.degraded.subsystems, ['db']);
  });

  test('denies world kb and host effects with typed uninitialized', async () => {
    const graph = await jsonFetch(`${service.url}/v1/daemon/worlds/${OWNED_WORLD}/kb/graph`);
    assert.equal(graph.status, 409, graph.text);
    assert.equal(graph.payload.error.code, 'uninitialized');

    const candidates = await jsonFetch(
      `${service.url}/v1/daemon/worlds/${OWNED_WORLD}/kb/candidates`,
    );
    assert.equal(candidates.status, 409);

    const patch = await jsonFetch(
      `${service.url}/v1/daemon/worlds/${OWNED_WORLD}/kb/patch-entity`,
      {
        method: 'POST',
        body: { entity_id: 'kb_new', expected_version: 0, patch: { title: 'Denied' } },
      },
    );
    assert.equal(patch.status, 409);
    assert.equal(patch.payload.error.code, 'uninitialized');

    const host = await jsonFetch(`${service.url}/v1/daemon/agent-host/health`);
    assert.equal(host.status, 409);
    assert.equal(host.payload.error.code, 'uninitialized');

    const sessions = await jsonFetch(`${service.url}/v1/daemon/agent-host/sessions`);
    assert.equal(sessions.status, 409);
  });

  test('close confirms status-only cleanup', async () => {
    const report = await service.close();
    service = undefined;
    assert.equal(report.state, 'closed');
    assert.equal(report.cleanup_confirmed, true);
  });

  test('provider-enabled profile also publishes uninitialized status', async () => {
    const { startService } = await import(join(serviceRoot, 'dist/index.js'));
    const local = await startService({
      home,
      host: '127.0.0.1',
      port: 18_431,
      allowRemote: false,
    });
    try {
      const status = await jsonFetch(`${local.url}/v1/daemon/runtime/status`);
      assert.equal(status.status, 200);
      assert.equal(status.payload.workspace_initialized, false);
      assert.equal(status.payload.runtime_mode, 'uninitialized');
      assert.equal(status.payload.acp.tool_execution_enabled, false);

      const daemon = await jsonFetch(`${local.url}/v1/daemon/daemon/status`);
      assert.equal(daemon.payload.subsystems.engine.status, 'down');
      assert.deepEqual(daemon.payload.degraded.subsystems, ['db', 'engine']);

      const host = await jsonFetch(`${local.url}/v1/daemon/agent-host/health`);
      assert.equal(host.status, 409);
      assert.equal(host.payload.error.code, 'uninitialized');
    } finally {
      const report = await local.close();
      assert.equal(report.state, 'closed');
      assert.equal(report.cleanup_confirmed, true);
    }
  });
});

describe('world-kb-http transport bounds (P4-T1)', () => {
  let home;
  let service;

  before(async () => {
    home = seedHome();
    service = await startDomainService(home, 18_440);
  });

  after(async () => {
    if (service) {
      await service.close();
    }
  });

  test('slow headers hit the configured 5s server deadline without native effects', async () => {
    const [, port] = service.url.replace('http://', '').split(':');
    const socket = net.connect({ host: '127.0.0.1', port: Number(port) });
    const started = Date.now();
    let response = '';

    await new Promise((resolve, reject) => {
      socket.setTimeout(20_000, () => {
        socket.destroy();
        reject(new Error('slow-header socket never settled'));
      });
      socket.on('connect', () => {
        // Partial request line + headers, then stall: the handler is never entered.
        socket.write('GET /v1/daemon/runtime/health HTTP/1.1\r\nHost: 127.0.0.1\r\n');
      });
      socket.on('data', (chunk) => {
        response += chunk.toString('utf8');
        if (response.includes('408')) {
          socket.destroy();
          resolve();
        }
      });
      socket.on('close', () => resolve());
      socket.on('error', () => resolve());
    });

    const elapsed = Date.now() - started;
    assert.match(response, /408/, `expected a 408 from headersTimeout, got: ${response}`);
    assert.ok(elapsed >= 4_000, `expected the 5s header deadline, closed after ${elapsed}ms`);
    assert.ok(elapsed < 10_000, `header deadline must be bounded, took ${elapsed}ms`);

    // The listener is still serving after one timed-out slow client.
    const healthy = await jsonFetch(`${service.url}/v1/daemon/runtime/health`);
    assert.equal(healthy.status, 200);
  });

  test('error envelopes carry a request id and no home path', async () => {
    const res = await jsonFetch(`${service.url}/v1/daemon/worlds/wld_does_not_exist/kb/graph`, {
      headers: { 'X-Request-Id': 'req-sanitize-1' },
    });
    assert.equal(res.status, 404);
    assert.equal(res.payload.error.request_id, 'req-sanitize-1');
    assert.equal(res.text.includes(home), false);
    assert.equal(res.text.includes('.nexus42'), false);
  });
});

describe('world-kb-http error policy (P4-T1)', () => {
  test('finite fractions survive serialization while unsafe integers are rejected', async () => {
    const { stringifyJsonSafe, mapNativeError, toErrorBody } = await import(
      join(serviceRoot, 'dist/errors.js')
    );

    assert.equal(stringifyJsonSafe({ ratio: 1.5, nested: [{ value: 0.25 }] }), '{"ratio":1.5,"nested":[{"value":0.25}]}');

    assert.throws(
      () => stringifyJsonSafe({ revision: Number.MAX_SAFE_INTEGER + 1 }),
      (error) => error.code === 'invalid_input' && /safe integer/.test(error.message),
    );
    assert.throws(
      () => stringifyJsonSafe({ revision: Number.POSITIVE_INFINITY }),
      (error) => error.code === 'invalid_input' && /not finite/.test(error.message),
    );

    const privateHome = '/Users/private-operator/.nexus42';
    const internal = new Error(
      JSON.stringify({
        code: 'internal',
        message: `database_error: unable to open ${privateHome}/state.db`,
        details: { home: privateHome },
        http_status: 500,
      }),
    );
    const mapped = mapNativeError(internal);
    assert.equal(mapped.code, 'internal');
    assert.equal(mapped.status, 500);
    assert.equal(mapped.message, 'Internal server error');
    const body = JSON.stringify(toErrorBody(mapped, 'req-int-1'));
    assert.equal(body.includes(privateHome), false);
    assert.equal(body.includes('state.db'), false);
    assert.match(body, /"request_id":"req-int-1"/);
  });
});

describe('world-kb-http tls (P4-T1)', () => {
  let home;
  let service;
  let certPath;
  let keyPath;
  let pem;

  before(async () => {
    home = seedHome();
    const material = seedTlsMaterial();
    certPath = material.certPath;
    keyPath = material.keyPath;
    pem = material.pem;
    service = await startService({
      home,
      host: '127.0.0.1',
      port: 18_441,
      allowRemote: false,
      domainOnly: true,
      tlsCert: certPath,
      tlsKey: keyPath,
    });
  });

  after(async () => {
    if (service) {
      await service.close();
    }
  });

  test('serves HTTPS and reports the DER certificate fingerprint', async () => {
    assert.match(service.url, /^https:\/\/127\.0\.0\.1:18441$/);

    const health = await httpsGet(`${service.url}/v1/daemon/runtime/health`, pem);
    assert.equal(health.status, 200);
    assert.match(health.body, /"status":"ok"/);

    const fingerprint = await httpsGet(`${service.url}/v1/daemon/runtime/cert-fingerprint`, pem);
    assert.equal(fingerprint.status, 200);
    const payload = JSON.parse(fingerprint.body);
    assert.equal(payload.algorithm, 'sha256');
    assert.equal(payload.fingerprint, expectedDerFingerprint(pem));
    assert.match(payload.fingerprint, /^SHA256:[0-9a-f]{2}(:[0-9a-f]{2}){31}$/);
    assert.equal(typeof payload.created_at, 'string');
  });

  test('remote admission fails closed before listen', async () => {
    const noRemote = await startService({
      home,
      host: '10.0.0.5',
      port: 18_442,
      allowRemote: false,
      domainOnly: true,
      tlsCert: certPath,
      tlsKey: keyPath,
    }).then(
      () => null,
      (error) => error,
    );
    assert.ok(noRemote, 'non-loopback bind without --allow-remote must be rejected');
    assert.match(String(noRemote.message), /non-loopback|allow-remote/i);

    const noKey = await startService({
      home,
      host: '10.0.0.5',
      port: 18_443,
      allowRemote: true,
      domainOnly: true,
      tlsCert: certPath,
      tlsKey: keyPath,
    }).then(
      () => null,
      (error) => error,
    );
    assert.ok(noKey, 'remote bind without a configured key must be rejected');
    assert.match(String(noKey.message), /API_KEY|api key|auth_required/i);
  });
});

describe('world-kb-http close owner (P4-T1)', () => {
  const closedReport = () => ({
    state: 'closed',
    cleanup_confirmed: true,
    pending_operations: [],
  });

  test('native close starts in the same turn as listener teardown, not after it', async () => {
    const { createCloseOwner } = await import(join(serviceRoot, 'dist/index.js'));
    const started = Date.now();
    let teardownStart = null;
    let coreStart = null;

    const close = createCloseOwner({
      getServer: () => ({ listening: true }),
      budgetMs: 600,
      teardownListener: async () => {
        teardownStart = Date.now() - started;
        await new Promise((resolve) => setTimeout(resolve, 250));
      },
      closeCore: async () => {
        coreStart = Date.now() - started;
        return closedReport();
      },
    });

    const report = await close();
    assert.equal(report.state, 'closed');
    // A sequential implementation would invoke the native close only after the
    // 250ms listener teardown; concurrency puts both in the first turn.
    assert.ok(coreStart !== null, 'native close must be invoked');
    assert.ok(coreStart < 100, `native close started ${coreStart}ms in, not immediately`);
    assert.ok(teardownStart < 100, `listener teardown started ${teardownStart}ms in`);
  });

  test('a hung listener close still settles inside the single budget', async () => {
    const { createCloseOwner, stopListening } = await import(join(serviceRoot, 'dist/index.js'));
    const fakeServer = {
      listening: true,
      close() {
        // Never calls back: models a listener whose close event never fires.
      },
      closeIdleConnections() {},
      closeAllConnections() {},
    };

    const started = Date.now();
    await stopListening(fakeServer, Date.now() + 250);
    const teardownElapsed = Date.now() - started;
    assert.ok(teardownElapsed >= 200, `backstop fired at ${teardownElapsed}ms`);
    assert.ok(teardownElapsed < 900, `backstop must be bounded, took ${teardownElapsed}ms`);

    const close = createCloseOwner({
      getServer: () => fakeServer,
      budgetMs: 300,
      closeCore: async () => closedReport(),
    });
    const closeStarted = Date.now();
    const report = await close();
    const total = Date.now() - closeStarted;
    assert.equal(report.state, 'closed');
    assert.ok(total < 900, `one budget must cover the hung listener, took ${total}ms`);
  });

  test('interrupted attempts stay retryable while a confirmed close is idempotent', async () => {
    const { createCloseOwner } = await import(join(serviceRoot, 'dist/index.js'));
    let calls = 0;
    const close = createCloseOwner({
      getServer: () => undefined,
      budgetMs: 200,
      closeCore: async () => {
        calls += 1;
        if (calls === 1) {
          return {
            state: 'interrupted',
            cleanup_confirmed: false,
            pending_operations: ['op:retained'],
            reason: 'user_requested',
          };
        }
        return closedReport();
      },
    });

    const first = await close();
    assert.equal(first.state, 'interrupted');
    assert.equal(first.cleanup_confirmed, false);
    assert.deepEqual(first.pending_operations, ['op:retained']);

    const second = await close();
    assert.equal(second.state, 'closed');
    assert.equal(calls, 2, 'an unconfirmed report must trigger a real retry');

    const third = await close();
    assert.equal(third.state, 'closed');
    assert.equal(calls, 2, 'a confirmed close is cached, not re-attempted');
  });

  test('concurrent callers share one close attempt', async () => {
    const { createCloseOwner } = await import(join(serviceRoot, 'dist/index.js'));
    let calls = 0;
    const close = createCloseOwner({
      getServer: () => undefined,
      budgetMs: 200,
      closeCore: async () => {
        calls += 1;
        await new Promise((resolve) => setTimeout(resolve, 60));
        return closedReport();
      },
    });

    const [a, b, c] = await Promise.all([close(), close(), close()]);
    assert.equal(calls, 1);
    assert.equal(a.state, 'closed');
    assert.equal(b.state, 'closed');
    assert.equal(c.state, 'closed');
  });
});

describe('world-kb-http lifecycle (P4-T1)', () => {
  let home;
  let binding;

  before(async () => {
    home = seedHome();
    const { loadNodePath } = await import(join(root, 'packages', 'nexus-native', 'dist', 'loader.js'));
    binding = require(loadNodePath());
  });

  test('close settles within the deadline while a request is still in flight', async () => {
    const local = await startDomainService(home, 18_451);
    const port = Number(local.url.split(':')[2]);
    // Admitted request with a declared body that never arrives: the handler is
    // parked in the body read, so the socket is genuinely active.
    const socket = net.connect({ host: '127.0.0.1', port });
    socket.on('error', () => undefined);
    await new Promise((resolve) => socket.on('connect', resolve));
    socket.write(
      'POST /v1/daemon/worlds/wld_owned/kb/patch-entity HTTP/1.1\r\n' +
        'Host: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: 4096\r\n\r\n' +
        '{"entity_id":',
    );

    const started = Date.now();
    const report = await local.close();
    const elapsed = Date.now() - started;
    socket.destroy();

    assert.ok(elapsed < 7_000, `close must stay within the deadline, took ${elapsed}ms`);
    assert.ok(['closed', 'interrupted'].includes(report.state));
    assert.equal(typeof report.cleanup_confirmed, 'boolean');
  });

  test('an interrupted close stays retryable until the retained owner settles', async () => {
    const local = await startDomainService(home, 18_450);
    binding.forceUnconfirmedCleanup(true);
    let first;
    try {
      first = await local.close();
    } finally {
      binding.forceUnconfirmedCleanup(false);
    }
    assert.equal(first.state, 'interrupted');
    assert.equal(first.cleanup_confirmed, false);

    const second = await local.close();
    assert.equal(second.state, 'closed');
    assert.equal(second.cleanup_confirmed, true);

    const third = await local.close();
    assert.equal(third.state, 'closed');
    assert.equal(third.cleanup_confirmed, true);
  });
});
