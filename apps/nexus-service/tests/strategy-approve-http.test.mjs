import assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { before, describe, test } from 'node:test';

const serviceRoot = join(dirname(fileURLToPath(import.meta.url)), '..');

/**
 * Approve route (`POST /v1/daemon/strategies/{strategy_id}/approve`): the preset family mounts it
 * at the Strategy tier and hands the whole request body — the guarded base revision and the exact
 * simulated trajectory — to the core authority. The core's own approvals, refusals and snapshot
 * immutability are proven in `crates/nexus-core/tests/approve_route.rs`; this target proves the
 * HTTP contract: path id, tier, payload passthrough (including a null trajectory, which the core
 * refuses as missing evidence rather than the adapter rejecting the body) and the untouched
 * refusal envelope.
 */

let routes;

before(async () => {
  const routesDist = join(serviceRoot, 'dist', 'routes.js');
  assert.ok(
    existsSync(routesDist),
    'dist/routes.js is missing — build the service before running this target',
  );
  routes = await import(join(serviceRoot, 'dist', 'routes.js'));
});

const STRATEGY = 'novel-project-init';
const PATH = `/v1/daemon/strategies/${STRATEGY}/approve`;

const TRAJECTORY = {
  schema_version: 1,
  ir_content_hash: 'f'.repeat(64),
  simulation_level: 'reachability',
  nodes_visited: ['done', 'start'],
  predicted_waits: [],
  effect_summary: 'read',
  stubbed_judges: [],
};

const SNAPSHOT = {
  approval_id: 'apr_00000000-0000-4000-8000-000000000000',
  strategy_id: STRATEGY,
  candidate_revision: 3,
  document: { id: STRATEGY, revision: 3, state: 'candidate' },
  content_hash: 'f'.repeat(64),
  trajectory: TRAJECTORY,
  pins: [{ id: 'prompt-step', version: '1.0.0', content_hash: 'a'.repeat(64) }],
  approver: 'test_creator',
  approved_at: '2026-11-13T00:00:00Z',
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
      activePrincipal: async () => 'principal-handle',
      approveStrategy: async (principal, strategyId, request) => {
        calls.push({ principal, strategyId, request });
        return SNAPSHOT;
      },
    },
  };
}

describe('approve route (POST /v1/daemon/strategies/{id}/approve)', () => {
  test('mounts on the Strategy tier of the preset family', () => {
    const route = routeFor();
    assert.equal(route.tier, 'tier2');
    assert.equal(route.family, 'presets');
  });

  test('returns the snapshot and hands the guarded request to the core', async () => {
    const calls = [];
    const response = await routeFor().handle(
      serviceWith(calls),
      [STRATEGY],
      new URLSearchParams(),
      { base_revision: 3, trajectory: TRAJECTORY },
    );

    assert.deepEqual(response.body, SNAPSHOT);
    assert.equal(calls.length, 1);
    assert.equal(calls[0].strategyId, STRATEGY);
    assert.deepEqual(calls[0].request, { base_revision: 3, trajectory: TRAJECTORY });
  });

  test('passes a null trajectory through so the core owns the missing-evidence refusal', async () => {
    const calls = [];
    await routeFor().handle(serviceWith(calls), [STRATEGY], new URLSearchParams(), {
      base_revision: 3,
      trajectory: null,
    });

    assert.equal(calls.length, 1);
    assert.equal(calls[0].request.trajectory, null);
  });

  test('surfaces the core refusal unchanged', async () => {
    // The native boundary rejects with a JSON `CoreError` envelope; the adapter must render its
    // status, code and structured details verbatim.
    const refusal = new Error(
      JSON.stringify({
        code: 'invalid_input',
        message: 'input validation failed',
        details: { code: 'approval_refused', reason: 'pin_changed' },
        http_status: 422,
      }),
    );
    const service = {
      core: {
        activePrincipal: async () => 'principal-handle',
        approveStrategy: async () => Promise.reject(refusal),
      },
    };

    await assert.rejects(
      () =>
        routeFor().handle(service, [STRATEGY], new URLSearchParams(), {
          base_revision: 3,
          trajectory: TRAJECTORY,
        }),
      (error) => {
        assert.equal(error.status, 422);
        assert.equal(error.details.reason, 'pin_changed');
        return true;
      },
    );
  });
});
