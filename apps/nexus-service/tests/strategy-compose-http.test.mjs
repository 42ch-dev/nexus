/**
 * v1.239 P3-T7/T8: `POST /v1/daemon/strategies/{id}/compose` and the simulate fence over a
 * composed candidate.
 *
 * Every assertion here runs over the real service: a disposable home seeded by the native
 * fixture binary, a small pattern registry written into `<home>/.nexus42/patterns`, and the
 * actual native core behind the route. The compose route is a write, so a stubbed core would
 * prove only passthrough — the envelope details, the unchanged candidate on every refusal and
 * the recomputed simulation hash are the contract, and only the real store can show them.
 *
 * Nothing here reads or writes the developer's own `~/.nexus42`.
 */
import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { after, before, describe, test } from 'node:test';

const serviceRoot = join(dirname(fileURLToPath(import.meta.url)), '..');

/** An embedded preset, so the revision-0 candidate exists without a shipped bundle. */
const STRATEGY = 'novel-writing';
const COMPOSE_PATH = `/v1/daemon/strategies/${STRATEGY}/compose`;
/**
 * The fence is proven on an embedded preset the static walk already accepts. `novel-writing`
 * (the T7 strategy) imports judged states carrying `linear` edges, so the reachability walk
 * refuses it before compose is involved — a separate, pre-existing import/simulate mismatch.
 */
const WALKABLE_STRATEGY = 'novel-project-init';
const WALKABLE_COMPOSE_PATH = `/v1/daemon/strategies/${WALKABLE_STRATEGY}/compose`;
const WALKABLE_SIMULATE_PATH = `/v1/daemon/strategies/${WALKABLE_STRATEGY}/simulate`;

const HASH = /^[0-9a-f]{64}$/;

/** One package directory: `pattern.toml` plus the IR fragment it pins. */
function writePackage(home, { id, effectClass, slots = [], fragment }) {
  const directory = join(home, '.nexus42', 'patterns', id);
  mkdirSync(directory, { recursive: true });
  // The manifest requires `slots`; an empty declaration is a top-level key, never absent.
  const declaredSlots =
    slots.length === 0
      ? 'slots = []\n'
      : slots
          .map(
            (slot) =>
              `\n[[slots]]\nname = "${slot.name}"\nkind = "${slot.kind}"\nrequired = ${slot.required}\n`,
          )
          .join('');
  writeFileSync(
    join(directory, 'pattern.toml'),
    `id = "${id}"\nversion = "1.0.0"\ndescription = "compose fixture ${id}"\n` +
      `effect_class = "${effectClass}"\nrequires_capabilities = []\n${declaredSlots}\n` +
      `[ir_compat]\nmin = 0\nmax = 0\n`,
  );
  writeFileSync(join(directory, 'workflow.ir.json'), JSON.stringify(fragment, null, 2));
}

/** A fragment whose single node declares `effectClass`; `nests` pins further packages. */
function fragment(id, effectClass, nests = []) {
  return {
    ir_version: 0,
    id,
    version: 1,
    revision: 0,
    state: 'candidate',
    meta: {
      description: `compose fixture ${id}`,
      kind: 'creator',
      run_intents: [],
      requires_capabilities: [],
    },
    entry: 'slot',
    terminal: ['slot'],
    nodes: [{ id: 'slot', kind: 'pattern_slot', effect_class: effectClass }],
    edges: [],
    nests,
    assets: [],
    signals: [],
    roles: [],
    effect_summary: effectClass,
    provenance: { source: 'compose' },
  };
}

/** A fragment that pins `next`, so two of them can close a cycle or form a depth chain. */
function pinned(id, next) {
  return fragment(id, 'pure', [
    {
      id: `${id}-nest`,
      parent_node: 'slot',
      expansion: 'defer_until_apply',
      pin_policy: 'exact',
      child: { kind: 'pattern', registry_id: next, version: '1.0.0' },
    },
  ]);
}

/** The registry the suite composes from. */
function writeRegistry(home) {
  writePackage(home, {
    id: 'prompt-step',
    effectClass: 'external',
    slots: [{ name: 'topic', kind: 'string', required: true }],
    fragment: fragment('prompt-step', 'external'),
  });
  writePackage(home, {
    id: 'judge-step',
    effectClass: 'read',
    slots: [
      { name: 'topic', kind: 'string', required: true },
      { name: 'rubric', kind: 'asset', required: false },
    ],
    fragment: fragment('judge-step', 'read'),
  });
  writePackage(home, { id: 'cycle-a', effectClass: 'pure', fragment: pinned('cycle-a', 'cycle-b') });
  writePackage(home, { id: 'cycle-b', effectClass: 'pure', fragment: pinned('cycle-b', 'cycle-a') });
  writePackage(home, { id: 'chain-a', effectClass: 'pure', fragment: pinned('chain-a', 'chain-b') });
  writePackage(home, { id: 'chain-b', effectClass: 'pure', fragment: pinned('chain-b', 'chain-c') });
  writePackage(home, { id: 'chain-c', effectClass: 'pure', fragment: pinned('chain-c', 'chain-d') });
  writePackage(home, { id: 'chain-d', effectClass: 'pure', fragment: fragment('chain-d', 'pure') });
}

function nest(baseRevision, { nestId, parentNode, packageId, version = '1.0.0' }) {
  return {
    base_revision: baseRevision,
    op: 'nest',
    payload: {
      nest_id: nestId,
      parent_node: parentNode,
      package: { id: packageId, version },
      expansion: 'expand_in_candidate',
    },
  };
}

const node = (document, id) => document.nodes.find((entry) => entry.id === id);
const binding = (document, id) => document.nests.find((entry) => entry.id === id);
const pin = (value) => ({ registry_id: value.registry_id, version: value.version });

describe('compose route (v1.239 P3-T7)', () => {
  let home;
  let service;
  /**
   * The candidate revision this suite last committed. A candidate has no read route yet, so
   * each successful compose advances the shared base and every refusal is asserted to leave it
   * exactly where it was — the revision is the store's own CAS authority, so this is the
   * strongest "nothing was written" evidence the wire exposes.
   */
  let revision = 0;

  before(async () => {
    home = mkdtempSync(join(tmpdir(), 'nexus-service-compose-'));
    const seed = spawnSync(
      'cargo',
      ['run', '-q', '-p', 'nexus-core-node', '--bin', 'native-wire-fixture-seed', '--', home],
      { stdio: 'inherit' },
    );
    assert.equal(seed.status, 0, seed.stderr?.toString());
    writeRegistry(home);

    const build = spawnSync('npx', ['tsc', '-p', 'tsconfig.json'], {
      cwd: serviceRoot,
      stdio: 'inherit',
    });
    assert.equal(build.status, 0);
    const { startService } = await import(join(serviceRoot, 'dist', 'index.js'));
    service = await startService({
      home,
      host: '127.0.0.1',
      port: 0,
      allowRemote: false,
      domainOnly: true,
    });
  });

  after(async () => {
    if (service) await service.close();
    if (home) rmSync(home, { recursive: true, force: true });
  });

  async function post(path, body) {
    const response = await fetch(`${service.url}${path}`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(body),
    });
    const text = await response.text();
    return { status: response.status, payload: text.length > 0 ? JSON.parse(text) : null };
  }

  /** Compose one operation and advance the shared base; the committed document is returned. */
  async function compose(body) {
    const { status, payload } = await post(COMPOSE_PATH, body);
    assert.equal(status, 200, JSON.stringify(payload));
    assert.equal(payload.new_revision, body.base_revision + 1);
    assert.equal(payload.document.revision, payload.new_revision);
    revision = payload.new_revision;
    return payload;
  }

  test('nest commits one revision with the resolved pin and names the unbound slot', async () => {
    const response = await compose(
      nest(0, { nestId: 'source-research', parentNode: 'collect-sources', packageId: 'prompt-step' }),
    );

    const slot = node(response.document, 'collect-sources');
    assert.equal(slot.kind, 'pattern_slot');
    assert.equal(slot.effect_class, 'external');
    assert.match(slot.pattern.content_hash, HASH);
    assert.deepEqual(pin(slot.pattern), { registry_id: 'prompt-step', version: '1.0.0' });

    const region = binding(response.document, 'source-research');
    assert.equal(region.parent_node, 'collect-sources');
    assert.equal(region.expansion, 'expand_in_candidate');
    assert.equal(region.pin_policy, 'exact');
    assert.deepEqual(region.child.content_hash, slot.pattern.content_hash);
    assert.deepEqual(pin(region.child), pin(slot.pattern));

    // A required slot with no binding is a named diagnostic, never a default.
    assert.deepEqual(response.validation_summary, [
      {
        code: 'unbound_required_slot',
        nest_id: 'source-research',
        package_id: 'prompt-step',
        package_version: '1.0.0',
        slot: 'topic',
        message:
          'nest `source-research`: pattern `prompt-step@1.0.0` requires slot `topic`, which is unbound',
      },
    ]);
  });

  test('bind_slot writes the value and clears its diagnostic', async () => {
    const response = await compose({
      base_revision: revision,
      op: 'bind_slot',
      payload: { nest_id: 'source-research', slot: 'topic', value: 'water policy' },
    });

    assert.equal(node(response.document, 'collect-sources').bind.topic, 'water policy');
    assert.deepEqual(response.validation_summary, []);
  });

  test('replace_subtree re-pins the region in place and keeps its bindings', async () => {
    const before = await post(COMPOSE_PATH, nest(revision, {
      nestId: 'swap',
      parentNode: 'swap-slot',
      packageId: 'prompt-step',
    }));
    assert.equal(before.status, 200, JSON.stringify(before.payload));
    revision = before.payload.new_revision;
    const bound = await compose({
      base_revision: revision,
      op: 'bind_slot',
      payload: { nest_id: 'swap', slot: 'topic', value: 'keep me' },
    });
    assert.equal(node(bound.document, 'swap-slot').bind.topic, 'keep me');

    const replaced = await compose({
      base_revision: revision,
      op: 'replace_subtree',
      payload: {
        nest_id: 'swap',
        package: { id: 'judge-step', version: '1.0.0' },
        expansion: 'expand_in_candidate',
      },
    });

    assert.deepEqual(pin(binding(replaced.document, 'swap').child), {
      registry_id: 'judge-step',
      version: '1.0.0',
    });
    assert.deepEqual(pin(node(replaced.document, 'swap-slot').pattern), {
      registry_id: 'judge-step',
      version: '1.0.0',
    });
    assert.equal(node(replaced.document, 'swap-slot').bind.topic, 'keep me');
    // Only the swapped region moved: the other region is untouched.
    assert.deepEqual(binding(replaced.document, 'source-research'), binding(bound.document, 'source-research'));
    assert.deepEqual(replaced.document.edges, bound.document.edges);
  });

  test('unnest flattens the pinned body and names the binding it dropped', async () => {
    const response = await compose({
      base_revision: revision,
      op: 'unnest',
      payload: { nest_id: 'swap' },
    });

    assert.equal(binding(response.document, 'swap'), undefined);
    assert.equal(node(response.document, 'swap-slot'), undefined);
    assert.ok(binding(response.document, 'source-research'), 'the untouched region stays');
    assert.ok(node(response.document, 'collect-sources'));
    // The swapped package's body is a candidate node at this level now, so the slot bindings the
    // collapsed node carried are dropped and named rather than defaulted away.
    assert.equal(
      response.document.nodes.filter((entry) => entry.kind === 'pattern_slot').length,
      2,
      JSON.stringify(response.document.nodes),
    );
    assert.deepEqual(response.validation_summary, [
      {
        code: 'slot_binding_dropped',
        nest_id: 'swap',
        slot: 'topic',
        message:
          'nest `swap`: flattening dropped slot binding `topic`, which the flattened fragment does not address',
      },
    ]);
  });

  test('a stale base is the typed conflict, writes nothing and names the current revision', async () => {
    const stale = nest(revision - 1, {
      nestId: 'stale',
      parentNode: 'stale-slot',
      packageId: 'prompt-step',
    });

    const refused = await post(COMPOSE_PATH, stale);
    assert.equal(refused.status, 409, JSON.stringify(refused.payload));
    assert.equal(refused.payload.success, false);
    assert.equal(refused.payload.error.code, 'strategy_candidate_conflict');
    assert.equal(
      refused.payload.error.message,
      `Candidate changed before this edit. Reload revision ${revision} and reapply the change.`,
    );
    assert.deepEqual(refused.payload.error.details, {
      strategy_id: STRATEGY,
      base_revision: revision - 1,
      current_revision: revision,
      conflicting_path: '$',
      recovery_hint: 'Reload the current candidate and explicitly reapply the compose operation.',
    });

    // The refused request consumed no revision: the same stale base still reads the same answer.
    const again = await post(COMPOSE_PATH, stale);
    assert.equal(again.status, 409);
    assert.equal(again.payload.error.details.current_revision, revision);

    // And the candidate still commits from the revision the conflict reported.
    const committed = await compose(nest(revision, {
      nestId: 'after-conflict',
      parentNode: 'after-conflict-slot',
      packageId: 'prompt-step',
    }));
    assert.ok(node(committed.document, 'stale-slot') === undefined, 'the refused op left no trace');
  });

  test('a nest cycle is a named 422 that leaves the candidate unchanged', async () => {
    const cyclic = nest(revision, {
      nestId: 'cycle-nest',
      parentNode: 'cycle-slot',
      packageId: 'cycle-a',
    });

    const refused = await post(COMPOSE_PATH, cyclic);
    assert.equal(refused.status, 422, JSON.stringify(refused.payload));
    assert.equal(refused.payload.error.code, 'invalid_input');
    assert.equal(refused.payload.error.details.code, 'compose_refused');
    assert.equal(refused.payload.error.details.reason, 'nest_cycle');
    assert.equal(refused.payload.error.details.nest_id, 'cycle-nest');
    assert.deepEqual(refused.payload.error.details.path, [
      'cycle-a@1.0.0',
      'cycle-b@1.0.0',
      'cycle-a@1.0.0',
    ]);

    // Same base, same refusal: no revision was consumed.
    const again = await post(COMPOSE_PATH, cyclic);
    assert.equal(again.status, 422);
    assert.equal(again.payload.error.details.reason, 'nest_cycle');
  });

  test('a nest past the depth bound is a named 422 that leaves the candidate unchanged', async () => {
    // chain-b nests chain-c which nests chain-d: exactly three levels, accepted.
    await compose(nest(revision, {
      nestId: 'depth-ok',
      parentNode: 'depth-slot',
      packageId: 'chain-b',
    }));

    const tooDeep = nest(revision, {
      nestId: 'depth-breach',
      parentNode: 'too-deep-slot',
      packageId: 'chain-a',
    });
    const refused = await post(COMPOSE_PATH, tooDeep);
    assert.equal(refused.status, 422, JSON.stringify(refused.payload));
    assert.equal(refused.payload.error.details.reason, 'nest_depth_exceeded');
    assert.equal(refused.payload.error.details.nest_id, 'depth-breach');
    assert.equal(refused.payload.error.details.depth, 4);

    const again = await post(COMPOSE_PATH, tooDeep);
    assert.equal(again.status, 422);
    assert.equal(again.payload.error.details.depth, 4);
  });

  test('an unavailable package is refused naming the requested id and version', async () => {
    const missing = nest(revision, {
      nestId: 'missing-package',
      parentNode: 'missing-slot',
      packageId: 'prompt-step',
      version: '9.9.9',
    });

    const refused = await post(COMPOSE_PATH, missing);
    assert.equal(refused.status, 422, JSON.stringify(refused.payload));
    assert.equal(refused.payload.error.details.reason, 'package_unavailable');
    assert.equal(refused.payload.error.details.nest_id, 'missing-package');
    assert.equal(refused.payload.error.details.package_id, 'prompt-step');
    assert.equal(refused.payload.error.details.package_version, '9.9.9');
    // The reader's own reason travels in the refusal message, naming the requested pin.
    assert.match(refused.payload.error.details.message, /prompt-step@9\.9\.9/);

    const again = await post(COMPOSE_PATH, missing);
    assert.equal(again.status, 422, 'no revision was consumed');
  });

  test('a malformed body is a 400, not a refusal envelope', async () => {
    const { status, payload } = await post(COMPOSE_PATH, { op: 'nest', payload: {} });
    assert.equal(status, 400, JSON.stringify(payload));
    assert.equal(payload.error.code, 'invalid_input');
  });
});

describe('simulate over a composed candidate (v1.239 P3-T8)', () => {
  let home;
  let service;
  let revision = 0;

  before(async () => {
    home = mkdtempSync(join(tmpdir(), 'nexus-service-compose-simulate-'));
    const seed = spawnSync(
      'cargo',
      ['run', '-q', '-p', 'nexus-core-node', '--bin', 'native-wire-fixture-seed', '--', home],
      { stdio: 'inherit' },
    );
    assert.equal(seed.status, 0, seed.stderr?.toString());
    writeRegistry(home);

    const build = spawnSync('npx', ['tsc', '-p', 'tsconfig.json'], {
      cwd: serviceRoot,
      stdio: 'inherit',
    });
    assert.equal(build.status, 0);
    const { startService } = await import(join(serviceRoot, 'dist', 'index.js'));
    service = await startService({
      home,
      host: '127.0.0.1',
      port: 0,
      allowRemote: false,
      domainOnly: true,
    });
  });

  after(async () => {
    if (service) await service.close();
    if (home) rmSync(home, { recursive: true, force: true });
  });

  async function post(path, body) {
    const response = await fetch(`${service.url}${path}`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(body),
    });
    const text = await response.text();
    return { status: response.status, payload: text.length > 0 ? JSON.parse(text) : null };
  }

  async function compose(body) {
    const { status, payload } = await post(WALKABLE_COMPOSE_PATH, body);
    assert.equal(status, 200, JSON.stringify(payload));
    revision = payload.new_revision;
    return payload;
  }

  let composed;

  test('the fence accepts the composed document unchanged', async () => {
    composed = await compose(
      nest(0, {
        nestId: 'source-research',
        parentNode: 'collect-sources',
        packageId: 'prompt-step',
      }),
    );
    const document = composed.document;

    const first = await post(WALKABLE_SIMULATE_PATH, { document });
    assert.equal(first.status, 200, JSON.stringify(first.payload));
    assert.match(first.payload.ir_content_hash, HASH);
    assert.equal(first.payload.simulation_level, 'reachability');
    assert.equal(first.payload.schema_version, 1);

    // The hash is recomputed from the body, so the same document yields the same hash and a
    // changed document yields a different one.
    const again = await post(WALKABLE_SIMULATE_PATH, { document });
    assert.equal(again.payload.ir_content_hash, first.payload.ir_content_hash);

    const unnest = await compose({
      base_revision: revision,
      op: 'unnest',
      payload: { nest_id: 'source-research' },
    });
    const withoutSlot = await post(WALKABLE_SIMULATE_PATH, { document: unnest.document });
    assert.equal(withoutSlot.status, 200);
    assert.notEqual(withoutSlot.payload.ir_content_hash, first.payload.ir_content_hash);

    // Nothing persisted and no revision consumed: the candidate still commits from `revision`.
    const stillThere = await compose(nest(revision, {
      nestId: 'fence-check',
      parentNode: 'fence-check-slot',
      packageId: 'prompt-step',
    }));
    assert.equal(stillThere.new_revision, revision);
  });

  test('a slot the graph wires is named by the trajectory', async () => {
    const document = composed.document;
    const wired = {
      ...document,
      edges: [
        ...document.edges,
        {
          id: `${document.entry}->collect-sources`,
          from: document.entry,
          to: 'collect-sources',
          kind: 'linear',
        },
      ],
    };

    const trajectory = await post(WALKABLE_SIMULATE_PATH, { document: wired });
    assert.equal(trajectory.status, 200, JSON.stringify(trajectory.payload));
    assert.ok(
      trajectory.payload.nodes_visited.includes('collect-sources'),
      `the walk names the pattern slot: ${JSON.stringify(trajectory.payload.nodes_visited)}`,
    );
    // The slot contributes its resolved pin's declared class, not the fragment body.
    assert.equal(trajectory.payload.effect_summary, 'external');
  });

  test('the path id and the candidate state are still enforced', async () => {
    const mismatch = await post(WALKABLE_SIMULATE_PATH, {
      document: { ...composed.document, id: 'another-strategy' },
    });
    assert.equal(mismatch.status, 400, JSON.stringify(mismatch.payload));
    assert.equal(mismatch.payload.error.code, 'invalid_input');
    assert.equal(mismatch.payload.error.details.wire_code, 'strategy_id_mismatch');

    const approved = await post(WALKABLE_SIMULATE_PATH, {
      document: { ...composed.document, state: 'approved' },
    });
    assert.equal(approved.status, 400, JSON.stringify(approved.payload));
    assert.equal(approved.payload.error.code, 'invalid_input');
    assert.equal(approved.payload.error.details.wire_code, 'strategy_not_candidate');
  });
});
