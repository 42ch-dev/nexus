import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { after, before, describe, test } from 'node:test';

const __dirname = dirname(fileURLToPath(import.meta.url));
const serviceRoot = join(__dirname, '..');

/**
 * P5-T1 bounded integration target: the World / Work / content / knowledge
 * families over the real in-process service and the real native temporary
 * store. No mock forwards anything — persistence is observed through a full
 * service close + reopen on the same home, and conflicts are observed as the
 * retained HTTP statuses.
 */

function seedHome() {
  const home = mkdtempSync(join(tmpdir(), 'nexus-service-domain-'));
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

async function startDomainService(home, port) {
  const { startService } = await import(join(serviceRoot, 'dist/index.js'));
  return startService({
    home,
    host: '127.0.0.1',
    port,
    allowRemote: false,
    domainOnly: true,
  });
}

const OWNED_WORLD = 'wld_owned';
const CREATE_WORK_BODY = {
  title: 'Domain Surface Work',
  long_term_goal: 'Prove the persisted Work selection',
  initial_idea: 'Seeded by the P5-T1 domain-http test',
  world_id: OWNED_WORLD,
};

describe('domain-http (P5-T1)', () => {
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
    service = await startDomainService(home, 18_441);
    baseUrl = service.url;
  });

  after(async () => {
    if (service) await service.close();
  });

  test('every assigned World/Work/content/knowledge route identity is mounted at its exact verb and tier', async () => {
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
      ['POST', '/v1/daemon/worlds'],
      ['DELETE', '/v1/daemon/worlds/([^/]+)'],
      ['GET', '/v1/daemon/narrative/worlds'],
      ['GET', '/v1/daemon/narrative/worlds/([^/]+)'],
      ['POST', '/v1/daemon/worlds/([^/]+)/kb/promote-candidate'],
      ['POST', '/v1/daemon/worlds/([^/]+)/kb/patch-relationship'],
      ['GET', '/v1/daemon/worlds/([^/]+)/kb/key-blocks/([^/]+)/state'],
      ['POST', '/v1/daemon/worlds/([^/]+)/forks'],
      ['POST', '/v1/daemon/worlds/([^/]+)/kb/pack/export'],
      ['POST', '/v1/daemon/worlds/([^/]+)/kb/pack/import'],
      ['GET', '/v1/daemon/worlds/([^/]+)/rules'],
      ['POST', '/v1/daemon/worlds/([^/]+)/rules'],
      ['PATCH', '/v1/daemon/worlds/([^/]+)/rules/([^/]+)'],
      ['GET', '/v1/daemon/worlds/([^/]+)/findings'],
      ['GET', '/v1/daemon/timeline/overview'],
      ['GET', '/v1/daemon/worlds/([^/]+)/timeline/events'],
      ['GET', '/v1/daemon/works'],
      ['POST', '/v1/daemon/works'],
      ['GET', '/v1/daemon/works/pool'],
      ['POST', '/v1/daemon/works/pool'],
      ['POST', '/v1/daemon/works/pool/promote'],
      ['POST', '/v1/daemon/works/pool/archive'],
      ['GET', '/v1/daemon/works/pool/inspiration'],
      ['POST', '/v1/daemon/works/pool/inspiration'],
      ['POST', '/v1/daemon/works/pool/inspiration/promote'],
      ['POST', '/v1/daemon/works/pool/inspiration/archive'],
      ['GET', '/v1/daemon/works/([^/]+)'],
      ['PATCH', '/v1/daemon/works/([^/]+)'],
      ['DELETE', '/v1/daemon/works/([^/]+)'],
      ['POST', '/v1/daemon/works/([^/]+)/inspiration'],
      ['POST', '/v1/daemon/works/([^/]+)/completion-lock/release'],
      ['POST', '/v1/daemon/works/([^/]+)/reconcile-chapters'],
      ['GET', '/v1/daemon/works/([^/]+)/chapters'],
      ['GET', '/v1/daemon/works/([^/]+)/chapters/([^/]+)'],
      ['PATCH', '/v1/daemon/works/([^/]+)/chapters/([^/]+)'],
      ['GET', '/v1/daemon/works/([^/]+)/chapters/([^/]+)/outline'],
      ['GET', '/v1/daemon/works/([^/]+)/chapters/([^/]+)/body'],
      ['GET', '/v1/daemon/works/([^/]+)/outline'],
      ['POST', '/v1/daemon/works/([^/]+)/outline/patch'],
      ['POST', '/v1/daemon/works/([^/]+)/chapters/([^/]+)/patch'],
      ['POST', '/v1/daemon/works/([^/]+)/timeline/patch'],
      ['GET', '/v1/daemon/kb/entries'],
      ['POST', '/v1/daemon/kb/entries'],
      ['GET', '/v1/daemon/kb/entries/([^/]+)'],
      ['DELETE', '/v1/daemon/kb/entries/([^/]+)'],
      ['GET', '/v1/daemon/works/([^/]+)/findings'],
      ['POST', '/v1/daemon/works/([^/]+)/findings'],
      ['POST', '/v1/daemon/works/([^/]+)/findings/from-review'],
      ['GET', '/v1/daemon/works/([^/]+)/findings/([^/]+)'],
      ['PATCH', '/v1/daemon/works/([^/]+)/findings/([^/]+)'],
      ['DELETE', '/v1/daemon/works/([^/]+)/findings/([^/]+)'],
      ['GET', '/v1/daemon/findings/stale'],
      ['PATCH', '/v1/daemon/findings/batch'],
      ['POST', '/v1/daemon/findings/prune'],
      ['GET', '/v1/daemon/findings/([^/]+)'],
      ['GET', '/v1/daemon/reading/progress'],
      ['PUT', '/v1/daemon/reading/progress'],
      ['DELETE', '/v1/daemon/reading/progress'],
      ['GET', '/v1/daemon/reading/annotations'],
      ['POST', '/v1/daemon/reading/annotations'],
      ['PATCH', '/v1/daemon/reading/annotations/([^/]+)'],
      ['DELETE', '/v1/daemon/reading/annotations/([^/]+)'],
      ['GET', '/v1/daemon/references'],
      ['GET', '/v1/daemon/references/([^/]+)'],
    ];
    const mounted = new Set(inventory.map((route) => `${route.method} ${route.path}`));
    for (const [method, path] of required) {
      assert.ok(
        mounted.has(`${method} ${path}`),
        `missing mounted identity: ${method} ${path}\nmounted:\n${[...mounted].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0)).join('\n')}`,
      );
    }
    // Tier parity (daemon mod.rs authority): the Creator home family is
    // tier1 (API-key only, no active creator); every P5-T1 family route is
    // tier2. None is unguarded. (The runtime liveness family reconciled in
    // P5-T5 is unguarded by contract and is out of this task's scope.)
    const OWN_FAMILIES = new Set(['worlds', 'works', 'content', 'knowledge']);
    for (const route of inventory) {
      if (!OWN_FAMILIES.has(route.family)) continue;
      if (route.path.startsWith('/v1/daemon/creators')) {
        assert.equal(route.tier, 'tier1', `${route.method} ${route.path} must stay tier1`);
      } else {
        assert.equal(route.tier, 'tier2', `${route.method} ${route.path} must stay tier2`);
      }
    }
  });

  test('work selection and world conflict persist over the real native store', async () => {
    // 1. Work creation keeps the retained 201 seam.
    const created = await jsonFetch(`${baseUrl}/v1/daemon/works`, {
      method: 'POST',
      body: CREATE_WORK_BODY,
    });
    assert.equal(created.status, 201, created.text);
    const workId = created.payload.work_id;

    // 2. Selection is the durable pool-active mutation, not a session-local
    // echo: it returns the active pool entry and survives a full close.
    const selected = await jsonFetch(`${baseUrl}/v1/daemon/works/pool`, {
      method: 'POST',
      // The P1-T1 authority (works.rs set_pool_active) accepts exactly this
      // action token; anything else is an invalid_action 400.
      body: { action: 'set_pool_active', work_id: workId },
    });
    assert.equal(selected.status, 200, selected.text);
    assert.equal(selected.payload.work_id, workId);
    assert.equal(selected.payload.status, 'active');

    await service.close();
    service = await startDomainService(home, 18_442);
    baseUrl = service.url;

    const poolAfterReopen = await jsonFetch(`${baseUrl}/v1/daemon/works/pool`);
    assert.equal(poolAfterReopen.status, 200, poolAfterReopen.text);
    const active = poolAfterReopen.payload.entries.find((entry) => entry.status === 'active');
    assert.ok(active, 'selection must persist across a service restart');
    assert.equal(active.work_id, workId);

    // 3. World KB CAS: a stale expected version is a 409 with the structured
    // conflict details, and nothing is written.
    const stale = await jsonFetch(
      `${baseUrl}/v1/daemon/worlds/${OWNED_WORLD}/kb/patch-entity`,
      { method: 'POST', body: { entity_id: 'kb_cas', expected_version: 1, patch: { title: 'Stale' } } },
    );
    assert.equal(stale.status, 409, stale.text);
    assert.equal(stale.payload.error.code, 'world_kb_conflict');
    assert.equal(stale.payload.error.details.entity_id, 'kb_cas');
    assert.equal(stale.payload.error.details.current_version, 2);

    // 4. The retained 204 delete is bodiless and the deletion persists.
    const removed = await jsonFetch(`${baseUrl}/v1/daemon/works/${workId}`, {
      method: 'DELETE',
    });
    assert.equal(removed.status, 204, removed.text);
    assert.equal(removed.payload, null);
    const afterDelete = await jsonFetch(`${baseUrl}/v1/daemon/works/${workId}`);
    assert.equal(afterDelete.status, 404);
  });

  test('Timeline overview emits required nullable cursor', async () => {
    // Terminal page: the seeded home holds fewer Worlds than the 20-row page
    // cap, so the overview ends here and the required nullable cursor must be
    // present as explicit null — never an omitted key.
    const terminal = await jsonFetch(`${baseUrl}/v1/daemon/timeline/overview`);
    assert.equal(terminal.status, 200, terminal.text);
    assert.ok(
      Object.prototype.hasOwnProperty.call(terminal.payload, 'cursor'),
      'terminal overview must carry the required cursor key',
    );
    assert.equal(terminal.payload.cursor, null);

    // Non-terminal page: exceed the 20-World page cap so page 1 must carry a
    // usable string cursor, and its continuation page closes again.
    for (let i = 0; i < 21; i += 1) {
      const created = await jsonFetch(`${baseUrl}/v1/daemon/worlds`, {
        method: 'POST',
        body: { title: `Overview World ${i}` },
      });
      assert.equal(created.status, 201, created.text);
    }

    const page1 = await jsonFetch(`${baseUrl}/v1/daemon/timeline/overview`);
    assert.equal(page1.status, 200, page1.text);
    assert.equal(page1.payload.worlds.length, 20);
    assert.equal(typeof page1.payload.cursor, 'string');
    assert.ok(page1.payload.cursor.length > 0, 'non-terminal page must carry a usable cursor');

    const page2 = await jsonFetch(
      `${baseUrl}/v1/daemon/timeline/overview?cursor=${encodeURIComponent(page1.payload.cursor)}`,
    );
    assert.equal(page2.status, 200, page2.text);
    assert.ok(
      Object.prototype.hasOwnProperty.call(page2.payload, 'cursor'),
      'continuation page must carry the required cursor key',
    );
    assert.equal(page2.payload.cursor, null);
    assert.equal(page2.payload.total_worlds, page1.payload.total_worlds);
  });

  test('scene/beat authoring reaches the core owner over HTTP and persists', async () => {
    // The native store's Work family resolves every filesystem path through
    // the selected workspace's registered `local_root`, so the durable proof
    // starts by registering one (with a real chapter SSOT beneath it).
    const creativeRoot = mkdtempSync(join(tmpdir(), 'nexus-service-creative-'));
    const workspaceDir = join(
      home,
      '.nexus42',
      'creators',
      'ctr_testcreator',
      'workspaces',
      'default',
    );
    writeFileSync(
      join(workspaceDir, 'meta.json'),
      JSON.stringify({ local_root: creativeRoot }),
    );

    const workRef = 'http-scene-beat';
    const created = await jsonFetch(`${baseUrl}/v1/daemon/works`, {
      method: 'POST',
      body: { ...CREATE_WORK_BODY, story_ref: workRef, title: 'Scene Beat Work' },
    });
    assert.equal(created.status, 201, created.text);
    const workId = created.payload.work_id;

    const storiesDir = join(creativeRoot, 'Works', workRef, 'Stories');
    mkdirSync(storiesDir, { recursive: true });
    writeFileSync(
      join(storiesDir, 'ch01-opening.md'),
      '---\nstatus: not_started\nvolume: 1\n---\nChapter one body\n',
    );
    const reconciled = await jsonFetch(
      `${baseUrl}/v1/daemon/works/${workId}/reconcile-chapters`,
      { method: 'POST' },
    );
    assert.equal(reconciled.status, 200, reconciled.text);
    assert.equal(reconciled.payload.created, 1, 'the chapter SSOT row is created');

    const patchOutline = (body) =>
      jsonFetch(`${baseUrl}/v1/daemon/works/${workId}/outline/patch`, {
        method: 'POST',
        body: { work_id: workId, ...body },
      });
    const readOutline = () => jsonFetch(`${baseUrl}/v1/daemon/works/${workId}/outline`);

    const initial = await readOutline();
    assert.equal(initial.status, 200, initial.text);
    assert.equal(initial.payload.outline_revision, 0);
    assert.deepEqual(initial.payload.scenes, []);
    assert.deepEqual(initial.payload.beats, []);

    const addedScene = await patchOutline({
      base_revision: 0,
      operation: 'add_scene',
      chapter_id: 1,
      title: 'Opening Scene',
    });
    assert.equal(addedScene.status, 200, addedScene.text);
    assert.equal(addedScene.payload.new_revision, 1);

    const afterScene = await readOutline();
    assert.equal(afterScene.payload.scenes.length, 1);
    const scene = afterScene.payload.scenes[0];
    assert.match(scene.scene_id, /^scn_[0-9a-f]{32}$/);
    assert.equal(scene.chapter_id, 1);
    assert.equal(scene.title, 'Opening Scene');
    assert.equal(scene.status, 'drafted');

    const addedBeat = await patchOutline({
      base_revision: 1,
      operation: 'add_beat',
      scene_id: scene.scene_id,
      title: 'Inciting Moment',
      status: 'completed',
    });
    assert.equal(addedBeat.status, 200, addedBeat.text);
    assert.equal(addedBeat.payload.new_revision, 2);

    const afterBeat = await readOutline();
    assert.equal(afterBeat.payload.beats.length, 1);
    const beat = afterBeat.payload.beats[0];
    assert.match(beat.beat_id, /^bet_[0-9a-f]{32}$/);
    assert.equal(beat.scene_id, scene.scene_id);
    assert.equal(beat.title, 'Inciting Moment');
    assert.equal(beat.status, 'completed');

    // Persistence is observed through a full service close + reopen on the
    // same home: the authored ids and their parent relationships survive.
    await service.close();
    service = await startDomainService(home, 18_443);
    baseUrl = service.url;

    const reopened = await readOutline();
    assert.equal(reopened.status, 200, reopened.text);
    assert.equal(reopened.payload.outline_revision, 2);
    assert.deepEqual(reopened.payload.scenes, afterBeat.payload.scenes);
    assert.deepEqual(reopened.payload.beats, afterBeat.payload.beats);

    // A stale base_revision is refused with the typed OCC envelope — the same
    // `outline_conflict` class the core layer asserts in
    // `retained_outline_scene_beat_stale_revision_conflicts` — and the canonical
    // content/revision is untouched. The transport must render the coded 409,
    // never a sanitized 500, or Task 3's conflict modal cannot open.
    const stale = await patchOutline({
      base_revision: 0,
      operation: 'add_scene',
      chapter_id: 1,
      title: 'Stale Scene',
    });
    assert.equal(stale.status, 409, stale.text);
    assert.equal(stale.payload.error.code, 'outline_conflict');
    assert.equal(stale.payload.error.details.current_revision, 2);

    const afterStale = await readOutline();
    assert.equal(afterStale.payload.outline_revision, 2);
    assert.deepEqual(afterStale.payload.scenes, afterBeat.payload.scenes);
    assert.deepEqual(afterStale.payload.beats, afterBeat.payload.beats);
  });
});
