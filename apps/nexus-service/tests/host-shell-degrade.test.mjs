import assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { before, describe, test } from 'node:test';

const serviceRoot = join(dirname(fileURLToPath(import.meta.url)), '..');

/**
 * I-000123: the service-only shell (a fresh home opened with
 * `allow_uninitialized`) has no Host authority until a workspace selection
 * promotes the core. The onboarding wizard calls the tier1 Agent-Host routes
 * BEFORE that promotion, so their native `uninitialized` rejection must degrade
 * to the same empty readiness bodies an absent host already produces — never a
 * 409 that deadlocks first-run setup. After promotion the full core exists and
 * the same rejection must still surface.
 *
 * The authority is stubbed at the `ServiceCore.core.hostQuery` seam the
 * handlers read, so the degradation is observed through the real route
 * handlers and their HTTP-visible bodies.
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

/** The native lifecycle's wire envelope for the uninitialized shell. */
const uninitializedEnvelope = {
  code: 'uninitialized',
  message: 'workspace not initialized',
  http_status: 409,
};

/** A ServiceCore stub carrying only what the tier1 host handlers read. */
function shellService(workspaceInitialized) {
  return {
    workspaceInitialized,
    core: {
      hostQuery: async () => {
        throw new Error(JSON.stringify(uninitializedEnvelope));
      },
    },
  };
}

const SCAN = { method: 'POST', path: '/v1/daemon/agent-host/scan' };

const TIER1 = [
  {
    method: 'GET',
    path: '/v1/daemon/agent-host/health',
    degraded: { running: false, active_sessions: 0, active_operations: 0 },
  },
  { method: 'GET', path: '/v1/daemon/agent-host/providers', degraded: { providers: [] } },
  { ...SCAN, degraded: { agents: [] } },
];

function routeFor({ method, path }) {
  const route = routes.DOMAIN_ROUTES.find(
    (candidate) => candidate.method === method && candidate.pattern.test(path),
  );
  assert.ok(route, `${method} ${path} is not mounted`);
  return route;
}

function handle(tier1, service) {
  return routeFor(tier1).handle(service, [], new URLSearchParams());
}

describe('tier1 host queries on the uninitialized shell (I-000123)', () => {
  test('degrade to empty readiness bodies instead of 409', async () => {
    for (const tier1 of TIER1) {
      const response = await handle(tier1, shellService(false));
      assert.deepEqual(response.body, tier1.degraded, `${tier1.method} ${tier1.path}`);
    }
  });

  test('a promoted host publishes native scan entries under the wire `agents` field', async () => {
    const entry = {
      name: 'claude-acp',
      registry_agent_id: 'claude-acp',
      launch_command: 'claude-acp',
      installed: true,
      version: '1.2.3',
    };
    const service = {
      workspaceInitialized: true,
      core: { hostQuery: async () => ({ scan: { entries: [entry] } }) },
    };
    const response = await handle(SCAN, service);
    assert.deepEqual(response.body, { agents: [entry] });
  });

  test('after promotion the same rejection still surfaces as the mapped 409', async () => {
    for (const tier1 of TIER1) {
      await assert.rejects(
        () => handle(tier1, shellService(true)),
        (error) => {
          assert.equal(error.status, 409);
          assert.equal(error.code, 'uninitialized');
          return true;
        },
        `${tier1.method} ${tier1.path}`,
      );
    }
  });
});
