import assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { before, describe, test } from 'node:test';

const serviceRoot = join(dirname(fileURLToPath(import.meta.url)), '..');

/**
 * Candidate read route (`GET /v1/daemon/strategies/{strategy_id}/candidate`): the preset family
 * mounts it at the Strategy tier and hands the path id to the core authority, which serves the
 * stored compose row when one exists and the revision-0 import projection otherwise. The store
 * semantics are proven in `crates/nexus-core/tests/strategy_candidate_store.rs` and end to end in
 * `strategy-compose-http.test.mjs`; this target proves the HTTP contract.
 */

let routes;
let HttpError;

before(async () => {
  const routesDist = join(serviceRoot, 'dist', 'routes.js');
  assert.ok(
    existsSync(routesDist),
    'dist/routes.js is missing — run `pnpm --filter @42ch/nexus-service run test` (or `… run build`) first',
  );
  routes = await import(join(serviceRoot, 'dist', 'routes.js'));
  ({ HttpError } = await import(join(serviceRoot, 'dist', 'errors.js')));
});

const SAMPLE_ID = 'novel-writing';
const PATH = `/v1/daemon/strategies/${SAMPLE_ID}/candidate`;

const DOCUMENT = {
  ir_version: 0,
  id: SAMPLE_ID,
  version: 1,
  revision: 3,
  state: 'candidate',
  meta: {
    description: 'composed candidate',
    kind: 'creator',
    run_intents: [],
    requires_capabilities: [],
  },
  entry: 'start',
  terminal: ['done'],
  nodes: [
    { id: 'start', kind: 'outer_state', effect_class: 'pure' },
    { id: 'done', kind: 'terminal', effect_class: 'pure' },
  ],
  edges: [{ id: 'start->done', from: 'start', to: 'done', kind: 'linear' }],
  nests: [],
  assets: [],
  signals: [],
  roles: [],
  effect_summary: 'pure',
  provenance: { source: 'compose' },
};

function routeFor() {
  const route = routes.DOMAIN_ROUTES.find(
    (candidate) => candidate.method === 'GET' && candidate.pattern.test(PATH),
  );
  assert.ok(route, `GET ${PATH} is not mounted`);
  return route;
}

describe('candidate route (GET /v1/daemon/strategies/{id}/candidate)', () => {
  test('mounts on the Strategy tier of the preset family', () => {
    const route = routeFor();
    assert.equal(route.tier, 'tier2');
    assert.equal(route.family, 'presets');
  });

  test('returns the canonical candidate for the path id, revision included', async () => {
    const calls = [];
    const service = {
      core: {
        activePrincipal: async () => 'principal',
        strategyCandidate: async (principal, strategyId) => {
          calls.push({ principal, strategyId });
          return DOCUMENT;
        },
      },
    };

    const response = await routeFor().handle(service, [SAMPLE_ID], new URLSearchParams());

    assert.deepEqual(response.body, DOCUMENT);
    assert.equal(response.body.revision, 3, 'the row revision travels with the document');
    assert.equal(calls.length, 1);
    assert.equal(calls[0].strategyId, SAMPLE_ID);
  });

  test('surfaces the core refusal unchanged', async () => {
    const refusal = new HttpError(404, 'not_found', 'Preset is not installed');
    const service = {
      core: {
        activePrincipal: async () => 'principal',
        strategyCandidate: async () => Promise.reject(refusal),
      },
    };

    await assert.rejects(
      routeFor().handle(service, [SAMPLE_ID], new URLSearchParams()),
      (error) => error === refusal,
    );
  });
});
