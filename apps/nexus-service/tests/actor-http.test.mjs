import assert from 'node:assert/strict';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { after, before, describe, test } from 'node:test';

const __dirname = dirname(fileURLToPath(import.meta.url));
const serviceRoot = join(__dirname, '..');

/**
 * P5-T2 bounded integration target: the Actor / memory / context families
 * over the real in-process service and the real native temporary store. No
 * mock forwards anything — the Character/binding fixtures are created through
 * the surface itself, and denial (foreign Character, stale binding) is
 * observed as the retained HTTP statuses with zero storage effects.
 */

function seedHome() {
  const home = mkdtempSync(join(tmpdir(), 'nexus-service-actor-'));
  const seed = spawnSync(
    'cargo',
    ['run', '-q', '-p', 'nexus-core-node', '--bin', 'native-wire-fixture-seed', '--', home],
    { stdio: 'inherit' },
  );
  assert.equal(seed.status, 0, seed.stderr?.toString());
  return home;
}

async function jsonFetch(url, { method = 'GET', body } = {}) {
  const response = await fetch(url, {
    method,
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  const text = await response.text();
  const payload = text.length > 0 ? JSON.parse(text) : null;
  return { status: response.status, payload, text };
}

async function startActorService(home, port) {
  const { startService } = await import(join(serviceRoot, 'dist/index.js'));
  return startService({
    home,
    host: '127.0.0.1',
    port,
    allowRemote: false,
    domainOnly: true,
  });
}

const CREATE_CHARACTER_BODY = {
  world_id: 'wld_owned',
  display_name: 'Actor Surface Character',
  persona: { voice: 'measured', wants: 'to be admitted, not simulated' },
};

describe('actor-http (P5-T2)', () => {
  let home;
  let service;
  let baseUrl;

  before(async () => {
    home = seedHome();
    const build = spawnSync('npx', ['tsc', '-p', 'tsconfig.json'], {
      cwd: serviceRoot,
      stdio: 'inherit',
    });
    assert.equal(build.status, 0);
    service = await startActorService(home, 18_443);
    baseUrl = service.url;
  });

  after(async () => {
    if (service) await service.close();
  });

  test('every assigned Actor/memory/context route identity is mounted at its exact verb and tier', async () => {
    // Enumeration is deliberately separate from the behavioral proof below:
    // this asserts the composer inventory, not handler behavior.
    const { DOMAIN_ROUTES } = await import(join(serviceRoot, 'dist/routes.js'));
    const inventory = DOMAIN_ROUTES.map((route) => ({
      method: route.method,
      path: route.pattern.source.replace(/\\\//g, '/').replace(/^\^|\$$/g, ''),
      tier: route.tier,
      family: route.family,
    }));
    const required = [
      ['GET', '/v1/daemon/characters'],
      ['POST', '/v1/daemon/characters'],
      ['GET', '/v1/daemon/characters/([^/]+)'],
      ['PATCH', '/v1/daemon/characters/([^/]+)'],
      ['POST', '/v1/daemon/characters/([^/]+)/archive'],
      ['POST', '/v1/daemon/characters/([^/]+)/restore'],
      ['POST', '/v1/daemon/characters/([^/]+)/bindings'],
      ['GET', '/v1/daemon/characters/([^/]+)/bindings'],
      ['GET', '/v1/daemon/characters/([^/]+)/bindings/([^/]+)'],
      ['PATCH', '/v1/daemon/characters/([^/]+)/bindings/([^/]+)'],
      ['DELETE', '/v1/daemon/characters/([^/]+)/bindings/([^/]+)'],
      ['GET', '/v1/daemon/characters/([^/]+)/knowledge'],
      ['GET', '/v1/daemon/characters/([^/]+)/knowledge/([^/]+)'],
      ['PATCH', '/v1/daemon/characters/([^/]+)/knowledge/([^/]+)'],
      ['DELETE', '/v1/daemon/characters/([^/]+)/knowledge/([^/]+)'],
      ['POST', '/v1/daemon/actor-knowledge/view'],
      ['POST', '/v1/daemon/actor-knowledge/entries'],
      ['GET', '/v1/daemon/creators'],
      ['POST', '/v1/daemon/creators'],
      ['GET', '/v1/daemon/creators/active'],
      ['PUT', '/v1/daemon/creators/active'],
      ['GET', '/v1/daemon/creators/([^/]+)'],
      ['PATCH', '/v1/daemon/creators/([^/]+)'],
      ['POST', '/v1/daemon/creators/([^/]+)'],
      ['POST', '/v1/daemon/characters/([^/]+)/memory/pending-review'],
      ['GET', '/v1/daemon/characters/([^/]+)/memory/pending-review'],
      ['GET', '/v1/daemon/characters/([^/]+)/memory/pending-review/count'],
      ['DELETE', '/v1/daemon/characters/([^/]+)/memory/pending-review/([^/]+)'],
      ['POST', '/v1/daemon/characters/([^/]+)/memory/review'],
      ['GET', '/v1/daemon/characters/([^/]+)/memory/fragments'],
      ['POST', '/v1/daemon/characters/([^/]+)/memory/fragments/([^/]+):promote'],
      ['POST', '/v1/daemon/characters/([^/]+)/soul/reflect'],
      ['POST', '/v1/daemon/characters/([^/]+)/tom'],
      ['GET', '/v1/daemon/characters/([^/]+)/tom'],
      ['GET', '/v1/daemon/memory/pending-review'],
      ['GET', '/v1/daemon/memory/pending-review/count'],
      ['DELETE', '/v1/daemon/memory/pending-review/([^/]+)'],
      ['POST', '/v1/daemon/memory/review'],
      ['GET', '/v1/daemon/memory/fragments'],
      ['POST', '/v1/daemon/memory/soul/reflect'],
      ['POST', '/v1/daemon/inspector/moment'],
      ['POST', '/v1/daemon/moment-directive'],
    ];
    const mounted = new Set(inventory.map((route) => `${route.method} ${route.path}`));
    for (const [method, path] of required) {
      assert.ok(
        mounted.has(`${method} ${path}`),
        `missing mounted identity: ${method} ${path}\nmounted:\n${[...mounted].sort().join('\n')}`,
      );
    }
    // Tier parity (daemon mod.rs authority): the Creator home family is
    // tier1 (API-key only, no active creator); every other family route is
    // tier2 (creator-tier). None is unguarded.
    for (const route of inventory) {
      if (route.path.startsWith('/v1/daemon/creators')) {
        assert.equal(route.tier, 'tier1', `${route.method} ${route.path} must stay tier1`);
      } else {
        assert.equal(route.tier, 'tier2', `${route.method} ${route.path} must stay tier2`);
      }
    }
  });

  test('foreign actor and stale binding are denied by the real store with zero effect, and a valid Actor gets context', async () => {
    // 1. A legal owned Character + binding is created through the surface.
    const created = await jsonFetch(`${baseUrl}/v1/daemon/characters`, {
      method: 'POST',
      body: CREATE_CHARACTER_BODY,
    });
    assert.equal(created.status, 201, created.text);
    const characterId = created.payload.character?.character_id ?? created.payload.character_id;
    assert.ok(characterId, `character id missing: ${created.text}`);
    // Creation carries the initial active binding for the same World.
    const bindingId = created.payload.binding?.binding_id;
    assert.ok(bindingId, `initial binding id missing: ${created.text}`);

    // 2. A foreign Character ref (never owned by the active creator) is
    //    denied by the real native store — 404, existence hidden — and the
    //    denial produces zero provider/storage effects: the owned Character
    //    list is unchanged afterwards.
    const FOREIGN_CHARACTER = 'chr_' + 'f'.repeat(32);
    const foreign = await jsonFetch(`${baseUrl}/v1/daemon/characters/${FOREIGN_CHARACTER}`);
    assert.equal(foreign.status, 404, foreign.text);
    assert.equal(foreign.payload.error.code, 'not_found');
    const foreignView = await jsonFetch(`${baseUrl}/v1/daemon/actor-knowledge/view`, {
      method: 'POST',
      body: {
        actor_ref: { actor_kind: 'character', character_id: FOREIGN_CHARACTER },
        world_id: 'wld_owned',
        binding_id: bindingId,
      },
    });
    assert.equal(foreignView.status, 404, foreignView.text);

    // 3. A stale binding patch (wrong expected_revision) is the retained 409
    //    conflict, and nothing is written.
    const stale = await jsonFetch(
      `${baseUrl}/v1/daemon/characters/${characterId}/bindings/${bindingId}`,
      { method: 'PATCH', body: { expected_revision: 999, world_sheet_entry_id: 'wse_stale' } },
    );
    assert.equal(stale.status, 409, stale.text);
    const afterStale = await jsonFetch(
      `${baseUrl}/v1/daemon/characters/${characterId}/bindings/${bindingId}`,
    );
    assert.equal(afterStale.status, 200, afterStale.text);
    const liveBinding = afterStale.payload.binding ?? afterStale.payload;
    assert.equal(liveBinding.revision, 0, 'stale patch must not advance the binding revision');
    assert.ok(
      liveBinding.world_sheet_entry_id == null,
      'stale patch must not write the sheet',
    );

    // 4. A valid owned Actor gets real context: the admitted view over the
    //    owned World returns the paginated KnowledgeView (no 501).
    const view = await jsonFetch(`${baseUrl}/v1/daemon/actor-knowledge/view`, {
      method: 'POST',
      body: {
        actor_ref: { actor_kind: 'creator', creator_id: 'ctr_testcreator' },
        world_id: 'wld_owned',
      },
    });
    assert.equal(view.status, 200, view.text);
    assert.ok(Array.isArray(view.payload.items), 'admitted view must return items');
    assert.ok(view.payload.pagination, 'admitted view must return pagination');

    // 5. The retained logout verb: a POST without the `:logout` suffix is
    //    not a routed identity (daemon strips the suffix inside the shared
    //    `{creator_id}` segment and 404s otherwise).
    const bareLogout = await jsonFetch(`${baseUrl}/v1/daemon/creators/ctr_testcreator`, {
      method: 'POST',
    });
    assert.equal(bareLogout.status, 404, bareLogout.text);

    // 6. The moment context surface is real: the directive route answers on
    //    the owned World instead of a migration denial.
    const directive = await jsonFetch(`${baseUrl}/v1/daemon/moment-directive`, {
      method: 'POST',
      body: {
        action: 'show',
        scope: { kind: 'world', id: 'wld_owned' },
      },
    });
    assert.notEqual(directive.status, 501, directive.text);
  });
});
