import assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { before, describe, test } from 'node:test';

const serviceRoot = join(dirname(fileURLToPath(import.meta.url)), '..');

/**
 * I-000124 follow-up: `GET/PUT /v1/daemon/agent-host/config` is the HTTP seam
 * over the native agent-host config document. The native core is stubbed at
 * the `ServiceCore.core` boundary the handlers read, so the route semantics
 * (defaults on a missing file, forward + return the saved document) are
 * observed through the real handlers.
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

const PATH = '/v1/daemon/agent-host/config';

function routeFor(method) {
  const route = routes.DOMAIN_ROUTES.find(
    (candidate) => candidate.method === method && candidate.pattern.test(PATH),
  );
  assert.ok(route, `${method} ${PATH} is not mounted`);
  return route;
}

const handle = (method, service, body) =>
  routeFor(method).handle(service, [], new URLSearchParams(), body);

const DEFAULT_CONFIG = {
  max_sessions: 4,
  max_ops_per_session: 1,
  timeouts: { launch_ms: 15000, initialize_ms: 15000, session_ms: 30000, prompt_ms: 180000, shutdown_ms: 5000 },
  policy: { unknown_provider: 'deny', unknown_tool_risk: 'deny', allow_model_fallback: true },
  providers: [],
};

/** A core stub backed by an in-memory document, mirroring the native save/load pair. */
function storeService(
  initial,
  restart = async () => ({ engine_epoch: 7, provider_ready: true }),
) {
  let stored = initial;
  const saves = [];
  const restarts = [];
  return {
    service: {
      workspaceInitialized: true,
      providerReady: false,
      engineEpoch: 0,
      core: {
        getAgentHostConfig: async () => stored,
        saveAgentHostConfig: async (config) => {
          saves.push(config);
          stored = config;
        },
        restartExecutionOwner: async () => {
          restarts.push(stored);
          return restart();
        },
      },
    },
    saves,
    restarts,
  };
}

describe('agent-host config routes (I-000124 follow-up)', () => {
  test('GET returns the native defaults when no config file exists', async () => {
    const { service } = storeService(DEFAULT_CONFIG);
    const response = await handle('GET', service);
    assert.deepEqual(response.body, DEFAULT_CONFIG);
  });

  test('PUT saves the document and returns the persisted config', async () => {
    const { service, saves } = storeService(DEFAULT_CONFIG);
    const request = {
      ...DEFAULT_CONFIG,
      providers: [
        { id: 'claude-acp', protocol: 'acp', command: 'claude-agent-acp', enabled: true },
      ],
    };
    const response = await handle('PUT', service, request);
    assert.deepEqual(response.body, request);
    assert.equal(saves.length, 1);
    assert.deepEqual(saves[0].providers, request.providers);
  });

  test('PUT restarts the engine owner and publishes the fresh readiness facts', async () => {
    const { service, restarts } = storeService(DEFAULT_CONFIG);
    const response = await handle('PUT', service, {
      ...DEFAULT_CONFIG,
      providers: [
        { id: 'claude-acp', protocol: 'acp', command: 'claude-agent-acp', enabled: true },
      ],
    });

    assert.equal(restarts.length, 1, 'the saved config must be applied in-process');
    assert.equal(service.providerReady, true);
    assert.equal(service.engineEpoch, 7);
    assert.equal(response.body.providers[0].id, 'claude-acp');
  });

  test('a restart failure is a typed 503 and keeps the old facts', async () => {
    const { service, saves } = storeService(DEFAULT_CONFIG, async () => {
      throw new Error('owner refused');
    });

    await assert.rejects(
      () =>
        handle('PUT', service, {
          ...DEFAULT_CONFIG,
          providers: [
            { id: 'claude-acp', protocol: 'acp', command: 'claude-agent-acp', enabled: true },
          ],
        }),
      (error) => {
        assert.equal(error.status, 503);
        assert.equal(error.code, 'engine_restart_failed');
        return true;
      },
    );
    // The document was saved, but nothing fabricates readiness.
    assert.equal(saves.length, 1);
    assert.equal(service.providerReady, false);
    assert.equal(service.engineEpoch, 0);
  });

  test('PUT rejects an invalid protocol before reaching the native save path', async () => {
    const { service, saves } = storeService(DEFAULT_CONFIG);
    await assert.rejects(
      () =>
        handle('PUT', service, {
          providers: [{ id: 'bad', protocol: 'shell' }],
        }),
      (error) => {
        assert.equal(error.status, 400);
        assert.equal(error.code, 'invalid_input');
        assert.match(error.message, /protocol must be "acp" or "native_cli"/);
        return true;
      },
    );
    assert.equal(saves.length, 0);
  });

  test('PUT rejects a document without a providers array', async () => {
    const { service } = storeService(DEFAULT_CONFIG);
    await assert.rejects(
      () => handle('PUT', service, { max_sessions: 2 }),
      (error) => {
        assert.equal(error.status, 400);
        assert.equal(error.code, 'invalid_input');
        return true;
      },
    );
  });
});
