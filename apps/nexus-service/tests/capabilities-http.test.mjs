import assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { before, describe, test } from 'node:test';

const serviceRoot = join(dirname(fileURLToPath(import.meta.url)), '..');

/**
 * CAP-001 / I-000125: `GET /v1/daemon/orchestration/capabilities` serves the
 * live registry rows in the hand-coded local wire shape (`camelCase` schemas).
 * The native lane is stubbed at the `ServiceCore.core` seam the handler reads.
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

const PATH = '/v1/daemon/orchestration/capabilities';

function routeFor() {
  const route = routes.DOMAIN_ROUTES.find(
    (candidate) => candidate.method === 'GET' && candidate.pattern.test(PATH),
  );
  assert.ok(route, `GET ${PATH} is not mounted`);
  return route;
}

const ROWS = [
  { name: 'world.query', inputSchema: '{"type":"object"}', outputSchema: '{}', origin: 'builtin' },
  { name: 'archive.export', inputSchema: '{}', outputSchema: '{}', origin: 'user' },
];

function serviceWith(items) {
  return {
    core: { listCapabilities: async () => ({ items }) },
  };
}

const handle = (search, service = serviceWith(ROWS)) =>
  routeFor().handle(service, [], search);

describe('capabilities route (CAP-001 / I-000125)', () => {
  test('returns the registry rows sorted by name with the served wire shape', async () => {
    const response = await handle(new URLSearchParams());
    assert.deepEqual(response.body.items, [ROWS[1], ROWS[0]]);
    assert.deepEqual(response.body.items[0], {
      name: 'archive.export',
      inputSchema: '{}',
      outputSchema: '{}',
      origin: 'user',
    });
    assert.equal(response.body.pagination.limit, 50);
    assert.equal(response.body.pagination.has_more, false);
  });

  test('applies limit and a name cursor', async () => {
    const first = await handle(new URLSearchParams({ limit: '1' }));
    assert.equal(first.body.items.length, 1);
    assert.equal(first.body.items[0].name, 'archive.export');
    assert.equal(first.body.pagination.has_more, true);
    assert.equal(first.body.pagination.next_cursor, 'archive.export');

    const second = await handle(
      new URLSearchParams({ limit: '1', cursor: 'archive.export' }),
    );
    assert.equal(second.body.items[0].name, 'world.query');
    assert.equal(second.body.pagination.has_more, false);
  });

  test('sorts descending with -name', async () => {
    const response = await handle(new URLSearchParams({ sort: '-name' }));
    assert.deepEqual(
      response.body.items.map((item) => item.name),
      ['world.query', 'archive.export'],
    );
  });

  test('rejects an unknown cursor instead of re-serving page one', async () => {
    await assert.rejects(
      () => handle(new URLSearchParams({ cursor: 'missing.capability' })),
      (error) => {
        assert.equal(error.status, 400);
        assert.equal(error.code, 'invalid_input');
        return true;
      },
    );
  });

  test('domain_only profiles answer a typed 501, not route_not_migrated', async () => {
    await assert.rejects(
      () => routeFor().handle({ domainOnly: true }, [], new URLSearchParams()),
      (error) => {
        assert.equal(error.status, 501);
        assert.equal(error.code, 'not_supported');
        assert.match(error.message, /provider-enabled profile/);
        return true;
      },
    );
  });

  test('rejects an unsupported sort key', async () => {
    await assert.rejects(
      () => handle(new URLSearchParams({ sort: 'origin' })),
      (error) => {
        assert.equal(error.status, 400);
        assert.equal(error.code, 'invalid_input');
        return true;
      },
    );
  });
});
