import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { after, before, describe, test } from 'node:test';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = join(__dirname, '..', '..', '..');
const serviceRoot = join(__dirname, '..');

const CREATOR = 'ctr_testcreator';
const SLUG = 'default';
const MODULE = 'basic-combat';
/** A World owned by another creator in the seeded store (`seed_wire_home`). */
const FOREIGN_WORLD = 'wld_foreign';

/**
 * Every observation below crosses HTTP into Rust: the module list/detail are
 * the compiled-in registry manifests (the invocation schema Run Studio renders
 * is the shipped one, never a TS shape), the run reaches the actual WASM
 * authority and returns its proposals, the World stays untouched until ONE
 * accept commits them, and detail/history/discard/clear read the durable run
 * rows the same authority wrote. The World and its two computable characters
 * are staged through the retained World/KB routes, so no private DB seed is
 * used, and the accepted effect is observed through the EXISTING KB and
 * timeline reads (C9) — this suite invents no second timeline API.
 */
function seededHome() {
  const home = mkdtempSync(join(tmpdir(), 'nexus-compute-http-'));
  const seed = spawnSync(
    'cargo',
    ['run', '-q', '-p', 'nexus-core-node', '--bin', 'native-wire-fixture-seed', '--', home],
    { cwd: root, stdio: 'inherit' },
  );
  assert.equal(seed.status, 0, seed.stderr?.toString());
  const creativeRoot = join(home, 'creative-root');
  mkdirSync(creativeRoot, { recursive: true });
  writeFileSync(
    join(home, '.nexus42', 'creators', CREATOR, 'workspaces', SLUG, 'meta.json'),
    JSON.stringify({ local_root: creativeRoot }),
  );
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

async function startComputeService(home, port) {
  const { startService } = await import(join(serviceRoot, 'dist/index.js'));
  return startService({ home, host: '127.0.0.1', port, allowRemote: false });
}

/** One owned World, created through the retained World route. */
async function createWorld(baseUrl, title) {
  const created = await jsonFetch(`${baseUrl}/v1/daemon/worlds`, {
    method: 'POST',
    body: { title },
  });
  assert.equal(created.status, 201, created.text);
  assert.ok(created.payload.world_id, created.text);
  return created.payload.world_id;
}

async function stageCombatant(baseUrl, worldId, entityId, name, stats) {
  const staged = await jsonFetch(`${baseUrl}/v1/daemon/worlds/${worldId}/kb/patch-entity`, {
    method: 'POST',
    body: {
      entity_id: entityId,
      expected_version: 0,
      patch: {
        title: name,
        block_type: 'character',
        body: {
          summary: `${name} (HTTP compute fixture)`,
          attributes: {
            novel_category: 'character',
            max_hp: stats.maxHp,
            base_atk: stats.baseAtk,
            base_def: stats.baseDef,
          },
          computable: true,
          state: {
            character: { current_hp: stats.currentHp, is_alive: true, status_effects: [] },
          },
        },
      },
    },
  });
  assert.equal(staged.status, 200, staged.text);
}

/** The existing World-KB state read: computability flag + mutable state (C9). */
async function readKeyBlockState(baseUrl, worldId, entityId) {
  const read = await jsonFetch(
    `${baseUrl}/v1/daemon/worlds/${worldId}/kb/key-blocks/${entityId}/state`,
  );
  assert.equal(read.status, 200, read.text);
  return read.payload;
}

/** The existing World timeline read, filtered to the accepted-run family (C9). */
async function readComputeTimeline(baseUrl, worldId) {
  const read = await jsonFetch(
    `${baseUrl}/v1/daemon/worlds/${worldId}/timeline/events?event_type=compute_result&status=canon`,
  );
  assert.equal(read.status, 200, read.text);
  return read.payload.items;
}

async function runModule(baseUrl, worldId, attackerId, defenderId) {
  return await jsonFetch(`${baseUrl}/v1/daemon/compute/run`, {
    method: 'POST',
    body: {
      world_id: worldId,
      module_id: MODULE,
      invocation_params: { attacker_id: attackerId, defender_id: defenderId },
    },
  });
}

function combatantIds(worldId) {
  const suffix = worldId.replace(/[^0-9a-f]/g, '').slice(-12);
  return { attackerId: `kb_a${suffix}`, defenderId: `kb_d${suffix}` };
}

/** A World with one attacker (20 ATK) and one defender (30/50 HP, 5 DEF). */
async function stageCombatWorld(baseUrl, title) {
  const worldId = await createWorld(baseUrl, title);
  const { attackerId, defenderId } = combatantIds(worldId);
  await stageCombatant(baseUrl, worldId, attackerId, 'Striker', {
    maxHp: 100,
    baseAtk: 20,
    baseDef: 3,
    currentHp: 100,
  });
  await stageCombatant(baseUrl, worldId, defenderId, 'Guardian', {
    maxHp: 50,
    baseAtk: 10,
    baseDef: 5,
    currentHp: 30,
  });
  return { worldId, attackerId, defenderId };
}

describe('compute-http (v1.195 P2-T3)', () => {
  let home;
  let service;
  let baseUrl;

  before(async () => {
    home = seededHome();
    service = await startComputeService(home, 18_470);
    baseUrl = service.url;
  });

  after(async () => {
    if (service) await service.close();
  });

  test('compute lifecycle: the eight public identities are mounted at tier2', async () => {
    const { DOMAIN_ROUTES } = await import(join(serviceRoot, 'dist/routes.js'));
    const compute = DOMAIN_ROUTES.filter((route) => route.family === 'compute').map((route) => ({
      method: route.method,
      path: route.pattern.source.replace(/\\\//g, '/').replace(/^\^|\$$/g, ''),
      tier: route.tier,
    }));
    const mounted = new Set(compute.map((route) => `${route.method} ${route.path}`));
    const required = [
      ['GET', '/v1/daemon/compute/modules'],
      ['GET', '/v1/daemon/compute/modules/([^/]+)'],
      ['POST', '/v1/daemon/compute/run'],
      ['GET', '/v1/daemon/compute/runs'],
      ['GET', '/v1/daemon/compute/runs/([^/]+)'],
      ['POST', '/v1/daemon/compute/runs/([^/]+)/accept'],
      ['POST', '/v1/daemon/compute/runs/([^/]+)/discard'],
      ['DELETE', '/v1/daemon/compute/runs'],
    ];
    assert.equal(compute.length, required.length, `compute family size:\n${compute.map((r) => `${r.method} ${r.path}`).join('\n')}`);
    for (const [method, path] of required) {
      assert.ok(
        mounted.has(`${method} ${path}`),
        `missing mounted identity: ${method} ${path}\nmounted:\n${[...mounted].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0)).join('\n')}`,
      );
    }
    for (const route of compute) {
      assert.equal(route.tier, 'tier2', `${route.method} ${route.path} must be tier2`);
    }
  });

  test('compute lifecycle: real module schema, zero-effect run, review reads and one accept', async () => {
    const discovery = await jsonFetch(`${baseUrl}/v1/daemon/compute/modules`);
    assert.equal(discovery.status, 200, discovery.text);
    const summary = discovery.payload.items.find((item) => item.module_id === MODULE);
    assert.ok(summary, `installed module must be listed: ${discovery.text}`);
    assert.equal(summary.name, 'Basic Combat');
    assert.equal(summary.version, '1.0.0');
    assert.equal(summary.status, 'ok');
    assert.equal(discovery.payload.has_more, false);

    const detail = await jsonFetch(`${baseUrl}/v1/daemon/compute/modules/${MODULE}`);
    assert.equal(detail.status, 200, detail.text);
    assert.equal(detail.payload.nexus_abi_version, 1);
    assert.equal(detail.payload.compute_export, 'compute');
    assert.deepEqual(detail.payload.required_key_block_types, ['character']);
    assert.equal(
      detail.payload.schemas.invocation.properties.attacker_id.type,
      'string',
      `the rendered invocation schema is the module manifest: ${detail.text}`,
    );
    assert.ok(detail.payload.schemas.invocation.properties.defender_id, detail.text);

    const absentModule = await jsonFetch(`${baseUrl}/v1/daemon/compute/modules/module_absent`);
    assert.equal(absentModule.status, 404, absentModule.text);
    assert.equal(absentModule.payload.error.code, 'not_found', absentModule.text);

    const { worldId, attackerId, defenderId } = await stageCombatWorld(baseUrl, 'Compute HTTP world');

    const run = await runModule(baseUrl, worldId, attackerId, defenderId);
    assert.equal(run.status, 200, run.text);
    assert.equal(run.payload.status, 'succeeded', run.text);
    assert.equal(run.payload.module_id, MODULE);
    assert.equal(run.payload.module_version, '1.0.0');
    const runId = run.payload.run_id;
    assert.ok(runId?.startsWith('run_'), `a real run id: ${run.text}`);
    const proposals = run.payload.proposals;
    assert.equal(proposals.battle_report.kind, 'combat');
    assert.equal(proposals.state_delta.length, 1);
    assert.equal(proposals.state_delta[0].op, 'sub');
    assert.equal(proposals.state_delta[0].path, 'character.current_hp');
    assert.equal(proposals.state_delta[0].target_key_block_id, defenderId);
    assert.equal(proposals.state_delta[0].value, 15);
    assert.equal(proposals.timeline_events.length, 1);
    assert.equal(
      proposals.timeline_events[0].summary,
      `${attackerId} struck ${defenderId} for 15 (30 -> 15 hp)`,
      `the module authored this summary: ${run.text}`,
    );

    const untouched = await readKeyBlockState(baseUrl, worldId, defenderId);
    assert.equal(untouched.is_computable, true);
    assert.equal(untouched.state.character.current_hp, 30);
    assert.deepEqual(await readComputeTimeline(baseUrl, worldId), []);

    const runDetail = await jsonFetch(`${baseUrl}/v1/daemon/compute/runs/${runId}`);
    assert.equal(runDetail.status, 200, runDetail.text);
    assert.equal(runDetail.payload.run_id, runId);
    assert.equal(runDetail.payload.status, 'succeeded');
    assert.equal(runDetail.payload.world_id, worldId);
    assert.equal(runDetail.payload.invocation_params.defender_id, defenderId);
    assert.equal(runDetail.payload.proposals.state_delta[0].value, 15);
    assert.equal(
      runDetail.payload.proposals.timeline_events[0].summary,
      `${attackerId} struck ${defenderId} for 15 (30 -> 15 hp)`,
    );

    const history = await jsonFetch(
      `${baseUrl}/v1/daemon/compute/runs?world_id=${worldId}&module_id=${MODULE}&status=succeeded`,
    );
    assert.equal(history.status, 200, history.text);
    assert.ok(
      history.payload.items.some((item) => item.run_id === runId),
      `the new run is listed under its filters: ${history.text}`,
    );
    assert.equal(
      history.payload.items.every((item) => item.world_id === worldId),
      true,
      history.text,
    );

    const accepted = await jsonFetch(`${baseUrl}/v1/daemon/compute/runs/${runId}/accept`, {
      method: 'POST',
      body: {},
    });
    assert.equal(accepted.status, 200, accepted.text);
    assert.equal(accepted.payload.status, 'applied');
    assert.equal(accepted.payload.applied.state_delta_count, 1);
    assert.equal(accepted.payload.applied.events_created, 1);
    assert.equal(accepted.payload.timeline_event_ids.length, 1);

    const applied = await readKeyBlockState(baseUrl, worldId, defenderId);
    assert.equal(applied.state.character.current_hp, 15, `damage 20−5 must land: ${JSON.stringify(applied)}`);
    const attackerUntouched = await readKeyBlockState(baseUrl, worldId, attackerId);
    assert.equal(attackerUntouched.state.character.current_hp, 100);

    const timeline = await readComputeTimeline(baseUrl, worldId);
    assert.equal(timeline.length, 1, JSON.stringify(timeline));
    assert.equal(timeline[0].id, accepted.payload.timeline_event_ids[0]);
    assert.equal(timeline[0].event_type, 'compute_result');
    assert.equal(timeline[0].status, 'canon');
    assert.ok(
      timeline[0].branch_id.startsWith('fbk_root'),
      `the accepted event lands on the World root branch: ${JSON.stringify(timeline[0])}`,
    );
    assert.ok(
      timeline[0].affected_key_block_ids.includes(defenderId),
      `the event names the entities it moved: ${JSON.stringify(timeline[0])}`,
    );

    const second = await jsonFetch(`${baseUrl}/v1/daemon/compute/runs/${runId}/accept`, {
      method: 'POST',
      body: {},
    });
    assert.equal(second.status, 409, second.text);
    assert.equal(second.payload.error.details?.wire_code, 'conflict', second.text);
    const afterSecond = await readKeyBlockState(baseUrl, worldId, defenderId);
    assert.equal(
      afterSecond.state?.character?.current_hp,
      15,
      `no double apply: ${JSON.stringify(afterSecond)}`,
    );
    assert.equal((await readComputeTimeline(baseUrl, worldId)).length, 1, 'no duplicate event');

    const appliedDetail = await jsonFetch(`${baseUrl}/v1/daemon/compute/runs/${runId}`);
    assert.equal(appliedDetail.status, 200, appliedDetail.text);
    assert.equal(appliedDetail.payload.status, 'applied');
    assert.equal(appliedDetail.payload.proposals.state_delta[0].value, 15);
  });

  test('compute lifecycle: a schema-invalid run persists as a failed row with its input detail', async () => {
    // The violation deliberately lives in `body.state`, not in
    // `body.attributes`: the delivered `ComputeInput.key_blocks` carries the
    // spoke ERC721 attributes ARRAY, which the manifest declares as an accepted
    // form (`type: ["object","array"]`), whereas the state map is carried
    // verbatim and its `character.current_hp` integer minimum applies.
    const worldId = await createWorld(baseUrl, 'Compute invalid-input world');
    const { attackerId, defenderId } = combatantIds(worldId);
    await stageCombatant(baseUrl, worldId, attackerId, 'Wounded Striker', {
      maxHp: 100,
      baseAtk: 20,
      baseDef: 3,
      currentHp: -5,
    });
    await stageCombatant(baseUrl, worldId, defenderId, 'Guardian', {
      maxHp: 50,
      baseAtk: 10,
      baseDef: 5,
      currentHp: 30,
    });

    const refused = await runModule(baseUrl, worldId, attackerId, defenderId);
    assert.equal(refused.status, 422, refused.text);
    assert.equal(refused.payload.error.code, 'invalid_input', refused.text);
    const entries = refused.payload.error.details?.invalid_entries;
    assert.ok(Array.isArray(entries) && entries.length >= 1, refused.text);
    assert.equal(
      entries.some((entry) => entry.entry_id === attackerId),
      true,
      `the refusal names the offending entry: ${refused.text}`,
    );
    assert.equal(entries.every((entry) => typeof entry.reason === 'string'), true, refused.text);

    const failedList = await jsonFetch(
      `${baseUrl}/v1/daemon/compute/runs?world_id=${worldId}&status=failed`,
    );
    assert.equal(failedList.status, 200, failedList.text);
    assert.equal(failedList.payload.items.length, 1, failedList.text);
    const failedRow = failedList.payload.items[0];
    assert.equal(failedRow.status, 'failed', failedList.text);
    assert.equal(failedRow.world_id, worldId, failedList.text);
    assert.equal(failedRow.module_id, MODULE, failedList.text);
    assert.ok(failedRow.run_id?.startsWith('run_'), failedList.text);

    const failedDetail = await jsonFetch(
      `${baseUrl}/v1/daemon/compute/runs/${failedRow.run_id}`,
    );
    assert.equal(failedDetail.status, 200, failedDetail.text);
    assert.equal(failedDetail.payload.status, 'failed', failedDetail.text);
    assert.equal(failedDetail.payload.error.code, 'invalid_input', failedDetail.text);
    const detailEntries = failedDetail.payload.error.details?.invalid_entries;
    assert.ok(Array.isArray(detailEntries) && detailEntries.length >= 1, failedDetail.text);
    assert.equal(
      detailEntries.some((entry) => entry.entry_id === attackerId),
      true,
      `the durable failure detail survives the read: ${failedDetail.text}`,
    );

    const defenderState = await readKeyBlockState(baseUrl, worldId, defenderId);
    assert.equal(defenderState.state.character.current_hp, 30);
    assert.deepEqual(await readComputeTimeline(baseUrl, worldId), []);
    const acceptFailed = await jsonFetch(
      `${baseUrl}/v1/daemon/compute/runs/${failedRow.run_id}/accept`,
      { method: 'POST', body: {} },
    );
    assert.equal(acceptFailed.status, 422, acceptFailed.text);
    assert.equal(acceptFailed.payload.error.details?.wire_code, 'invalid_state', acceptFailed.text);
    assert.equal((await readComputeTimeline(baseUrl, worldId)).length, 0);
  });

  test('compute ownership: foreign scope and malformed queries refuse without side effects', async () => {
    const { worldId, attackerId, defenderId } = await stageCombatWorld(
      baseUrl,
      'Compute ownership world',
    );

    const foreign = await runModule(baseUrl, FOREIGN_WORLD, attackerId, defenderId);
    assert.equal(foreign.status, 403, foreign.text);
    assert.equal(foreign.payload.error.code, 'forbidden', foreign.text);
    const absentWorld = await runModule(baseUrl, 'wld_absent', attackerId, defenderId);
    assert.equal(absentWorld.status, 403, absentWorld.text);

    const unknownModule = await jsonFetch(`${baseUrl}/v1/daemon/compute/run`, {
      method: 'POST',
      body: {
        world_id: worldId,
        module_id: 'module_absent',
        invocation_params: { attacker_id: attackerId, defender_id: defenderId },
      },
    });
    assert.equal(unknownModule.status, 404, unknownModule.text);
    assert.equal(unknownModule.payload.error.code, 'not_found', unknownModule.text);

    const missingScope = await jsonFetch(`${baseUrl}/v1/daemon/compute/run`, {
      method: 'POST',
      body: { module_id: MODULE },
    });
    assert.equal(missingScope.status, 400, missingScope.text);
    assert.equal(missingScope.payload.error.code, 'invalid_input', missingScope.text);
    const negativeLimit = await jsonFetch(`${baseUrl}/v1/daemon/compute/runs?limit=-1`);
    assert.equal(negativeLimit.status, 400, negativeLimit.text);
    assert.equal(negativeLimit.payload.error.code, 'invalid_input', negativeLimit.text);
    const unknownKey = await jsonFetch(`${baseUrl}/v1/daemon/compute/runs?world_id=${worldId}&wat=1`);
    assert.equal(unknownKey.status, 400, unknownKey.text);
    assert.equal(unknownKey.payload.error.code, 'invalid_input', unknownKey.text);
    const badStatus = await jsonFetch(`${baseUrl}/v1/daemon/compute/runs?status=not_a_status`);
    assert.equal(badStatus.status, 400, badStatus.text);
    assert.equal(badStatus.payload.error.code, 'invalid_input', badStatus.text);

    const foreignHistory = await jsonFetch(
      `${baseUrl}/v1/daemon/compute/runs?world_id=${FOREIGN_WORLD}`,
    );
    assert.equal(foreignHistory.status, 200, foreignHistory.text);
    assert.deepEqual(foreignHistory.payload.items, []);
    for (const path of ['runs/run_absent', 'runs/run_absent/accept', 'runs/run_absent/discard']) {
      const missing = await jsonFetch(`${baseUrl}/v1/daemon/compute/${path}`, {
        method: path === 'runs/run_absent' ? 'GET' : 'POST',
        ...(path === 'runs/run_absent' ? {} : { body: {} }),
      });
      assert.equal(missing.status, 404, `${path}: ${missing.text}`);
      assert.equal(missing.payload.error.code, 'not_found', `${path}: ${missing.text}`);
    }

    const untouched = await readKeyBlockState(baseUrl, worldId, defenderId);
    assert.equal(untouched.state.character.current_hp, 30);
    assert.deepEqual(await readComputeTimeline(baseUrl, worldId), []);
  });

  test('compute clear: World-scoped terminal clear keeps accepted effects', async () => {
    const { worldId, attackerId, defenderId } = await stageCombatWorld(baseUrl, 'Compute clear world');

    const acceptedRun = await runModule(baseUrl, worldId, attackerId, defenderId);
    assert.equal(acceptedRun.payload.status, 'succeeded', acceptedRun.text);
    const appliedRunId = acceptedRun.payload.run_id;
    const accepted = await jsonFetch(`${baseUrl}/v1/daemon/compute/runs/${appliedRunId}/accept`, {
      method: 'POST',
      body: {},
    });
    assert.equal(accepted.status, 200, accepted.text);

    const discardRun = await runModule(baseUrl, worldId, attackerId, defenderId);
    assert.equal(discardRun.payload.status, 'succeeded', discardRun.text);
    const discardedRunId = discardRun.payload.run_id;
    const discarded = await jsonFetch(
      `${baseUrl}/v1/daemon/compute/runs/${discardedRunId}/discard`,
      { method: 'POST' },
    );
    assert.equal(discarded.status, 200, discarded.text);
    assert.deepEqual(discarded.payload, { run_id: discardedRunId, status: 'discarded' });
    const afterDiscard = await readKeyBlockState(baseUrl, worldId, defenderId);
    assert.equal(afterDiscard.state.character.current_hp, 15, 'discard commits nothing');
    assert.equal((await readComputeTimeline(baseUrl, worldId)).length, 1);
    const secondDiscard = await jsonFetch(
      `${baseUrl}/v1/daemon/compute/runs/${discardedRunId}/discard`,
      { method: 'POST' },
    );
    assert.equal(secondDiscard.status, 409, secondDiscard.text);
    assert.equal(secondDiscard.payload.error.details?.wire_code, 'conflict', secondDiscard.text);

    const malformed = [
      ['/v1/daemon/compute/runs', 'missing World scope'],
      [`/v1/daemon/compute/runs?world_id=${worldId}&status=running`, 'non-terminal status'],
      [`/v1/daemon/compute/runs?world_id=${worldId}&status=succeeded`, 'needs-review status'],
      [`/v1/daemon/compute/runs?world_id=${worldId}&statuses=applied`, 'unknown key'],
    ];
    for (const [path, label] of malformed) {
      const refused = await jsonFetch(`${baseUrl}${path}`, { method: 'DELETE' });
      assert.equal(refused.status, 422, `${label}: ${refused.text}`);
      assert.equal(refused.payload.error.code, 'invalid_input', `${label}: ${refused.text}`);
    }
    for (const runId of [appliedRunId, discardedRunId]) {
      const survived = await jsonFetch(`${baseUrl}/v1/daemon/compute/runs/${runId}`);
      assert.equal(survived.status, 200, `${runId} must survive a malformed clear: ${survived.text}`);
    }

    const clearedApplied = await jsonFetch(
      `${baseUrl}/v1/daemon/compute/runs?world_id=${worldId}&status=applied`,
      { method: 'DELETE' },
    );
    assert.equal(clearedApplied.status, 200, clearedApplied.text);
    assert.deepEqual(clearedApplied.payload, { deleted: 1 });
    const appliedGone = await jsonFetch(`${baseUrl}/v1/daemon/compute/runs/${appliedRunId}`);
    assert.equal(appliedGone.status, 404, appliedGone.text);
    const discardedSurvived = await jsonFetch(
      `${baseUrl}/v1/daemon/compute/runs/${discardedRunId}`,
    );
    assert.equal(discardedSurvived.status, 200, discardedSurvived.text);

    const effectStands = await readKeyBlockState(baseUrl, worldId, defenderId);
    assert.equal(effectStands.state.character.current_hp, 15);
    const timelineStands = await readComputeTimeline(baseUrl, worldId);
    assert.equal(timelineStands.length, 1);

    const clearedRest = await jsonFetch(`${baseUrl}/v1/daemon/compute/runs?world_id=${worldId}`, {
      method: 'DELETE',
    });
    assert.equal(clearedRest.status, 200, clearedRest.text);
    assert.deepEqual(clearedRest.payload, { deleted: 1 });
    const history = await jsonFetch(`${baseUrl}/v1/daemon/compute/runs?world_id=${worldId}`);
    assert.equal(history.status, 200, history.text);
    assert.deepEqual(history.payload.items, []);
    const effectSurvivesClear = await readKeyBlockState(baseUrl, worldId, defenderId);
    assert.equal(effectSurvivesClear.state.character.current_hp, 15);
    assert.equal((await readComputeTimeline(baseUrl, worldId)).length, 1);

    const foreignClear = await jsonFetch(
      `${baseUrl}/v1/daemon/compute/runs?world_id=${FOREIGN_WORLD}`,
      { method: 'DELETE' },
    );
    assert.equal(foreignClear.status, 403, foreignClear.text);
    assert.equal(foreignClear.payload.error.code, 'forbidden', foreignClear.text);
  });
});
