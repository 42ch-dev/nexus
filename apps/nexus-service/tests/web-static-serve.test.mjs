import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import {
  chmodSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from 'node:fs';
import { open as openFile } from 'node:fs/promises';
import http from 'node:http';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { Readable, Writable } from 'node:stream';
import { fileURLToPath } from 'node:url';
import { after, before, describe, mock, test } from 'node:test';

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = join(__dirname, '..', '..', '..');
const serviceRoot = join(__dirname, '..');

const ENABLED_PORT = 18_531;
const DISABLED_PORT = 18_532;
const KEYED_PORT = 18_533;
const ESCAPE_PORT = 18_535;

// Root bypasses the file mode, so the unreadable-file case cannot be provoked.
const isRoot = typeof process.getuid === 'function' && process.getuid() === 0;

const INDEX_HTML = '<!doctype html><html><body>nexus-web-fixture</body></html>';
const APP_JS = "console.log('app-test');\n";

const tempDirs = [];

function tempDir(prefix) {
  const dir = mkdtempSync(join(tmpdir(), prefix));
  tempDirs.push(dir);
  return dir;
}

/**
 * Built-SPA fixture: an `assets/` hit, an ordinary served file, a file whose
 * mode denies reading (metadata still resolves), and a symlink whose target
 * lives outside the root.
 */
function webDistFixture() {
  const base = tempDir('nexus-webdist-');
  const root = join(base, 'dist');
  mkdirSync(join(root, 'assets'), { recursive: true });
  writeFileSync(join(root, 'index.html'), INDEX_HTML);
  writeFileSync(join(root, 'assets', 'app-test.js'), APP_JS);
  writeFileSync(join(root, 'robots.txt'), 'User-agent: *\n');
  const unreadable = join(root, 'unreadable.txt');
  writeFileSync(unreadable, 'unreadable');
  chmodSync(unreadable, 0o000);
  const outside = join(base, 'outside');
  mkdirSync(outside);
  writeFileSync(join(outside, 'secret.txt'), 'secret');
  symlinkSync(outside, join(root, 'escape'), 'dir');
  return root;
}

/**
 * Dist whose `index.html` is a symlink to a file outside the root: the SPA
 * fallback must be refused exactly like a direct `/index.html` hit.
 */
function escapingIndexFixture() {
  const base = tempDir('nexus-webdist-esc-');
  const root = join(base, 'dist');
  mkdirSync(join(root, 'assets'), { recursive: true });
  const outside = join(base, 'outside');
  mkdirSync(outside);
  writeFileSync(join(outside, 'index.html'), '<!doctype html><body>outside-secret</body>');
  symlinkSync(join(outside, 'index.html'), join(root, 'index.html'), 'file');
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

/** Error envelope with the volatile request id masked, for byte-identity checks. */
function envelopeOf(response) {
  return response.body.toString().replace(/"request_id":"[^"]*"/, '"request_id":"<id>"');
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

    test('a dot-segment alias to index.html keeps the entry-HTML no-store tier', async () => {
      // `/assets/%2e%2e%2findex.html` normalizes to the root `index.html`; the
      // cache tier follows the served location, not the raw first segment.
      const res = await rawRequest(ENABLED_PORT, '/assets/%2e%2e%2findex.html');
      assert.equal(res.status, 200);
      assert.equal(res.headers['content-type'], 'text/html; charset=utf-8');
      assert.equal(res.headers['cache-control'], 'no-store');
      assert.equal(res.body.toString(), INDEX_HTML);
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
        assert.equal(res.headers['x-content-type-options'], 'nosniff');
      });
    }

    test('a percent-encoded /v1/ prefix keeps the 501 envelope', async () => {
      const res = await rawRequest(ENABLED_PORT, '/%76%31/daemon/nope');
      assert.equal(res.status, 501);
      assert.equal(errorCode(res), 'route_not_migrated');
    });

    test('a dot-segment /v1/ form keeps the byte-identical 501 envelope', async () => {
      // `new URL()` collapses `/foo/../v1/...` to `/v1/...` for `matchRoute`,
      // while the raw target does not start with `/v1/`; the exclusion must
      // judge the normalized pathname too or this answers the SPA fallback 200.
      const direct = await rawRequest(ENABLED_PORT, '/v1/daemon/nope');
      const bypass = await rawRequest(ENABLED_PORT, '/foo/../v1/daemon/nope');
      assert.equal(bypass.status, 501);
      assert.equal(errorCode(bypass), 'route_not_migrated');
      assert.equal(envelopeOf(bypass), envelopeOf(direct));
    });

    test('a mixed dot-segment + percent-encoded /v1/ form keeps the byte-identical 501 envelope', async () => {
      // `/foo/../%76%31/...`: `new URL()` collapses `..` but preserves
      // `%76%31`, and the raw target starts with `/foo/..`, so neither view
      // alone sees `/v1/`; the exclusion must decode AND dot-normalize the raw
      // target or this answers the SPA fallback 200. Both mixed forms share
      // the `/%76%31/...` pathname, so their 501 is byte-identical to the
      // encoded-prefix case.
      const encoded = await rawRequest(ENABLED_PORT, '/%76%31/daemon/nope');
      for (const path of ['/foo/../%76%31/daemon/nope', '/foo/%2e%2e/%76%31/daemon/nope']) {
        const res = await rawRequest(ENABLED_PORT, path);
        assert.equal(res.status, 501, `${path} status`);
        assert.equal(errorCode(res), 'route_not_migrated', `${path} code`);
        assert.equal(envelopeOf(res), envelopeOf(encoded), `${path} envelope`);
      }
    });

    test('POST on a mixed dot-segment + percent-encoded /v1/ form keeps the 501 envelope', async () => {
      const res = await rawRequest(ENABLED_PORT, '/foo/../%76%31/daemon/nope', { method: 'POST' });
      assert.equal(res.status, 501);
      assert.equal(errorCode(res), 'route_not_migrated');
    });

    test('an empty-segment + `..` /v1 form keeps the byte-identical 501 envelope', async () => {
      // `path.resolve` ignores duplicate slashes before folding dot segments,
      // so `..` must consume the real parent `a`, never the empty segment:
      // `/a//../%76%31/x` resolves to `<root>/v1/x` in the handler. The guard
      // collapse must agree or this answers the SPA fallback 200. Each pair
      // differs only in dot-segment spelling and shares its pathname, so the
      // 501 bodies are byte-identical.
      const pairs = [
        ['/a//../%76%31/x', '/a//%2e%2e/%76%31/x'],
        ['/a///../%76%31/x', '/a///%2e%2e/%76%31/x'],
      ];
      for (const [path, spelling] of pairs) {
        const res = await rawRequest(ENABLED_PORT, path);
        assert.equal(res.status, 501, `${path} status`);
        assert.equal(errorCode(res), 'route_not_migrated', `${path} code`);
        const twin = await rawRequest(ENABLED_PORT, spelling);
        assert.equal(envelopeOf(res), envelopeOf(twin), `${path} envelope`);
      }
    });

    test('POST on an empty-segment + `..` /v1 form keeps the 501 envelope, not a static 405', async () => {
      const res = await rawRequest(ENABLED_PORT, '/a//../%76%31/x', { method: 'POST' });
      assert.equal(res.status, 501);
      assert.equal(errorCode(res), 'route_not_migrated');
    });

    test('a trailing `.` / `..` /v1 form keeps the byte-identical 501 envelope', async () => {
      // Skipping empty and `.` segments makes `/v1/`, `/v1/.`, `/%76%31/.`
      // and `/%76%31/x/..` collapse to the same `/v1`, which `path.resolve`
      // also resolves to `<root>/v1`; the guard must admit that bare form or
      // the equivalent paths split between the API and static views.
      const encoded = await rawRequest(ENABLED_PORT, '/%76%31/');
      for (const path of ['/%76%31/.', '/%76%31/x/..']) {
        const res = await rawRequest(ENABLED_PORT, path);
        assert.equal(res.status, 501, `${path} status`);
        assert.equal(errorCode(res), 'route_not_migrated', `${path} code`);
        assert.equal(envelopeOf(res), envelopeOf(encoded), `${path} envelope`);
      }
      const direct = await rawRequest(ENABLED_PORT, '/v1/');
      const dotted = await rawRequest(ENABLED_PORT, '/v1/.');
      assert.equal(dotted.status, 501);
      assert.equal(errorCode(dotted), 'route_not_migrated');
      assert.equal(envelopeOf(dotted), envelopeOf(direct));
      const bare = await rawRequest(ENABLED_PORT, '/v1');
      assert.equal(bare.status, 501);
      assert.equal(errorCode(bare), 'route_not_migrated');
    });

    test('a decoded ? dot-segment /v1/ form keeps the byte-identical 501 envelope', async () => {
      // `%3f` decodes to `?`: a filename character for the static handler's
      // `path.resolve`, URL structure for any re-parse. Both views must
      // segment the same string, or `foo?`/`..` collapses into the API domain
      // here while the handler serves the SPA fallback 200 instead.
      const encoded = await rawRequest(ENABLED_PORT, '/%76%31/daemon/nope');
      const res = await rawRequest(ENABLED_PORT, '/foo%3f/../%76%31/daemon/nope');
      assert.equal(res.status, 501);
      assert.equal(errorCode(res), 'route_not_migrated');
      assert.equal(envelopeOf(res), envelopeOf(encoded));
    });

    test('a decoded # dot-segment /v1/ form keeps the byte-identical 501 envelope', async () => {
      // `%23` decodes to `#`: same filename-character contract as `?`.
      const encoded = await rawRequest(ENABLED_PORT, '/%76%31/daemon/nope');
      const res = await rawRequest(ENABLED_PORT, '/foo%23/../%76%31/daemon/nope');
      assert.equal(res.status, 501);
      assert.equal(errorCode(res), 'route_not_migrated');
      assert.equal(envelopeOf(res), envelopeOf(encoded));
    });

    test('POST on a decoded ? dot-segment /v1/ form keeps the 501 envelope', async () => {
      const res = await rawRequest(ENABLED_PORT, '/foo%3f/../%76%31/daemon/nope', { method: 'POST' });
      assert.equal(res.status, 501);
      assert.equal(errorCode(res), 'route_not_migrated');
    });

    test('an absolute-form request target resolves to the same asset', async () => {
      const res = await rawRequest(ENABLED_PORT, `http://127.0.0.1:${ENABLED_PORT}/assets/app-test.js`);
      assert.equal(res.status, 200);
      assert.equal(res.headers['content-type'], 'text/javascript');
      assert.equal(res.body.toString(), APP_JS);
    });

    test(
      'an unreadable asset is a clean 500 envelope, never the asset framing',
      { skip: isRoot },
      async () => {
        const res = await rawRequest(ENABLED_PORT, '/unreadable.txt');
        assert.equal(res.status, 500);
        const parsed = JSON.parse(res.body.toString());
        assert.equal(parsed.success, false);
        assert.equal(parsed.error.code, 'internal');
        assert.equal(parsed.error.message, 'Internal server error');
        const declared = res.headers['content-length'];
        if (declared !== undefined) assert.equal(Number(declared), res.body.length);
      },
    );

    test('an open-success first-read failure is the 500 envelope, never the asset framing', async () => {
      // `open` succeeds here; only the opened file's first read fails, which is
      // the failure QC3-002 flagged as swallowed onto a destroyed response.
      const probeHandle = await openFile(join(root, 'robots.txt'), 'r');
      const fileHandleProto = Object.getPrototypeOf(probeHandle);
      await probeHandle.close();
      mock.method(
        fileHandleProto,
        'createReadStream',
        () => new Readable({ read() { this.destroy(new Error('injected read failure')); } }),
      );
      try {
        const res = await rawRequest(ENABLED_PORT, '/robots.txt');
        assert.equal(res.status, 500);
        const parsed = JSON.parse(res.body.toString());
        assert.equal(parsed.success, false);
        assert.equal(parsed.error.code, 'internal');
        assert.equal(parsed.error.message, 'Internal server error');
        assert.equal(Number(res.headers['content-length']), res.body.length);
        assert.ok(!res.body.toString().includes('User-agent'));
      } finally {
        mock.restoreAll();
      }
    });

    test('POST / is a 405 method_not_allowed envelope', async () => {
      const res = await rawRequest(ENABLED_PORT, '/', { method: 'POST' });
      assert.equal(res.status, 405);
      assert.equal(errorCode(res), 'method_not_allowed');
      assert.equal(res.headers['x-content-type-options'], 'nosniff');
    });

    test('a non-allowlisted Origin still hits the checkOrigin 403', async () => {
      const res = await rawRequest(ENABLED_PORT, '/', {
        headers: { origin: 'https://evil.example' },
      });
      assert.equal(res.status, 403);
      assert.equal(errorCode(res), 'forbidden');
    });
  });

  describe('enabled mode (escaping index.html symlink)', () => {
    let service;

    before(async () => {
      service = await startFixtureService(ESCAPE_PORT, { webDist: escapingIndexFixture() });
    });

    after(async () => {
      if (service) await withTimeout(service.close(), 8_000);
    });

    for (const path of ['/', '/missing-spa-route', '/index.html']) {
      test(`GET ${path} refuses the outside-root fallback with a bare 404`, async () => {
        const res = await rawRequest(ESCAPE_PORT, path);
        assert.equal(res.status, 404);
        assert.equal(errorCode(res), 'not_found');
        assert.ok(!res.body.toString().includes('outside-secret'));
        assert.equal(res.headers['x-content-type-options'], 'nosniff');
      });
    }
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
      const res = new Writable({
        write(_chunk, _encoding, callback) {
          callback();
        },
      });
      res.statusCode = 0;
      res.headers = {};
      res.setHeader = (name, value) => {
        res.headers[name.toLowerCase()] = value;
      };
      return res;
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
