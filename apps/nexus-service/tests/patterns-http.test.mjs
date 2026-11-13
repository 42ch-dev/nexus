import assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { before, describe, test } from 'node:test';

const serviceRoot = join(dirname(fileURLToPath(import.meta.url)), '..');

/**
 * v1.239 P3-T4: `GET /v1/daemon/patterns` serves the read-only package registry
 * listing. The reader's own fixtures prove the scan and the fragment pin; this
 * suite drives the route over the `ServiceCore.core` seam it reads, so the
 * pagination, the typed refusals and the isolation of an unavailable package
 * are the handler's observed behaviour.
 */

let routes;

before(async () => {
  const routesDist = join(serviceRoot, 'dist', 'routes.js');
  assert.ok(
    existsSync(routesDist),
    'dist/routes.js is missing — run `pnpm --filter @42ch/nexus-service run test` (or `… run build`) first',
  );
  routes = await import(join(serviceRoot, 'dist', 'routes.js'));
});

const PATH = '/v1/daemon/patterns';

function routeFor() {
  const route = routes.DOMAIN_ROUTES.find(
    (candidate) => candidate.method === 'GET' && candidate.pattern.test(PATH),
  );
  assert.ok(route, `GET ${PATH} is not mounted`);
  return route;
}

/** Two readable packages and one malformed manifest, as the reader reports them. */
const ITEMS = [
  {
    package_dir: 'beta',
    id: 'beta',
    version: '1.0.0',
    description: 'second package',
    content_hash: 'b'.repeat(64),
    effect_class: 'pure',
    slots: [],
    requires_capabilities: [],
    available: true,
    reason: null,
  },
  {
    package_dir: 'bad-toml',
    id: null,
    version: null,
    description: null,
    content_hash: null,
    effect_class: null,
    slots: [],
    requires_capabilities: [],
    available: false,
    reason: '`pattern.toml` is malformed: expected an equals, found a newline at line 1',
  },
  {
    package_dir: 'alpha',
    id: 'alpha',
    version: '1.0.0',
    description: 'first package',
    content_hash: 'a'.repeat(64),
    effect_class: 'read',
    slots: [{ name: 'topic', kind: 'string', required: true }],
    requires_capabilities: ['search'],
    available: true,
    reason: null,
  },
];

function serviceWith(items) {
  return {
    core: { activePrincipal: async () => ({ handle: 'principal' }), listPatterns: async () => items },
  };
}

const handle = (search, service = serviceWith(ITEMS)) =>
  routeFor().handle(service, [], search ?? new URLSearchParams());

describe('patterns route (v1.239 P3-T4)', () => {
  test('one response carries both valid packages and the malformed one with its reason', async () => {
    const response = await handle();
    assert.deepEqual(
      response.body.items.map((item) => item.package_dir),
      ['alpha', 'bad-toml', 'beta'],
    );

    const byDir = new Map(response.body.items.map((item) => [item.package_dir, item]));
    for (const packageDir of ['alpha', 'beta']) {
      const item = byDir.get(packageDir);
      assert.equal(item.available, true);
      assert.equal(item.reason, null);
      assert.match(item.content_hash, /^[0-9a-f]{64}$/);
    }
    const broken = byDir.get('bad-toml');
    assert.equal(broken.available, false);
    assert.equal(broken.content_hash, null);
    assert.equal(broken.effect_class, null);
    assert.match(broken.reason, /pattern\.toml` is malformed/);

    assert.equal(response.body.pagination.limit, 100);
    assert.equal(response.body.pagination.has_more, false);
    assert.equal(response.body.pagination.next_cursor, undefined);
  });

  test('pages by the last served package_dir', async () => {
    const first = await handle(new URLSearchParams({ limit: '2' }));
    assert.deepEqual(
      first.body.items.map((item) => item.package_dir),
      ['alpha', 'bad-toml'],
    );
    assert.equal(first.body.pagination.limit, 2);
    assert.equal(first.body.pagination.has_more, true);
    assert.equal(first.body.pagination.next_cursor, 'bad-toml');

    const second = await handle(
      new URLSearchParams({ limit: '2', cursor: first.body.pagination.next_cursor }),
    );
    assert.deepEqual(
      second.body.items.map((item) => item.package_dir),
      ['beta'],
    );
    assert.equal(second.body.pagination.has_more, false);
    assert.equal(second.body.pagination.next_cursor, undefined);
  });

  test('an empty registry is an empty listing, not an error', async () => {
    const response = await handle(new URLSearchParams(), serviceWith([]));
    assert.deepEqual(response.body.items, []);
    assert.equal(response.body.pagination.has_more, false);
  });

  test('an out-of-range cursor is a typed 400, never a 500', async () => {
    await assert.rejects(
      () => handle(new URLSearchParams({ cursor: 'v1:42' })),
      (error) => {
        assert.equal(error.status, 400);
        assert.equal(error.code, 'invalid_input');
        return true;
      },
    );
  });

  test('a limit outside the positive integers is a typed 400', async () => {
    for (const limit of ['0', '-1', 'many']) {
      await assert.rejects(
        () => handle(new URLSearchParams({ limit })),
        (error) => {
          assert.equal(error.status, 400);
          assert.equal(error.code, 'invalid_input');
          return true;
        },
      );
    }
  });

  test('refuses an undocumented query key instead of widening the page', async () => {
    await assert.rejects(
      () => handle(new URLSearchParams({ sort: 'version' })),
      (error) => {
        assert.equal(error.status, 400);
        assert.equal(error.code, 'invalid_input');
        return true;
      },
    );
  });
});
