import assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { before, describe, test } from 'node:test';

const serviceRoot = join(dirname(fileURLToPath(import.meta.url)), '..');

/**
 * Apply transport (`POST /v1/daemon/strategies/{strategy_id}/apply` and
 * `GET /v1/daemon/strategies/{strategy_id}/applied`): the preset family mounts both at the
 * Strategy tier and hands the whole request body to the core authority. The apply write, its five
 * refusals, the append-only rows and the `candidate_revision` ordering are proven against a real
 * store in `crates/nexus-core/tests/apply_write.rs`; this target proves the HTTP contract — path
 * id, tier, payload passthrough, the applied identity read back through the status route, the null
 * `current_apply` case, and the refusal status mapping (`already_applied` and
 * `approval_hash_changed` are conflicts, mirroring `strategy_already_approved`; the other three
 * refusals keep the core's client-error status).
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
const APPLY_PATH = `/v1/daemon/strategies/${STRATEGY}/apply`;
const APPLIED_PATH = `/v1/daemon/strategies/${STRATEGY}/applied`;

const APPLIED = {
  apply_id: 'apl_00000000-0000-4000-8000-000000000000',
  approval_id: 'apr_00000000-0000-4000-8000-000000000001',
  content_hash: 'f'.repeat(64),
  applied_at: '2026-11-14T00:00:00Z',
  supersedes_apply_id: null,
};

const NEWEST = {
  approval_id: 'apr_00000000-0000-4000-8000-000000000002',
  content_hash: 'a'.repeat(64),
  candidate_revision: 7,
  approved_at: '2026-11-13T00:00:00Z',
};

function routeFor(method, path) {
  const route = routes.DOMAIN_ROUTES.find(
    (candidate) => candidate.method === method && candidate.pattern.test(path),
  );
  assert.ok(route, `${method} ${path} is not mounted`);
  return route;
}

function serviceWith(calls) {
  return {
    core: {
      activePrincipal: async () => 'principal-handle',
      applyStrategy: async (principal, strategyId, request) => {
        calls.push({ principal, strategyId, request });
        return APPLIED;
      },
      strategyApplied: async (principal, strategyId) => {
        calls.push({ principal, strategyId });
        return { current_apply: APPLIED, newest_approval: NEWEST };
      },
    },
  };
}

/** A native rejection: the wire `CoreError` envelope with the refusal inside `details`. */
function refusal(reason) {
  return new Error(
    JSON.stringify({
      code: 'invalid_input',
      message: 'input validation failed',
      details: { code: 'apply_refused', reason },
      http_status: 422,
    }),
  );
}

function refusingService(reason) {
  return {
    core: {
      activePrincipal: async () => 'principal-handle',
      applyStrategy: async () => Promise.reject(refusal(reason)),
    },
  };
}

const BODY = { approval_content_hash: APPLIED.content_hash };

describe('apply transport (POST /v1/daemon/strategies/{id}/apply)', () => {
  test('mounts on the Strategy tier of the preset family', () => {
    const route = routeFor('POST', APPLY_PATH);
    assert.equal(route.tier, 'tier2');
    assert.equal(route.family, 'presets');
  });

  test('returns the applied identity and hands the content hash to the core', async () => {
    const calls = [];
    const response = await routeFor('POST', APPLY_PATH).handle(
      serviceWith(calls),
      [STRATEGY],
      new URLSearchParams(),
      BODY,
    );

    assert.deepEqual(response.body, APPLIED);
    assert.equal(calls.length, 1);
    assert.equal(calls[0].strategyId, STRATEGY);
    assert.deepEqual(calls[0].request, BODY);
  });

  test('reads the applied identity back through the status route', async () => {
    const calls = [];
    const service = serviceWith(calls);
    const applied = await routeFor('POST', APPLY_PATH).handle(
      service,
      [STRATEGY],
      new URLSearchParams(),
      BODY,
    );
    const status = await routeFor('GET', APPLIED_PATH).handle(
      service,
      [STRATEGY],
      new URLSearchParams(),
      undefined,
    );

    assert.deepEqual(status.body.current_apply, applied.body);
    assert.deepEqual(status.body.newest_approval, NEWEST);
  });

  test('a repeat apply is the mapped conflict, not a 500', async () => {
    await assert.rejects(
      () =>
        routeFor('POST', APPLY_PATH).handle(
          refusingService('already_applied'),
          [STRATEGY],
          new URLSearchParams(),
          BODY,
        ),
      (error) => {
        assert.equal(error.status, 409);
        assert.equal(error.code, 'invalid_input');
        assert.equal(error.details.reason, 'already_applied');
        return true;
      },
    );
  });

  test('changed approval bytes are the mapped conflict', async () => {
    await assert.rejects(
      () =>
        routeFor('POST', APPLY_PATH).handle(
          refusingService('approval_hash_changed'),
          [STRATEGY],
          new URLSearchParams(),
          BODY,
        ),
      (error) => {
        assert.equal(error.status, 409);
        assert.equal(error.details.reason, 'approval_hash_changed');
        return true;
      },
    );
  });

  test('the remaining refusals keep the core client-error status', async () => {
    for (const reason of [
      'approval_not_found',
      'approval_strategy_mismatch',
      'strategy_has_no_schedule',
    ]) {
      await assert.rejects(
        () =>
          routeFor('POST', APPLY_PATH).handle(
            refusingService(reason),
            [STRATEGY],
            new URLSearchParams(),
            BODY,
          ),
        (error) => {
          assert.equal(error.status, 422, reason);
          assert.equal(error.details.reason, reason);
          return true;
        },
      );
    }
  });
});

describe('applied status (GET /v1/daemon/strategies/{id}/applied)', () => {
  test('mounts on the Strategy tier of the preset family', () => {
    const route = routeFor('GET', APPLIED_PATH);
    assert.equal(route.tier, 'tier2');
    assert.equal(route.family, 'presets');
  });

  test('returns the current apply and the newest approval unchanged', async () => {
    const calls = [];
    const response = await routeFor('GET', APPLIED_PATH).handle(
      serviceWith(calls),
      [STRATEGY],
      new URLSearchParams(),
      undefined,
    );

    assert.deepEqual(response.body, { current_apply: APPLIED, newest_approval: NEWEST });
    assert.deepEqual(calls, [{ principal: 'principal-handle', strategyId: STRATEGY }]);
  });

  test('a strategy with no apply answers null instead of a refusal', async () => {
    const service = {
      core: {
        activePrincipal: async () => 'principal-handle',
        strategyApplied: async () => ({ current_apply: null, newest_approval: null }),
      },
    };
    const response = await routeFor('GET', APPLIED_PATH).handle(
      service,
      [STRATEGY],
      new URLSearchParams(),
      undefined,
    );

    assert.deepEqual(response.body, { current_apply: null, newest_approval: null });
  });
});
