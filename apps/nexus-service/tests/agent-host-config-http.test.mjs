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
let workspaces;

before(async () => {
  const routesDist = join(serviceRoot, 'dist', 'routes.js');
  assert.ok(
    existsSync(routesDist),
    'dist/routes.js is missing — run `pnpm --filter @42ch/nexus-service run test` (or `… run build`) first',
  );
  routes = await import(join(serviceRoot, 'dist', 'routes.js'));
  workspaces = await import(join(serviceRoot, 'dist', 'workspaces.js'));
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

  test('two overlapping PUTs apply in order and both restart', async () => {
    let stored = { ...DEFAULT_CONFIG };
    const restarts = [];
    const service = {
      workspaceInitialized: true,
      providerReady: false,
      engineEpoch: 0,
      core: {
        getAgentHostConfig: async () => stored,
        saveAgentHostConfig: async (config) => {
          await new Promise((resolve) => setTimeout(resolve, 5));
          stored = config;
        },
        restartExecutionOwner: async () => {
          restarts.push(stored.providers.map((provider) => provider.id));
          await new Promise((resolve) => setTimeout(resolve, 5));
          return { engine_epoch: restarts.length, provider_ready: true };
        },
      },
    };
    const first = {
      ...DEFAULT_CONFIG,
      providers: [{ id: 'first-acp', protocol: 'acp', command: 'first-agent', enabled: true }],
    };
    const second = {
      ...DEFAULT_CONFIG,
      providers: [...first.providers, { id: 'second-acp', protocol: 'acp', command: 'second-agent', enabled: true }],
    };

    const [a, b] = await Promise.all([
      handle('PUT', service, first),
      handle('PUT', service, second),
    ]);

    assert.equal(a.body.providers.length, 1, 'the first response precedes the second write');
    assert.equal(b.body.providers.length, 2);
    assert.deepEqual(
      stored.providers.map((provider) => provider.id),
      ['first-acp', 'second-acp'],
    );
    // Each restart saw the document its own save had just written.
    assert.deepEqual(restarts, [['first-acp'], ['first-acp', 'second-acp']]);
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

  test('PUT enforces the wire schema constraints per field', async () => {
    const cases = [
      [{ ...DEFAULT_CONFIG, max_sessions: 0 }, /max_sessions/],
      [{ ...DEFAULT_CONFIG, max_sessions: 1.5 }, /max_sessions/],
      [{ ...DEFAULT_CONFIG, max_ops_per_session: -1 }, /max_ops_per_session/],
      [
        { ...DEFAULT_CONFIG, timeouts: { ...DEFAULT_CONFIG.timeouts, launch_ms: 0 } },
        /timeouts\.launch_ms/,
      ],
      [
        { ...DEFAULT_CONFIG, timeouts: { ...DEFAULT_CONFIG.timeouts, unknown_ms: 1 } },
        /unknown field 'unknown_ms' at timeouts/,
      ],
      [
        { ...DEFAULT_CONFIG, policy: { ...DEFAULT_CONFIG.policy, unknown_provider: 'maybe' } },
        /policy\.unknown_provider/,
      ],
      [
        { ...DEFAULT_CONFIG, policy: { ...DEFAULT_CONFIG.policy, unknown_tool_risk: 'maybe' } },
        /policy\.unknown_tool_risk/,
      ],
      [
        { ...DEFAULT_CONFIG, policy: { ...DEFAULT_CONFIG.policy, allow_model_fallback: 'yes' } },
        /policy\.allow_model_fallback/,
      ],
      [{ ...DEFAULT_CONFIG, extra: true }, /unknown field 'extra' at config/],
      [
        {
          ...DEFAULT_CONFIG,
          providers: [{ id: 'claude-acp', protocol: 'acp', command: 'x', shell: true }],
        },
        /unknown field 'shell' at providers\[0\]/,
      ],
      [
        {
          ...DEFAULT_CONFIG,
          providers: [{ id: 'claude-acp', protocol: 'acp', command: 'x', args: ['ok', 1] }],
        },
        /providers\[0\]\.args/,
      ],
    ];
    for (const [body, pattern] of cases) {
      const { service, saves } = storeService(DEFAULT_CONFIG);
      await assert.rejects(
        () => handle('PUT', service, body),
        (error) => {
          assert.equal(error.status, 400, JSON.stringify(body));
          assert.equal(error.code, 'invalid_input');
          assert.match(error.message, pattern);
          return true;
        },
      );
      assert.equal(saves.length, 0, 'an invalid document never reaches the native save path');
    }
  });

  test('a PUT and a workspace selection never interleave on the owner lifecycle', async () => {
    const log = [];
    const pause = () => new Promise((resolve) => setTimeout(resolve, 10));
    const service = {
      workspaceInitialized: true,
      providerReady: false,
      engineEpoch: 0,
      creatorId: null,
      workspaceSlug: null,
      core: {
        selectWorkspace: async () => {
          log.push('select:start');
          await pause();
          log.push('select:end');
          return { creator_id: 'ctr_x', workspace_slug: 'default' };
        },
        getAgentHostConfig: async () => {
          log.push('config:get');
          return DEFAULT_CONFIG;
        },
        saveAgentHostConfig: async () => {
          log.push('save:start');
          await pause();
          log.push('save:end');
        },
        restartExecutionOwner: async () => {
          log.push('restart:start');
          await pause();
          log.push('restart:end');
          return { engine_epoch: 1, provider_ready: true };
        },
      },
    };

    await Promise.all([
      handle('PUT', service, {
        ...DEFAULT_CONFIG,
        providers: [{ id: 'claude-acp', protocol: 'acp', command: 'claude-agent-acp', enabled: true }],
      }),
      workspaces.setActiveWorkspace(service, {
        creator_id: 'ctr_x',
        workspace_slug: 'default',
      }),
    ]);

    // ONE queue: the save+restart completes before the selection starts.
    assert.deepEqual(log, [
      'save:start',
      'save:end',
      'restart:start',
      'restart:end',
      'config:get',
      'select:start',
      'select:end',
    ]);
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
