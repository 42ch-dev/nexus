import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { mkdirSync, mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { after, before, describe, test } from 'node:test';
import { isNativeCoreErrorCode, openCore, parseNativeCoreError } from '../dist/index.js';

/**
 * P0-T5 bounded native target: the six contract §2 Actor Host methods over the
 * real native core, the real attached Host authority, and the real service
 * composition underneath.
 *
 * Fixtures are local and deterministic: one ephemeral home seeded by the
 * crate's own wire-fixture binary (an owned World plus its Creator) with the
 * selected-workspace registration the product's create path writes, and two
 * ACP provider families pointing at
 * `crates/nexus-agent-host/tests/fixtures/mock_acp_workflow.py` — a hermetic
 * python peer with no model and no network. Nothing is mocked: every prompt
 * below is executed by that peer process, and the observed message text is the
 * peer's own deterministic transformation of the prompt, so a facade echo
 * cannot pass this file. The `mock-acp-block` family never answers a prompt,
 * which is what makes cancellation and session shutdown observable instead of
 * racy.
 *
 * Readings are owner-bound and process-lifetime: the principal handle is
 * minted by the native open and re-verified per call, and a core-indexed or
 * tombstoned Actor id is refused on the lower-level provider lane.
 *
 * Run the native addon build once first (`node packages/nexus-native/scripts/build.mjs`);
 * this file rebuilds it in `before` so a lone run is still self-contained.
 */

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = join(__dirname, '..', '..', '..');
const fixture = resolve(root, 'crates/nexus-agent-host/tests/fixtures/mock_acp_workflow.py');
const CREATOR = 'ctr_testcreator';
const WORLD = 'wld_owned';
const MAIN_PROVIDER = 'mock-acp-main';
const BLOCK_PROVIDER = 'mock-acp-block';
const OPEN_OPTIONS = { user_home: '', access: 'engine_owner', allow_uninitialized: false };

/** The literal realpath of a binary, so a symlinked `which python3` still
 * resolves to the interpreter the config must name. */
function realpathOf(path) {
  return execFileSync('python3', ['-c', `import os,sys;print(os.path.realpath(sys.argv[1]))`, path], {
    encoding: 'utf8',
  }).trim();
}

/** One ephemeral home, seeded through the crate's own fixture binary, plus the
 * selected-workspace registration and the two provider families.
 *
 * The registration is the same on-disk layout the product's create path writes
 * (`operational_workspace_dir/meta.json` with `local_root`): an engine-owner
 * open pins that root as the Host's workspace boundary and probes the catalog
 * there, which is what makes an ACP family actually launchable — a seed without
 * it leaves every provider `probe_context_unavailable`. Per-family `env` rides
 * in the same `[[providers]]` element, which is how the block family gets its
 * never-answering prompt. */
function seedHome() {
  const home = mkdtempSync(join(tmpdir(), 'nexus-native-actor-host-'));
  const seed = spawnSync(
    'cargo',
    ['run', '-q', '-p', 'nexus-core-node', '--bin', 'native-wire-fixture-seed', '--', home],
    { cwd: root },
  );
  assert.equal(seed.status, 0, seed.stderr?.toString());
  const creativeRoot = join(home, 'creative', CREATOR, 'default');
  mkdirSync(creativeRoot, { recursive: true });
  const operational = join(home, '.nexus42', 'creators', CREATOR, 'workspaces', 'default');
  mkdirSync(operational, { recursive: true });
  writeFileSync(
    join(operational, 'meta.json'),
    JSON.stringify({
      schema_version: 1,
      creator_id: CREATOR,
      workspace_slug: 'default',
      local_root: creativeRoot,
      workspace_id: null,
      created_at: '2020-01-01T00:00:00Z',
    }),
  );
  const python = realpathOf(execFileSync('which', ['python3'], { encoding: 'utf8' }).trim());
  const log = join(home, 'acp-fixture.log');
  const families = [
    [MAIN_PROVIDER, { ACP_FIXTURE_LOG: log }],
    [BLOCK_PROVIDER, { ACP_FIXTURE_LOG: log, BLOCK_PROMPT: '1' }],
  ]
    .map(([id, env]) => {
      const envLines = Object.entries(env)
        .map(([key, value]) => `${key} = ${JSON.stringify(value)}`)
        .join('\n');
      return (
        `[[providers]]\nid = ${JSON.stringify(id)}\nprotocol = "acp"\n` +
        `command = ${JSON.stringify(python)}\nargs = [${JSON.stringify(fixture)}]\n` +
        `enabled = true\n[providers.env]\n${envLines}\n`
      );
    })
    .join('\n');
  const dir = join(home, '.nexus42', 'agent-host');
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, 'config.toml'), families);
  return { home, creativeRoot };
}

/** A real ACP provider engine behind the JS callbacks, counting how many calls
 * actually reach it so a native refusal can be proven to have stopped short of
 * the provider lane. */
async function actorProviders() {
  const { createAcpProvider } = await import('../../nexus-provider-acp/dist/index.js');
  const engine = createAcpProvider();
  const reached = { count: 0 };
  return {
    reached,
    callbacks: {
      call: async (request) => {
        reached.count += 1;
        return engine.call(request);
      },
      next: async (operationId, maxEvents, maxBytes) => {
        reached.count += 1;
        return engine.next(operationId, maxEvents, maxBytes);
      },
    },
  };
}

/** The authoritative Character outcome once the run left `running`. */
async function waitForOutcome(host, principal, operationId) {
  const deadline = Date.now() + 30_000;
  for (;;) {
    const outcome = await host.hostCharacterOperation(principal, operationId);
    if (outcome.run_status !== 'running') return outcome;
    assert.ok(
      Date.now() < deadline,
      `operation ${operationId} never settled: ${JSON.stringify(outcome)}`,
    );
    await new Promise((settle) => setTimeout(settle, 25));
  }
}

/** Accumulate the lower-level provider lane's batches until its terminal. */
async function drainRawOperation(host, operationId) {
  const texts = [];
  const deadline = Date.now() + 30_000;
  for (;;) {
    const batch = await host.nextProviderEvents(operationId, 16, 65_536);
    assert.equal(batch.operation_id, operationId);
    for (const event of batch.events) {
      if (event.MessageDelta) texts.push(event.MessageDelta.text);
      if (event.OpFinished) return { texts, reason: event.OpFinished.reason };
    }
    assert.ok(Date.now() < deadline, `raw operation ${operationId} never finished`);
    await new Promise((settle) => setTimeout(settle, 25));
  }
}

describe('native Actor Host surface (P0-T5)', { concurrency: 1 }, () => {
  let home;
  let creativeRoot;
  let core;
  let principal;
  let providers;
  let characterId;
  let bindingId;
  let mainSessionId;
  let mainOperationId;

  before(async () => {
    ({ home, creativeRoot } = seedHome());
    const addon = spawnSync('node', ['packages/nexus-native/scripts/build.mjs'], {
      cwd: root,
      stdio: 'inherit',
    });
    assert.equal(addon.status, 0, 'native addon build must succeed');
    const acp = spawnSync('pnpm', ['--filter', '@42ch/nexus-provider-acp', 'build'], {
      cwd: root,
      stdio: 'inherit',
    });
    assert.equal(acp.status, 0, 'ACP provider package build must succeed');
    providers = await actorProviders();
    core = await openCore({ ...OPEN_OPTIONS, user_home: home }, providers.callbacks);
    principal = await core.activePrincipal();
  });

  after(async () => {
    if (core) await core.close();
  });

  test('the six contract methods are exported and an Actor session runs to its real terminal', async () => {
    for (const name of [
      'hostCreateSession',
      'hostExecuteOperation',
      'hostCharacterOperation',
      'hostCancelOperation',
      'hostShutdownSession',
      'nextHostEvents',
    ]) {
      assert.equal(typeof core[name], 'function', `${name} must be exported through native`);
    }

    // A real owned Character + binding is created through the surface itself.
    const created = await core.createCharacter(principal, {
      world_id: WORLD,
      display_name: 'Native Actor Host',
      persona: { voice: 'plain' },
    });
    characterId = created.character.character_id;
    bindingId = created.binding.binding_id;
    assert.match(characterId, /^chr_[0-9a-f]{32}$/);
    assert.ok(bindingId, `initial binding missing: ${JSON.stringify(created)}`);

    const viewpoint = { world_id: WORLD, binding_id: bindingId };
    const session = await core.hostCreateSession(principal, {
      provider_id: MAIN_PROVIDER,
      // The session cwd must be inside the engine-owner Host's pinned workspace
      // boundary, which is the registered creative root (not the nexus root the
      // wire omits to).
      cwd: creativeRoot,
      actor_ref: { actor_kind: 'character', character_id: characterId },
      viewpoint,
    });
    mainSessionId = session.session_id;
    assert.equal(session.provider_id, MAIN_PROVIDER, JSON.stringify(session));
    assert.deepEqual(session.actor_ref, { actor_kind: 'character', character_id: characterId });
    assert.equal(session.viewpoint.world_id, WORLD);
    assert.equal(session.viewpoint.binding_id, bindingId);

    const prompt = 'native actor host proof';
    const started = await core.hostExecuteOperation(principal, mainSessionId, {
      kind: 'prompt',
      content: prompt,
    });
    mainOperationId = started.operation_id;
    assert.equal(started.session_id, mainSessionId);
    assert.equal(started.status, 'started');
    assert.match(mainOperationId, /^[0-9a-f-]{36}$/);

    const outcome = await waitForOutcome(core, principal, mainOperationId);
    assert.equal(outcome.session_id, mainSessionId);
    assert.equal(outcome.run_status, 'succeeded');
    assert.equal(outcome.finish_reason, 'end_turn');
    assert.deepEqual(outcome.capture, { status: 'disabled', pending_id: null, code: null });

    // The events are the local ACP peer's own transformation of the prompt and
    // its own end_turn — a fabricated projection cannot carry them.
    const batch = await core.nextHostEvents(principal, mainSessionId, mainOperationId, 16, 65_536);
    assert.equal(batch.operation_id, mainOperationId);
    const texts = batch.events.filter((event) => event.MessageDelta).map((e) => e.MessageDelta.text);
    assert.ok(
      texts.some((text) => text.includes(`transformed:${prompt}`)),
      `the peer's own message chunk must be observable: ${JSON.stringify(batch.events)}`,
    );
    assert.ok(
      batch.events.some((event) => event.OpFinished?.reason === 'end_turn'),
      JSON.stringify(batch.events),
    );

    // Owner-bound reads: another operation/session pair and an unknown id are
    // the same bounded not_found, never a cross-session leak.
    await assert.rejects(
      () => core.nextHostEvents(principal, mainSessionId, randomUUID(), 16, 65_536),
      (error) => isNativeCoreErrorCode(error, 'not_found'),
    );
    await assert.rejects(
      () => core.hostCharacterOperation(principal, randomUUID()),
      (error) => isNativeCoreErrorCode(error, 'not_found'),
    );
    // The principal handle is resolved and verified on every effect/read, so a
    // handle this open did not mint is refused before anything else runs.
    await assert.rejects(
      () => core.hostCharacterOperation(`p:9:${CREATOR}:default`, mainOperationId),
      /invalid principal handle/,
    );
    await assert.rejects(
      () => core.hostShutdownSession(`p:1:ctr_othercreator:default`, mainSessionId),
      /invalid principal handle/,
    );
  });

  test('an accepted cancellation is the operation truth, and a later cancel is the finished conflict', async () => {
    const session = await core.hostCreateSession(principal, {
      provider_id: BLOCK_PROVIDER,
      cwd: creativeRoot,
      actor_ref: { actor_kind: 'character', character_id: characterId },
      viewpoint: { world_id: WORLD, binding_id: bindingId },
    });
    const started = await core.hostExecuteOperation(principal, session.session_id, {
      kind: 'prompt',
      content: 'cancel this run',
    });

    const cancel = await core.hostCancelOperation(principal, started.operation_id);
    assert.equal(cancel.operation_id, started.operation_id);
    assert.equal(cancel.status, 'cancelled');

    const outcome = await waitForOutcome(core, principal, started.operation_id);
    assert.equal(outcome.run_status, 'cancelled');
    assert.equal(outcome.finish_reason, 'cancelled');
    assert.deepEqual(outcome.capture, { status: 'disabled', pending_id: null, code: null });

    // The settled operation refuses a second cancel with its own coded
    // conflict, never a fabricated second success.
    await assert.rejects(
      () => core.hostCancelOperation(principal, started.operation_id),
      (error) => {
        const wire = parseNativeCoreError(error);
        return wire?.code === 'owner_busy' && wire?.details?.conflict_code === 'actor_operation_finished';
      },
    );

    await core.hostShutdownSession(principal, session.session_id);
  });

  test('session shutdown cancels its live work, releases the session, and tombstones its ids', async () => {
    const session = await core.hostCreateSession(principal, {
      provider_id: BLOCK_PROVIDER,
      cwd: creativeRoot,
      actor_ref: { actor_kind: 'character', character_id: characterId },
      viewpoint: { world_id: WORLD, binding_id: bindingId },
    });
    const started = await core.hostExecuteOperation(principal, session.session_id, {
      kind: 'prompt',
      content: 'shutdown this run',
    });

    const shutdown = await core.hostShutdownSession(principal, session.session_id);
    assert.equal(shutdown.session_id, session.session_id);
    assert.equal(shutdown.status, 'shutdown');

    // The shutdown's accepted cancel settles the run it retired.
    const outcome = await waitForOutcome(core, principal, started.operation_id);
    assert.equal(outcome.run_status, 'cancelled');

    // The retired id is a tombstone: even the raw provider lane refuses it, so
    // a released Actor session can never be resurrected as provider-only.
    await assert.rejects(
      () =>
        core.providerCall({
          request_id: randomUUID(),
          method: 'cancel',
          deadline_ms: 30_000,
          session_id: session.session_id,
          payload: {},
        }),
      (error) => isNativeCoreErrorCode(error, 'forbidden'),
    );
  });

  test('the raw provider lane refuses core-indexed Actor ids and keeps provider-only ids working', async () => {
    const reachedBefore = providers.reached.count;

    await assert.rejects(
      () =>
        core.providerCall({
          request_id: randomUUID(),
          method: 'launch',
          deadline_ms: 30_000,
          session_id: mainSessionId,
          payload: { provider_id: MAIN_PROVIDER },
        }),
      (error) => isNativeCoreErrorCode(error, 'forbidden'),
    );
    await assert.rejects(
      () =>
        core.providerCall({
          request_id: randomUUID(),
          method: 'cancel',
          deadline_ms: 30_000,
          operation_id: mainOperationId,
          payload: {},
        }),
      (error) => isNativeCoreErrorCode(error, 'forbidden'),
    );
    await assert.rejects(
      () => core.nextProviderEvents(mainOperationId, 16, 65_536),
      (error) => isNativeCoreErrorCode(error, 'forbidden'),
    );
    assert.equal(
      providers.reached.count,
      reachedBefore,
      'a refused Actor id must not reach the provider lane at all',
    );

    // An id the core never indexed keeps its provider-only semantics end to
    // end: the arbitrary id launches, prompts and terminates on the same peer.
    const rawSessionId = randomUUID();
    const launched = await core.providerCall({
      request_id: randomUUID(),
      method: 'launch',
      deadline_ms: 30_000,
      session_id: rawSessionId,
      payload: { provider_id: MAIN_PROVIDER },
    });
    assert.equal(launched.ok, true, JSON.stringify(launched));
    assert.equal(launched.session_id, rawSessionId);

    const executed = await core.providerCall({
      request_id: randomUUID(),
      method: 'execute',
      deadline_ms: 30_000,
      session_id: rawSessionId,
      payload: {
        Prompt: {
          op_id: randomUUID(),
          content: [{ Text: { text: 'raw lane proof' } }],
          permission_scope: null,
        },
      },
    });
    assert.equal(executed.ok, true, JSON.stringify(executed));
    const rawOperationId = executed.operation_id;
    assert.ok(rawOperationId, JSON.stringify(executed));

    const raw = await drainRawOperation(core, rawOperationId);
    assert.equal(raw.reason, 'end_turn');
    assert.ok(
      raw.texts.some((text) => text.includes('transformed:raw lane proof')),
      `the provider-only lane must still deliver the peer's output: ${JSON.stringify(raw.texts)}`,
    );

    await core.providerCall({
      request_id: randomUUID(),
      method: 'shutdown',
      deadline_ms: 30_000,
      session_id: rawSessionId,
      payload: {},
    });

    // An adapter-shaped id is opaque, not a UUID: the gate must not claim an id
    // the core never indexed, and the lane's own refusal is what answers an
    // unknown operation id.
    const adapterSessionId = 'legacy-adapter-session';
    const legacy = await core.providerCall({
      request_id: randomUUID(),
      method: 'launch',
      deadline_ms: 30_000,
      session_id: adapterSessionId,
      payload: { provider_id: MAIN_PROVIDER },
    });
    assert.equal(legacy.ok, true, JSON.stringify(legacy));
    assert.equal(legacy.session_id, adapterSessionId);
    await core.providerCall({
      request_id: randomUUID(),
      method: 'shutdown',
      deadline_ms: 30_000,
      session_id: adapterSessionId,
      payload: {},
    });
    await assert.rejects(
      () => core.nextProviderEvents('legacy-adapter-operation', 16, 65_536),
      (error) => !isNativeCoreErrorCode(error, 'forbidden'),
    );
  });

  test('close and reopen leave no Actor outcome or session to replay', async () => {
    await core.close();
    providers = await actorProviders();
    core = await openCore({ ...OPEN_OPTIONS, user_home: home }, providers.callbacks);
    const reopenedPrincipal = await core.activePrincipal();

    // Process-lifetime detailed outcomes: the pre-close operation is MISSING
    // after the reopen, never inferred as a success from provider-only state.
    await assert.rejects(
      () => core.hostCharacterOperation(reopenedPrincipal, mainOperationId),
      (error) => isNativeCoreErrorCode(error, 'not_found'),
    );
    await assert.rejects(
      () => core.hostShutdownSession(reopenedPrincipal, mainSessionId),
      (error) => isNativeCoreErrorCode(error, 'not_found'),
    );
  });
});
