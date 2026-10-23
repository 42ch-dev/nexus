#!/usr/bin/env node
/**
 * Task 4 — workspace family exposure (list + active get/set), extended by
 * v1220 P3-T2 with create.
 *
 * `POST /v1/daemon/workspaces` forwards to the core create capability; the
 * core's typed refusals map through the shared error fold (`workspace_conflict`
 * → 409, `invalid_input` → 400). The real composer and HTTP server run over a
 * stub core here: route identity, contract shape, retained query semantics and
 * the in-place runtime identity refresh are the transport behaviour under test
 * (core create semantics are covered by
 * `crates/nexus-core/tests/workspace_create.rs`).
 */
import assert from 'node:assert/strict';
import http from 'node:http';
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import test, { after, before, describe } from 'node:test';

const serviceRoot = join(dirname(fileURLToPath(import.meta.url)), '..');
const tempDirs = [];

after(() => {
  for (const dir of tempDirs) rmSync(dir, { recursive: true, force: true });
});

const WORKSPACES = [
  {
    creator_id: 'ctr_a',
    workspace_slug: 'default',
    creative_root: '/tmp/nexus-a-default',
    display_name: '默认工作区',
  },
  { creator_id: 'ctr_a', workspace_slug: 'second', creative_root: '/tmp/nexus-a-second' },
  { creator_id: 'ctr_b', workspace_slug: 'default', creative_root: '/tmp/nexus-b-default' },
];

function tempHome() {
  const home = mkdtempSync(join(tmpdir(), 'nexus-workspace-http-'));
  tempDirs.push(home);
  return home;
}

function jsonFetch(url, { method = 'GET', body } = {}) {
  return new Promise((resolveRequest, rejectRequest) => {
    const req = http.request(url, { method }, (res) => {
      const chunks = [];
      res.on('data', (chunk) => chunks.push(chunk));
      res.on('end', () => {
        const text = Buffer.concat(chunks).toString('utf8');
        resolveRequest({
          status: res.statusCode,
          payload: text.length > 0 ? JSON.parse(text) : null,
          text,
        });
      });
    });
    req.on('error', rejectRequest);
    if (body !== undefined) req.write(JSON.stringify(body));
    req.end();
  });
}

async function startServer(stubState) {
  const previousKey = process.env.NEXUS42_DAEMON_API_KEY;
  delete process.env.NEXUS42_DAEMON_API_KEY;
  try {
    const { resolveServiceConfig } = await import(join(serviceRoot, 'dist/config.js'));
    const { createServiceServer, listenServer } = await import(join(serviceRoot, 'dist/server.js'));

    const config = resolveServiceConfig({
      home: tempHome(),
      host: '127.0.0.1',
      port: 0,
      allowRemote: false,
    });
    // Boot snapshot semantics: the shell starts without an initialized
    // workspace and only the set-active handler may flip it in place.
    const service = {
      startedAt: new Date().toISOString(),
      tlsFingerprint: null,
      domainOnly: true,
      providerReady: false,
      workspaceInitialized: false,
      creatorId: null,
      workspaceSlug: null,
      core: stubState.core,
    };
    const created = createServiceServer(config, service, async () => ({
      state: 'closed',
      cleanup_confirmed: true,
      pending_operations: [],
    }));
    const endpoint = await listenServer(created.server, config);
    return {
      service,
      url: endpoint.url,
      close: () =>
        new Promise((resolveClose) => {
          created.server.closeAllConnections();
          created.server.close(() => resolveClose());
        }),
    };
  } finally {
    if (previousKey === undefined) delete process.env.NEXUS42_DAEMON_API_KEY;
    else process.env.NEXUS42_DAEMON_API_KEY = previousKey;
  }
}

function stubState() {
  const state = {
    activeSlug: 'default',
    selected: null,
    createdRequest: null,
    createdItems: [],
    promotions: 0,
    failPromotion: false,
    core: {
      async listWorkspaces() {
        const items = [...WORKSPACES, ...state.createdItems];
        return {
          items: items.map((item) => ({ ...item })),
          pagination: { limit: items.length, has_more: false, next_cursor: null },
        };
      },
      async getActiveWorkspace() {
        const active =
          WORKSPACES.find((item) => item.workspace_slug === state.activeSlug) ?? WORKSPACES[0];
        return {
          creator_id: active.creator_id,
          workspace_slug: active.workspace_slug,
          creative_root: active.creative_root,
          operational_dir: `/tmp/home/.nexus42/creators/${active.creator_id}/workspaces/${active.workspace_slug}`,
        };
      },
      async selectWorkspace(request) {
        state.activeSlug = request.workspace_slug;
        state.selected = request;
        return {
          creator_id: request.creator_id ?? 'ctr_a',
          workspace_slug: request.workspace_slug,
        };
      },
      // The core owns validation/exclusivity; this stub reproduces its typed
      // refusals so the transport fold (409/400) is what gets exercised.
      async createWorkspace(request) {
        state.createdRequest = request;
        if (request.workspace_slug === 'duplicate') {
          throw new Error(
            JSON.stringify({
              code: 'workspace_conflict',
              message: "workspace conflict: workspace 'duplicate' already exists",
              http_status: 409,
            }),
          );
        }
        if (request.workspace_slug === 'invalid') {
          throw new Error(
            JSON.stringify({
              code: 'invalid_input',
              message: 'invalid input: workspace_slug — must be a single path segment',
              details: { field: 'workspace_slug', reason: 'must be a single path segment' },
              http_status: 400,
            }),
          );
        }
        const creativeRoot = `/tmp/home/Nexus/${request.workspace_slug}`;
        state.createdItems.push({
          creator_id: request.creator_id,
          workspace_slug: request.workspace_slug,
          creative_root: creativeRoot,
          ...(request.display_name === undefined
            ? {}
            : { display_name: request.display_name }),
        });
        return {
          creator_id: request.creator_id,
          workspace_slug: request.workspace_slug,
          creative_root: creativeRoot,
          operational_dir: `/tmp/home/.nexus42/creators/${request.creator_id}/workspaces/${request.workspace_slug}`,
          state_db_path: `/tmp/home/.nexus42/creators/${request.creator_id}/workspaces/${request.workspace_slug}/state.db`,
        };
      },
      async promoteCore() {
        state.promotions += 1;
        if (state.failPromotion) throw new Error('promotion refused');
      },
    },
  };
  return state;
}

describe('workspace-http (v1219 shell Task 4)', () => {
  let server;
  let state;

  before(async () => {
    const build = spawnSync('npx', ['tsc', '-p', 'tsconfig.json'], {
      cwd: serviceRoot,
      stdio: 'inherit',
    });
    assert.equal(build.status, 0, 'service build failed');
    state = stubState();
    server = await startServer(state);
  });

  after(async () => {
    if (server) await server.close();
  });

  test('the family is mounted at exactly list + create + active get/set', async () => {
    const { DOMAIN_ROUTES } = await import(join(serviceRoot, 'dist/routes.js'));
    const mounted = DOMAIN_ROUTES.filter((route) => route.family === 'workspaces').map(
      (route) => `${route.method} ${route.pattern.source}`,
    );
    assert.deepEqual(mounted, [
      'GET ^\\/v1\\/daemon\\/workspaces$',
      'POST ^\\/v1\\/daemon\\/workspaces$',
      'GET ^\\/v1\\/daemon\\/workspace$',
      'POST ^\\/v1\\/daemon\\/workspace\\/active$',
    ]);
  });

  test('list returns the frozen contract with the retained query semantics', async () => {
    const all = await jsonFetch(`${server.url}/v1/daemon/workspaces`);
    assert.equal(all.status, 200, all.text);
    assert.deepEqual(all.payload.items, WORKSPACES);
    assert.deepEqual(all.payload.pagination, { limit: 50, has_more: false });

    const filtered = await jsonFetch(`${server.url}/v1/daemon/workspaces?creator_id=ctr_a`);
    assert.equal(filtered.status, 200, filtered.text);
    assert.equal(filtered.payload.items.length, 2);
    assert.equal(
      filtered.payload.items.every((item) => item.creator_id === 'ctr_a'),
      true,
    );

    const firstPage = await jsonFetch(`${server.url}/v1/daemon/workspaces?limit=1`);
    assert.equal(firstPage.status, 200, firstPage.text);
    assert.deepEqual(firstPage.payload.items, [WORKSPACES[0]]);
    assert.deepEqual(firstPage.payload.pagination, {
      limit: 1,
      has_more: true,
      next_cursor: 'ctr_a/default',
    });

    const secondPage = await jsonFetch(
      `${server.url}/v1/daemon/workspaces?limit=1&cursor=ctr_a/default`,
    );
    assert.equal(secondPage.status, 200, secondPage.text);
    assert.deepEqual(secondPage.payload.items, [WORKSPACES[1]]);
    assert.equal(secondPage.payload.pagination.next_cursor, 'ctr_a/second');

    const malformed = await jsonFetch(`${server.url}/v1/daemon/workspaces?limit=0`);
    assert.equal(malformed.status, 400, malformed.text);
    assert.equal(malformed.payload.error.code, 'invalid_input');

    const unknownKey = await jsonFetch(`${server.url}/v1/daemon/workspaces?slug=default`);
    assert.equal(unknownKey.status, 400, unknownKey.text);
    assert.equal(unknownKey.payload.error.code, 'invalid_input');

    // An unknown cursor must not silently re-serve page one.
    const unknownCursor = await jsonFetch(
      `${server.url}/v1/daemon/workspaces?cursor=ctr_a%2Fmissing`,
    );
    assert.equal(unknownCursor.status, 400, unknownCursor.text);
    assert.equal(unknownCursor.payload.error.code, 'invalid_input');
  });

  test('active read returns ActiveWorkspaceResponse, never 501', async () => {
    const active = await jsonFetch(`${server.url}/v1/daemon/workspace`);
    assert.equal(active.status, 200, active.text);
    assert.deepEqual(Object.keys(active.payload).sort(), [
      'creative_root',
      'creator_id',
      'operational_dir',
      'workspace_slug',
    ]);
    assert.equal(active.payload.workspace_slug, 'default');
    assert.equal(
      active.payload.operational_dir,
      '/tmp/home/.nexus42/creators/ctr_a/workspaces/default',
    );
  });

  test('set-active promotes before claiming readiness', async () => {
    const before = await jsonFetch(`${server.url}/v1/daemon/runtime/status`);
    assert.equal(before.status, 200, before.text);
    assert.equal(before.payload.workspace_initialized, false);

    // A failed promotion must not publish readiness: the request fails and the
    // status stays uninitialized (the next selection retries).
    state.failPromotion = true;
    const refused = await jsonFetch(`${server.url}/v1/daemon/workspace/active`, {
      method: 'POST',
      body: { creator_id: 'ctr_a', workspace_slug: 'second' },
    });
    assert.equal(refused.status, 500, refused.text);
    assert.equal(state.promotions, 1);
    const stillShell = await jsonFetch(`${server.url}/v1/daemon/runtime/status`);
    assert.equal(stillShell.payload.workspace_initialized, false);

    state.failPromotion = false;
    const selected = await jsonFetch(`${server.url}/v1/daemon/workspace/active`, {
      method: 'POST',
      body: { creator_id: 'ctr_a', workspace_slug: 'second' },
    });
    assert.equal(selected.status, 200, selected.text);
    assert.deepEqual(selected.payload, { creator_id: 'ctr_a', workspace_slug: 'second' });
    assert.deepEqual(state.selected, { creator_id: 'ctr_a', workspace_slug: 'second' });
    assert.equal(state.promotions, 2);

    // Readiness is claimed only after the promotion succeeded.
    const after = await jsonFetch(`${server.url}/v1/daemon/runtime/status`);
    assert.equal(after.status, 200, after.text);
    assert.equal(after.payload.workspace_initialized, true);
    assert.equal(after.payload.runtime_mode, 'domain_only');

    // The selected identity is observable through the active read.
    const followUp = await jsonFetch(`${server.url}/v1/daemon/workspace`);
    assert.equal(followUp.status, 200, followUp.text);
    assert.equal(followUp.payload.workspace_slug, 'second');

    // Unknown workspace paths keep the retained 501 refusal.
    const legacyAlias = await jsonFetch(`${server.url}/v1/daemon/workspaces/active`);
    assert.equal(legacyAlias.status, 501, legacyAlias.text);
    assert.equal(legacyAlias.payload.error.code, 'route_not_migrated');
  });

  test('create returns the contract shape and is visible in the list', async () => {
    const created = await jsonFetch(`${server.url}/v1/daemon/workspaces`, {
      method: 'POST',
      body: {
        creator_id: 'ctr_a',
        workspace_slug: 'third',
        display_name: '第三个工作区',
      },
    });
    assert.equal(created.status, 201, created.text);
    assert.deepEqual(Object.keys(created.payload).sort(), [
      'creative_root',
      'creator_id',
      'operational_dir',
      'state_db_path',
      'workspace_slug',
    ]);
    assert.deepEqual(state.createdRequest, {
      creator_id: 'ctr_a',
      workspace_slug: 'third',
      display_name: '第三个工作区',
    });

    const listed = await jsonFetch(`${server.url}/v1/daemon/workspaces?creator_id=ctr_a`);
    assert.equal(listed.status, 200, listed.text);
    const row = listed.payload.items.find((item) => item.workspace_slug === 'third');
    assert.ok(row, 'created workspace is visible in the list');
    assert.equal(row.display_name, '第三个工作区');
  });

  test('create maps conflict to 409 and validation rejection to 400', async () => {
    const conflict = await jsonFetch(`${server.url}/v1/daemon/workspaces`, {
      method: 'POST',
      body: { creator_id: 'ctr_a', workspace_slug: 'duplicate' },
    });
    assert.equal(conflict.status, 409, conflict.text);
    assert.equal(conflict.payload.error.code, 'workspace_conflict');

    const invalid = await jsonFetch(`${server.url}/v1/daemon/workspaces`, {
      method: 'POST',
      body: { creator_id: 'ctr_a', workspace_slug: 'invalid' },
    });
    assert.equal(invalid.status, 400, invalid.text);
    assert.equal(invalid.payload.error.code, 'invalid_input');
    assert.equal(invalid.payload.error.details.field, 'workspace_slug');

    // A non-object body keeps the service's own pre-wire 400.
    const notAnObject = await jsonFetch(`${server.url}/v1/daemon/workspaces`, {
      method: 'POST',
      body: [],
    });
    assert.equal(notAnObject.status, 400, notAnObject.text);
    assert.equal(notAnObject.payload.error.code, 'invalid_input');
  });

  test('concurrent set-active requests promote exactly once', async () => {
    const concurrentState = stubState();
    const concurrent = await startServer(concurrentState);
    try {
      const select = () =>
        jsonFetch(`${concurrent.url}/v1/daemon/workspace/active`, {
          method: 'POST',
          body: { creator_id: 'ctr_a', workspace_slug: 'default' },
        });
      const [left, right] = await Promise.all([select(), select()]);
      assert.equal(left.status, 200, left.text);
      assert.equal(right.status, 200, right.text);
      // The select+promote pair is serialized: the second request finds the
      // process ready instead of publishing a second core.
      assert.equal(concurrentState.promotions, 1);
      const status = await jsonFetch(`${concurrent.url}/v1/daemon/runtime/status`);
      assert.equal(status.payload.workspace_initialized, true);
    } finally {
      await concurrent.close();
    }
  });
});

const NATIVE_CREATOR = 'ctr_testcreator';

/** Fixture home plus a registered second workspace (discovery reads `meta.json`). */
function seedWorkspaceHome() {
  const home = mkdtempSync(join(tmpdir(), 'nexus-workspace-native-'));
  tempDirs.push(home);
  const seed = spawnSync(
    'cargo',
    ['run', '-q', '-p', 'nexus-core-node', '--bin', 'native-wire-fixture-seed', '--', home],
    { stdio: 'inherit' },
  );
  assert.equal(seed.status, 0, seed.stderr?.toString());

  const workspacesRoot = join(home, '.nexus42', 'creators', NATIVE_CREATOR, 'workspaces');
  const creativeRoots = {};
  for (const slug of ['default', 'second']) {
    creativeRoots[slug] = join(home, 'creative', slug);
    mkdirSync(creativeRoots[slug], { recursive: true });
    mkdirSync(join(workspacesRoot, slug), { recursive: true });
    writeFileSync(
      join(workspacesRoot, slug, 'meta.json'),
      JSON.stringify({
        schema_version: 1,
        creator_id: NATIVE_CREATOR,
        workspace_slug: slug,
        local_root: creativeRoots[slug],
        workspace_id: null,
      }),
    );
  }
  // A registered workspace with no `local_root`: the active read must omit
  // `creative_root` instead of naming an empty path (D25).
  mkdirSync(join(workspacesRoot, 'third'), { recursive: true });
  writeFileSync(
    join(workspacesRoot, 'third', 'meta.json'),
    JSON.stringify({
      schema_version: 1,
      creator_id: NATIVE_CREATOR,
      workspace_slug: 'third',
      workspace_id: null,
    }),
  );
  return { home, workspacesRoot, creativeRoots };
}

describe('workspace-http native binding (v1219 shell Task 4)', () => {
  let fixture;
  let service;

  before(async () => {
    fixture = seedWorkspaceHome();
    const { startService } = await import(join(serviceRoot, 'dist/index.js'));
    service = await startService({
      home: fixture.home,
      host: '127.0.0.1',
      port: 18_461,
      allowRemote: false,
      domainOnly: true,
    });
  });

  after(async () => {
    if (service) await service.close();
  });

  test('list, active read and set-active run over the real native core', async () => {
    const listed = await jsonFetch(`${service.url}/v1/daemon/workspaces`);
    assert.equal(listed.status, 200, listed.text);
    assert.deepEqual(
      listed.payload.items.map((item) => item.workspace_slug),
      ['default', 'second', 'third'],
    );
    assert.equal(listed.payload.items[0].creative_root, fixture.creativeRoots.default);

    const active = await jsonFetch(`${service.url}/v1/daemon/workspace`);
    assert.equal(active.status, 200, active.text);
    assert.deepEqual(active.payload, {
      creator_id: NATIVE_CREATOR,
      workspace_slug: 'default',
      creative_root: fixture.creativeRoots.default,
      operational_dir: join(fixture.workspacesRoot, 'default'),
    });

    const selected = await jsonFetch(`${service.url}/v1/daemon/workspace/active`, {
      method: 'POST',
      body: { workspace_slug: 'second' },
    });
    assert.equal(selected.status, 200, selected.text);
    assert.deepEqual(selected.payload, {
      creator_id: NATIVE_CREATOR,
      workspace_slug: 'second',
    });

    // The same session now reports the selected workspace, and selection
    // initialized the chosen workspace DB (never a route-level side effect).
    const after = await jsonFetch(`${service.url}/v1/daemon/workspace`);
    assert.equal(after.status, 200, after.text);
    assert.equal(after.payload.workspace_slug, 'second');
    assert.equal(after.payload.operational_dir, join(fixture.workspacesRoot, 'second'));
    assert.equal(existsSync(join(fixture.workspacesRoot, 'second', 'state.db')), true);

    const status = await jsonFetch(`${service.url}/v1/daemon/runtime/status`);
    assert.equal(status.status, 200, status.text);
    assert.equal(status.payload.workspace_initialized, true);

    // A workspace registered without a creative root answers without the
    // optional field (the client must not display an empty path).
    const bare = await jsonFetch(`${service.url}/v1/daemon/workspace/active`, {
      method: 'POST',
      body: { workspace_slug: 'third' },
    });
    assert.equal(bare.status, 200, bare.text);
    const bareActive = await jsonFetch(`${service.url}/v1/daemon/workspace`);
    assert.equal(bareActive.status, 200, bareActive.text);
    assert.equal('creative_root' in bareActive.payload, false);
    assert.equal(bareActive.payload.operational_dir, join(fixture.workspacesRoot, 'third'));
  });

  test('create registers over the real native core and maps typed refusals', async () => {
    const created = await jsonFetch(`${service.url}/v1/daemon/workspaces`, {
      method: 'POST',
      body: {
        creator_id: NATIVE_CREATOR,
        workspace_slug: 'fourth',
        display_name: '第四个工作区',
      },
    });
    assert.equal(created.status, 201, created.text);
    assert.equal(created.payload.creator_id, NATIVE_CREATOR);
    assert.equal(created.payload.workspace_slug, 'fourth');
    assert.equal(created.payload.creative_root, join(fixture.home, 'Nexus', 'fourth'));

    // Success implies the workspace is discoverable through the list.
    const listed = await jsonFetch(`${service.url}/v1/daemon/workspaces`);
    const row = listed.payload.items.find((item) => item.workspace_slug === 'fourth');
    assert.ok(row, 'created workspace is discoverable');
    assert.equal(row.display_name, '第四个工作区');

    const duplicate = await jsonFetch(`${service.url}/v1/daemon/workspaces`, {
      method: 'POST',
      body: { creator_id: NATIVE_CREATOR, workspace_slug: 'fourth' },
    });
    assert.equal(duplicate.status, 409, duplicate.text);
    assert.equal(duplicate.payload.error.code, 'workspace_conflict');

    const invalid = await jsonFetch(`${service.url}/v1/daemon/workspaces`, {
      method: 'POST',
      body: { creator_id: NATIVE_CREATOR, workspace_slug: 'bad/slug' },
    });
    assert.equal(invalid.status, 400, invalid.text);
    assert.equal(invalid.payload.error.code, 'invalid_input');
  });
});

describe('workspace-http service-only shell (v1219 shell Task 4 fix round 2)', () => {
  let home;
  let service;

  before(async () => {
    // A genuinely fresh home: no `.nexus42`, no config, no state DB — the
    // service opens the status-only shell, exactly the first-run case.
    home = mkdtempSync(join(tmpdir(), 'nexus-workspace-shell-'));
    tempDirs.push(home);
    const { startService } = await import(join(serviceRoot, 'dist/index.js'));
    service = await startService({
      home,
      host: '127.0.0.1',
      port: 18_462,
      allowRemote: false,
      domainOnly: true,
    });
  });

  after(async () => {
    if (service) await service.close();
  });

  test('selection promotes the shell in process and completes the first-run path', async () => {
    const status = await jsonFetch(`${service.url}/v1/daemon/runtime/status`);
    assert.equal(status.status, 200, status.text);
    assert.equal(status.payload.workspace_initialized, false);

    // A session that never selects keeps the full domain denied.
    const denied = await jsonFetch(`${service.url}/v1/daemon/works`);
    assert.equal(denied.status, 409, denied.text);
    assert.equal(denied.payload.error.code, 'uninitialized');

    const empty = await jsonFetch(`${service.url}/v1/daemon/creators`);
    assert.equal(empty.status, 200, empty.text);
    assert.deepEqual(empty.payload.items, []);

    const created = await jsonFetch(`${service.url}/v1/daemon/creators`, {
      method: 'POST',
      body: { display_name: 'Fresh Creator' },
    });
    assert.equal(created.status, 201, created.text);
    const creatorId = created.payload.creator_id;
    assert.match(creatorId, /^ctr_local[a-zA-Z0-9]+$/);

    // The created display name survives in the listing (global identity row +
    // enrichment cache), not degraded to the creator id.
    const listed = await jsonFetch(`${service.url}/v1/daemon/creators`);
    assert.equal(listed.status, 200, listed.text);
    assert.deepEqual(
      listed.payload.items.map((item) => ({ id: item.creator_id, name: item.display_name })),
      [{ id: creatorId, name: 'Fresh Creator' }],
    );

    // Before any selection the pre-created `default` directory is unregistered.
    const beforeSelect = await jsonFetch(`${service.url}/v1/daemon/workspaces`);
    assert.equal(beforeSelect.status, 200, beforeSelect.text);
    assert.deepEqual(beforeSelect.payload.items, []);

    const selected = await jsonFetch(`${service.url}/v1/daemon/workspace/active`, {
      method: 'POST',
      body: { creator_id: creatorId, workspace_slug: 'default' },
    });
    assert.equal(selected.status, 200, selected.text);
    assert.deepEqual(selected.payload, { creator_id: creatorId, workspace_slug: 'default' });

    const creativeRoot = join(home, 'Nexus');
    const active = await jsonFetch(`${service.url}/v1/daemon/workspace`);
    assert.equal(active.status, 200, active.text);
    assert.deepEqual(active.payload, {
      creator_id: creatorId,
      workspace_slug: 'default',
      creative_root: creativeRoot,
      operational_dir: join(home, '.nexus42', 'creators', creatorId, 'workspaces', 'default'),
    });

    // Same session, no restart: promotion published the full core.
    const after = await jsonFetch(`${service.url}/v1/daemon/runtime/status`);
    assert.equal(after.status, 200, after.text);
    assert.equal(after.payload.workspace_initialized, true);
    assert.equal(after.payload.runtime_mode, 'domain_only');

    const domain = await jsonFetch(`${service.url}/v1/daemon/works`);
    assert.equal(domain.status, 200, domain.text);

    // The first selection registered the pre-created default workspace.
    const discovered = await jsonFetch(`${service.url}/v1/daemon/workspaces`);
    assert.equal(discovered.status, 200, discovered.text);
    assert.deepEqual(discovered.payload.items, [
      {
        creator_id: creatorId,
        workspace_slug: 'default',
        creative_root: creativeRoot,
        display_name: '默认工作区',
      },
    ]);
  });
});


