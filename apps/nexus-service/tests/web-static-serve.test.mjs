import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from 'node:fs';
import http from 'node:http';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { after, before, describe, test } from 'node:test';

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = join(__dirname, '..', '..', '..');
const serviceRoot = join(__dirname, '..');

const ENABLED_PORT = 18_531;
const DISABLED_PORT = 18_532;
const KEYED_PORT = 18_533;

const INDEX_HTML = '<!doctype html><html><body>nexus-web-fixture</body></html>';
const APP_JS = "console.log('app-test');\n";

const tempDirs = [];

function tempDir(prefix) {
  const dir = mkdtempSync(join(tmpdir(), prefix));
  tempDirs.push(dir);
  return dir;
}

/**
 * Built-SPA fixture: an `assets/` hit, an ordinary served file, and a symlink
 * whose target lives outside the root.
 */
function webDistFixture() {
  const base = tempDir('nexus-webdist-');
  const root = join(base, 'dist');
  mkdirSync(join(root, 'assets'), { recursive: true });
  writeFileSync(join(root, 'index.html'), INDEX_HTML);
  writeFileSync(join(root, 'assets', 'app-test.js'), APP_JS);
  writeFileSync(join(root, 'robots.txt'), 'User-agent: *\n');
  const outside = join(base, 'outside');
  mkdirSync(outside);
  writeFileSync(join(outside, 'secret.txt'), 'secret');
  symlinkSync(outside, join(root, 'escape'), 'dir');
  return root;
}

/**
 * Request helper over `node:http` with an explicit raw path: `fetch` and the
 * WHATWG URL parser both collapse `%2e%2e` dot segments before they reach the
 * server, which is exactly the traversal form under test.
 */
function rawRequest(port, path, { method = 'GET', headers = {} } = {}) {
  return new Promise((resolveRequest, rejectRequest) => {
    const req = http.request({ host: '127.0.0.1', port, path, method, headers, agent: false }, (res) => {
      const chunks = [];
      res.on('data', (chunk) => chunks.push(chunk));
      res.on('end', () =>
        resolveRequest({
          status: res.statusCode,
          headers: res.headers,
          body: Buffer.concat(chunks),
        }),
      );
    });
    req.on('error', rejectRequest);
    req.end();
  });
}

function errorCode(response) {
  return JSON.parse(response.body.toString()).error.code;
}

function withTimeout(promise, ms) {
  let timer;
  return Promise.race([
    promise,
    new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Error('close timeout')), ms);
    }),
  ]).finally(() => clearTimeout(timer));
}

async function startFixtureService(port, extra = {}) {
  const { startService } = await import(join(serviceRoot, 'dist', 'index.js'));
  return startService({
    home: tempDir('nexus-web-home-'),
    host: '127.0.0.1',
    port,
    allowRemote: false,
    domainOnly: true,
    ...extra,
  });
}

describe('web static serve (20261019-web-static-serve)', () => {
  let createStaticFileHandler;
  let resolveWebDist;
  let parseCliArgs;

  before(() => {
    const build = spawnSync('npx', ['tsc', '-p', 'tsconfig.json'], {
      cwd: serviceRoot,
      stdio: 'inherit',
    });
    assert.equal(build.status, 0);
  });

  before(async () => {
    ({ createStaticFileHandler, resolveWebDist } = await import(
      join(serviceRoot, 'dist', 'static-files.js')
    ));
    ({ parseCliArgs } = await import(join(serviceRoot, 'dist', 'config.js')));
  });

  after(() => {
    for (const dir of tempDirs) rmSync(dir, { recursive: true, force: true });
  });

  describe('enabled mode (explicit --web-dist)', () => {
    let service;
    let root;

    before(async () => {
      root = webDistFixture();
      service = await startFixtureService(ENABLED_PORT, { webDist: root });
    });

    after(async () => {
      if (service) await withTimeout(service.close(), 8_000);
    });

    test('GET / serves index.html with no-store and nosniff', async () => {
      const res = await rawRequest(ENABLED_PORT, '/');
      assert.equal(res.status, 200);
      assert.equal(res.headers['content-type'], 'text/html; charset=utf-8');
      assert.equal(res.headers['cache-control'], 'no-store');
      assert.equal(res.headers['x-content-type-options'], 'nosniff');
      assert.equal(res.body.toString(), INDEX_HTML);
    });

    test('GET /assets/app-test.js serves the hashed asset with its MIME and long cache', async () => {
      const res = await rawRequest(ENABLED_PORT, '/assets/app-test.js');
      assert.equal(res.status, 200);
      assert.equal(res.headers['content-type'], 'text/javascript');
      assert.equal(res.headers['cache-control'], 'public, max-age=3600');
      assert.equal(res.headers['x-content-type-options'], 'nosniff');
      assert.equal(res.body.toString(), APP_JS);
    });

    test('a served file outside assets/ keeps no-store', async () => {
      const res = await rawRequest(ENABLED_PORT, '/robots.txt');
      assert.equal(res.status, 200);
      assert.equal(res.headers['content-type'], 'text/plain; charset=utf-8');
      assert.equal(res.headers['cache-control'], 'no-store');
    });

    test('GET /spa/route falls back to index.html', async () => {
      const res = await rawRequest(ENABLED_PORT, '/spa/route');
      assert.equal(res.status, 200);
      assert.equal(res.headers['content-type'], 'text/html; charset=utf-8');
      assert.equal(res.headers['cache-control'], 'no-store');
      assert.equal(res.body.toString(), INDEX_HTML);
    });

    test('HEAD / returns the GET headers with an empty body', async () => {
      const res = await rawRequest(ENABLED_PORT, '/', { method: 'HEAD' });
      assert.equal(res.status, 200);
      assert.equal(res.headers['content-type'], 'text/html; charset=utf-8');
      assert.equal(res.headers['cache-control'], 'no-store');
      assert.equal(res.headers['x-content-type-options'], 'nosniff');
      assert.equal(res.body.length, 0);
    });

    test('GET /v1/daemon/daemon/status is unchanged', async () => {
      const res = await rawRequest(ENABLED_PORT, '/v1/daemon/daemon/status');
      assert.equal(res.status, 200);
      assert.equal(JSON.parse(res.body.toString()).lifecycle_state, 'running');
    });

    test('GET an unknown /v1 route keeps the 501 envelope', async () => {
      const res = await rawRequest(ENABLED_PORT, '/v1/daemon/nope');
      assert.equal(res.status, 501);
      assert.equal(errorCode(res), 'route_not_migrated');
    });

    test('POST an unknown /v1 route keeps the 501 envelope', async () => {
      const res = await rawRequest(ENABLED_PORT, '/v1/daemon/nope', { method: 'POST' });
      assert.equal(res.status, 501);
      assert.equal(errorCode(res), 'route_not_migrated');
    });

    for (const path of [
      '/%2e%2e/%2e%2e/etc/passwd',
      '/%2E%2E/',
      '/%2e%2e%5c',
      '/%00',
      '/%2e%2e/web/dist/index.html',
      '/..%2f..%2fetc/passwd',
      '/%2Fetc%2Fpasswd',
      '/escape/secret.txt',
    ]) {
      test(`traversal attempt ${path} is refused with a bare 404`, async () => {
        const res = await rawRequest(ENABLED_PORT, path);
        assert.equal(res.status, 404);
        assert.equal(errorCode(res), 'not_found');
        assert.equal(JSON.parse(res.body.toString()).error.message, 'not found');
        assert.ok(!res.body.toString().includes('ENOENT'));
        assert.ok(!res.body.toString().includes('secret'));
      });
    }

    test('POST / is a 405 method_not_allowed envelope', async () => {
      const res = await rawRequest(ENABLED_PORT, '/', { method: 'POST' });
      assert.equal(res.status, 405);
      assert.equal(errorCode(res), 'method_not_allowed');
    });

    test('a non-allowlisted Origin still hits the checkOrigin 403', async () => {
      const res = await rawRequest(ENABLED_PORT, '/', {
        headers: { origin: 'https://evil.example' },
      });
      assert.equal(res.status, 403);
      assert.equal(errorCode(res), 'forbidden');
    });
  });

  describe('enabled mode (keyed)', () => {
    let service;

    before(async () => {
      process.env.NEXUS42_DAEMON_API_KEY = 'web-static-test-key';
      service = await startFixtureService(KEYED_PORT, { webDist: webDistFixture() });
    });

    after(async () => {
      delete process.env.NEXUS42_DAEMON_API_KEY;
      if (service) await withTimeout(service.close(), 8_000);
    });

    test('static serving is exempt from the API-key gate only', async () => {
      const staticRes = await rawRequest(KEYED_PORT, '/');
      assert.equal(staticRes.status, 200);
      assert.equal(staticRes.body.toString(), INDEX_HTML);

      const unguarded = await rawRequest(KEYED_PORT, '/v1/daemon/daemon/status');
      assert.equal(unguarded.status, 200);

      const guarded = await rawRequest(KEYED_PORT, '/v1/daemon/agent-host/health');
      assert.equal(guarded.status, 401);
      assert.equal(errorCode(guarded), 'auth_required');
    });
  });

  describe('disabled mode (webDist: null)', () => {
    let service;

    before(async () => {
      service = await startFixtureService(DISABLED_PORT, { webDist: null });
    });

    after(async () => {
      if (service) await withTimeout(service.close(), 8_000);
    });

    test('GET / keeps the pre-static 501 behavior', async () => {
      const res = await rawRequest(DISABLED_PORT, '/');
      assert.equal(res.status, 501);
      assert.equal(errorCode(res), 'route_not_migrated');
    });

    test('POST / keeps the pre-static 501 behavior', async () => {
      const res = await rawRequest(DISABLED_PORT, '/', { method: 'POST' });
      assert.equal(res.status, 501);
      assert.equal(errorCode(res), 'route_not_migrated');
    });

    test('the API surface is unchanged', async () => {
      const res = await rawRequest(DISABLED_PORT, '/v1/daemon/daemon/status');
      assert.equal(res.status, 200);
      assert.equal(JSON.parse(res.body.toString()).lifecycle_state, 'running');
    });
  });

  describe('startup and resolution', () => {
    test('explicit --web-dist naming a missing dir fails before bind', async () => {
      const missing = join(tempDir('nexus-web-missing-'), 'nope');
      const error = await startFixtureService(18_534, { webDist: missing }).then(
        () => null,
        (rejection) => rejection,
      );
      assert.ok(error instanceof Error);
      assert.match(error.message, /--web-dist requires/);
      assert.ok(error.message.includes(missing));
    });

    test('--web-dist parses into an absolute directory', () => {
      const args = parseCliArgs(['--home', '/tmp/nexus-web-home', '--web-dist', 'some/dist']);
      assert.equal(args.webDist, resolve('some/dist'));
    });

    test('resolveWebDist(null) force-disables serving', () => {
      assert.equal(resolveWebDist(null), null);
    });

    test('resolveWebDist(explicit) requires index.html in the directory', () => {
      const root = webDistFixture();
      assert.equal(resolveWebDist(root), root);
      assert.throws(() => resolveWebDist(join(root, 'missing')), /--web-dist requires/);
    });

    test('resolveWebDist(undefined) discovers the repo-anchored build output or misses', () => {
      const built = join(repoRoot, 'apps', 'web', 'dist');
      const discovered = resolveWebDist(undefined);
      if (existsSync(join(built, 'index.html'))) {
        assert.equal(discovered, built);
      } else {
        assert.equal(discovered, null);
      }
    });
  });

  describe('static-files resolver (unit)', () => {
    let root;
    let handler;

    before(() => {
      root = webDistFixture();
      handler = createStaticFileHandler({ root, fallback: join(root, 'index.html') });
    });

    function fakeRes() {
      return {
        statusCode: 0,
        headers: {},
        setHeader(name, value) {
          this.headers[name.toLowerCase()] = value;
        },
        end(body) {
          this.body = body;
        },
      };
    }

    for (const path of [
      '/../etc/passwd',
      '/%2e%2e/',
      '/%2E%2E/',
      '/%2e%2e%5c',
      '/%00',
      '/%2Fetc%2Fpasswd',
      '/escape/secret.txt',
    ]) {
      test(`${path} resolves to a refusal`, async () => {
        await assert.rejects(
          handler(fakeRes(), 'GET', path),
          (error) => error.status === 404 && error.code === 'not_found',
        );
      });
    }

    test('an unknown in-root path falls back to index.html', async () => {
      const res = fakeRes();
      await handler(res, 'GET', '/no/such/file');
      assert.equal(res.statusCode, 200);
      assert.equal(res.headers['content-type'], 'text/html; charset=utf-8');
      assert.equal(res.headers['cache-control'], 'no-store');
    });
  });
});
