import assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { before, describe, test } from 'node:test';

const serviceRoot = join(dirname(fileURLToPath(import.meta.url)), '..');

/**
 * Workflow-IR read route (`GET /v1/daemon/strategies/{strategy_id}/workflow-ir`): the preset
 * family mounts it at the Strategy tier and hands the path id to the core authority, which
 * projects the installed preset through the existing `nexus-preset` import adapter. The core
 * resolution and the schema round-trip are proven in
 * `crates/nexus-core/tests/simulation_services.rs`; this target proves the HTTP contract.
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
const PATH = `/v1/daemon/strategies/${SAMPLE_ID}/workflow-ir`;

const DOCUMENT = {
  ir_version: 0,
  id: SAMPLE_ID,
  version: 1,
  revision: 0,
  state: 'candidate',
  meta: {
    description: 'installed strategy',
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
  provenance: { source: 'import_preset' },
};

function routeFor() {
  const route = routes.DOMAIN_ROUTES.find(
    (candidate) => candidate.method === 'GET' && candidate.pattern.test(PATH),
  );
  assert.ok(route, `GET ${PATH} is not mounted`);
  return route;
}

describe('workflow-ir route (GET /v1/daemon/strategies/{id}/workflow-ir)', () => {
  test('mounts on the Strategy tier of the preset family', () => {
    const route = routeFor();
    assert.equal(route.tier, 'tier2');
    assert.equal(route.family, 'presets');
  });

  test('returns the core document for the path id', async () => {
    const calls = [];
    const service = {
      core: {
        activePrincipal: async () => 'principal',
        strategyWorkflowIr: async (principal, strategyId) => {
          calls.push({ principal, strategyId });
          return DOCUMENT;
        },
      },
    };

    const response = await routeFor().handle(service, [SAMPLE_ID], new URLSearchParams());

    assert.deepEqual(response.body, DOCUMENT);
    assert.equal(calls.length, 1);
    assert.equal(calls[0].strategyId, SAMPLE_ID);
  });

  test('surfaces the core refusal unchanged', async () => {
    const refusal = new HttpError(404, 'not_found', 'Preset is not installed');
    const service = {
      core: {
        activePrincipal: async () => 'principal',
        strategyWorkflowIr: async () => Promise.reject(refusal),
      },
    };

    await assert.rejects(
      () => routeFor().handle(service, [SAMPLE_ID], new URLSearchParams()),
      (error) => {
        assert.equal(error.status, 404);
        assert.equal(error.code, 'not_found');
        return true;
      },
    );
  });
});
