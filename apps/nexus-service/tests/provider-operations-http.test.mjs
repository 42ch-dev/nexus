import assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { before, describe, test } from 'node:test';

const serviceRoot = join(dirname(fileURLToPath(import.meta.url)), '..');
const SESSION = '9f9619ff-8b86-4d11-b42d-00cf4fc964ff';

/**
 * CAP-007 / I-000131: `POST /v1/daemon/agent-host/sessions/{id}/operations`
 * serves all three generated branches. `set_model`/`set_mode` used to answer
 * 501; they now dispatch on the provider lane (HostOperation wire payload) or
 * forward unchanged to the native authority for Actor-backed sessions.
 */

let routes;
let ProviderRegistry;

before(async () => {
  const routesDist = join(serviceRoot, 'dist', 'routes.js');
  assert.ok(
    existsSync(routesDist),
    'dist/routes.js is missing — run `pnpm --filter @42ch/nexus-service run test` (or `… run build`) first',
  );
  routes = await import(join(serviceRoot, 'dist', 'routes.js'));
  ({ ProviderRegistry } = await import(join(serviceRoot, 'dist', 'provider-registry.js')));
});

function serviceFor({
  nativeSession = null,
  providerCall = async () => {
    throw new Error('providerCall not stubbed');
  },
  hostExecuteOperation = async () => {
    throw new Error('hostExecuteOperation not stubbed');
  },
}) {
  const registry = new ProviderRegistry();
  registry.registerSession({
    sessionId: SESSION,
    providerId: 'mock-acp',
    state: 'Ready',
    activeOpId: null,
  });
  return {
    domainOnly: false,
    providerRegistry: registry,
    core: {
      activePrincipal: async () => 'p:1:ctr_x:default',
      hostQuery: async () => (nativeSession ? { session: nativeSession } : {}),
      providerCall,
      hostExecuteOperation,
    },
  };
}

function operationsRoute() {
  const pattern = `/v1/daemon/agent-host/sessions/${SESSION}/operations`;
  const route = routes.DOMAIN_ROUTES.find(
    (candidate) => candidate.method === 'POST' && candidate.pattern.test(pattern),
  );
  assert.ok(route, 'POST operations route is not mounted');
  return route;
}

const execute = (service, body) =>
  operationsRoute().handle(service, [SESSION], new URLSearchParams(), body);

describe('agent-host session operations (CAP-007 / I-000131)', () => {
  test('set_model dispatches the SetModel HostOperation on the provider lane', async () => {
    const calls = [];
    const service = serviceFor({
      providerCall: async (request) => {
        calls.push(request);
        return { ok: true, operation_id: 'op-set-model', session_id: SESSION };
      },
    });

    const response = await execute(service, { kind: 'set_model', model: 'claude-opus' });

    assert.equal(calls.length, 1);
    assert.deepEqual(calls[0].payload, { SetModel: { model: 'claude-opus' } });
    assert.equal(response.body.operation_id, 'op-set-model');
  });

  test('set_mode dispatches the SetMode HostOperation on the provider lane', async () => {
    const calls = [];
    const service = serviceFor({
      providerCall: async (request) => {
        calls.push(request);
        return { ok: true, operation_id: 'op-set-mode', session_id: SESSION };
      },
    });

    const response = await execute(service, { kind: 'set_mode', mode: 'architect' });

    assert.deepEqual(calls[0].payload, { SetMode: { mode: 'architect' } });
    assert.equal(response.body.operation_id, 'op-set-mode');
  });

  test('an Actor-backed session forwards set_model to the native authority unchanged', async () => {
    const requests = [];
    const service = serviceFor({
      nativeSession: {
        session_id: SESSION,
        provider_id: 'mock-acp',
        state: 'Ready',
        actor_ref: { actor_id: 'act_1', viewpoint: 'actor' },
      },
      hostExecuteOperation: async (_principal, _sessionId, request) => {
        requests.push(request);
        return { operation_id: 'op-actor', session_id: SESSION, status: 'started' };
      },
    });

    const response = await execute(service, { kind: 'set_model', model: 'claude-opus' });

    assert.deepEqual(requests, [{ kind: 'set_model', model: 'claude-opus' }]);
    assert.equal(response.body.operation_id, 'op-actor');
  });

  test('a native rejection keeps its typed wire mapping', async () => {
    const service = serviceFor({
      providerCall: async () => {
        throw new Error(
          JSON.stringify({
            code: 'busy',
            message: 'provider is busy',
            details: {},
            http_status: 503,
          }),
        );
      },
    });

    await assert.rejects(
      () => execute(service, { kind: 'set_model', model: 'claude-opus' }),
      (error) => {
        assert.equal(error.status, 503);
        assert.equal(error.code, 'busy');
        return true;
      },
    );
  });

  test('a malformed set_model branch is 400 before any provider effect', async () => {
    let called = false;
    const service = serviceFor({
      providerCall: async () => {
        called = true;
        return { ok: true };
      },
    });

    await assert.rejects(
      () => execute(service, { kind: 'set_model' }),
      (error) => {
        assert.equal(error.status, 400);
        assert.equal(error.code, 'invalid_input');
        return true;
      },
    );
    assert.equal(called, false);
  });

  test('domain_only profiles answer a typed 501, not route_not_migrated', async () => {
    const route = routes.DOMAIN_ROUTES.find(
      (candidate) =>
        candidate.method === 'POST' && candidate.pattern.test('/v1/daemon/agent-host/sessions'),
    );
    assert.ok(route, 'POST sessions route is not mounted');
    const service = {
      domainOnly: true,
      providerRegistry: new ProviderRegistry(),
      core: {},
    };

    await assert.rejects(
      () => route.handle(service, [], new URLSearchParams(), { provider_id: 'mock-acp' }),
      (error) => {
        assert.equal(error.status, 501);
        assert.equal(error.code, 'not_supported');
        assert.match(error.message, /provider-enabled profile/);
        return true;
      },
    );
  });
});
