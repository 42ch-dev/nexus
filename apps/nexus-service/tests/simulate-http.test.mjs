import assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { before, describe, test } from 'node:test';

const serviceRoot = join(dirname(fileURLToPath(import.meta.url)), '..');

/**
 * Simulate route (`POST /v1/daemon/strategies/{strategy_id}/simulate`): the preset family
 * mounts it, admits it at the Strategy tier and hands the whole candidate body to the core
 * authority. The core's own static walk is proven in
 * `crates/nexus-orchestration/tests/sample_trajectory.rs`; this target proves the HTTP
 * contract — path id, tier, payload passthrough and the untouched core payload.
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

const SAMPLE_ID = 'game-narrative';
const PATH = `/v1/daemon/strategies/${SAMPLE_ID}/simulate`;

/** The `game-narrative` candidate: one judged state with a declared `default` edge. */
const CANDIDATE = {
  ir_version: 0,
  id: SAMPLE_ID,
  version: 1,
  revision: 0,
  state: 'candidate',
  meta: {
    description: 'game narrative import',
    kind: 'creator',
    run_intents: ['knowledge_ingest'],
    requires_capabilities: ['judge.llm'],
  },
  entry: 'routing',
  terminal: ['done'],
  nodes: [
    {
      id: 'routing',
      kind: 'outer_state',
      effect_class: 'external',
      exit_when: { kind: 'llm_judge', judge_capability: 'judge.llm' },
    },
    { id: 'trigger_extract', kind: 'outer_state', effect_class: 'external' },
    { id: 'done', kind: 'terminal', effect_class: 'pure' },
  ],
  edges: [
    { id: 'routing->trigger_extract', from: 'routing', to: 'trigger_extract', kind: 'default' },
    { id: 'trigger_extract->done', from: 'trigger_extract', to: 'done', kind: 'linear' },
  ],
  nests: [],
  assets: [],
  signals: [{ name: 'game_event', action: 'force_transition', target: 'trigger_extract' }],
  roles: [],
  effect_summary: 'external',
  provenance: { source: 'import_preset' },
};

const TRAJECTORY = {
  schema_version: 1,
  ir_content_hash: 'f'.repeat(64),
  simulation_level: 'reachability',
  nodes_visited: ['done', 'routing', 'trigger_extract'],
  predicted_waits: [],
  effect_summary: 'external',
  stubbed_judges: ['routing'],
};

function routeFor() {
  const route = routes.DOMAIN_ROUTES.find(
    (candidate) => candidate.method === 'POST' && candidate.pattern.test(PATH),
  );
  assert.ok(route, `POST ${PATH} is not mounted`);
  return route;
}

function serviceWith(calls) {
  return {
    core: {
      simulateStrategy: async (principal, strategyId, request) => {
        calls.push({ principal, strategyId, request });
        return TRAJECTORY;
      },
    },
  };
}

describe('simulate route (POST /v1/daemon/strategies/{id}/simulate)', () => {
  test('mounts on the Strategy tier of the preset family', () => {
    const route = routeFor();
    assert.equal(route.tier, 'tier2');
    assert.equal(route.family, 'presets');
  });

  test('returns the trajectory and hands the whole candidate to the core', async () => {
    const calls = [];
    const response = await routeFor().handle(
      serviceWith(calls),
      [SAMPLE_ID],
      new URLSearchParams(),
      { document: CANDIDATE },
    );

    assert.deepEqual(response.body, TRAJECTORY);
    assert.equal(calls.length, 1);
    assert.equal(calls[0].strategyId, SAMPLE_ID);
    assert.deepEqual(calls[0].request, { document: CANDIDATE });
  });

  test('surfaces the core refusal unchanged', async () => {
    const refusal = Object.assign(new Error('invalid input: document'), {
      status: 400,
      code: 'invalid_input',
    });
    const service = { core: { simulateStrategy: async () => Promise.reject(refusal) } };

    await assert.rejects(
      () =>
        routeFor().handle(service, [SAMPLE_ID], new URLSearchParams(), { document: CANDIDATE }),
      (error) => {
        assert.equal(error.status, 400);
        assert.equal(error.code, 'invalid_input');
        return true;
      },
    );
  });
});
