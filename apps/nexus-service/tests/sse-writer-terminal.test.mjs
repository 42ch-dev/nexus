import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { EventEmitter } from 'node:events';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { before, describe, test } from 'node:test';

const __dirname = dirname(fileURLToPath(import.meta.url));
const serviceRoot = join(__dirname, '..');

/**
 * Host-side regression for the shared SSE writer's TERMINAL marker (P1-T3 L2
 * Critical): a retained gap slot recorded AFTER the hub's terminal sorts after
 * it by sequence, so a stale/cursorless replay must END at the terminal frame —
 * never write the gap behind it onto the wire.
 *
 * The consumer observable is the wire itself: the real `OperationEventHub`
 * replay plan is written through the real `SseWriter` against a recording
 * response, and the assertion is on the frames that reached it.
 */
let sse;

before(async () => {
  assert.equal(
    spawnSync('npx', ['tsc', '-p', 'tsconfig.json'], { cwd: serviceRoot, stdio: 'inherit' }).status,
    0,
  );
  sse = await import(join(serviceRoot, 'dist/sse.js'));
});

describe('sse-writer-terminal (shared writer terminal marker)', () => {
  test('a cursorless replay of a terminal hub never delivers a gap frame after the terminal', async () => {
    const hub = new sse.OperationEventHub('op-terminal', 'sess-terminal');
    assert.ok(hub.recordEvent({ OpStarted: { session_id: 'mock-session' } }), 'the data frame is retained');
    assert.ok(
      hub.recordEvent({ OpFinished: { reason: 'end_turn' } }),
      'the terminal frame is retained',
    );
    assert.ok(
      hub.recordGap({
        reason: 'interrupted',
        operation_id: 'op-terminal',
        resync_required: true,
        inspect_url: '/v1/daemon/agent-host/operations/op-terminal',
      }),
      'the late gap slot is retained for the stale replay',
    );

    const plan = hub.planReplay(undefined);
    assert.equal(plan.kind, 'all', 'a cursorless replay replays every retained frame');

    // The real response the writer writes to: every byte it emits is observed.
    const chunks = [];
    const res = new EventEmitter();
    res.write = (chunk) => {
      chunks.push(Buffer.from(chunk));
      return true;
    };
    res.writableEnded = false;
    res.destroyed = false;
    res.end = () => {};
    const writer = new sse.SseWriter(res, hub);

    // The Host replay loop's own stop condition: the terminal frame ends the
    // replay, so nothing retained behind it can reach the wire.
    let endedAtTerminal = false;
    for (const frame of plan.frames) {
      const result = await writer.writeFrame(frame);
      assert.equal(result, 'ok', `every replay frame is writable: ${frame.event}`);
      if (frame.isTerminal) {
        endedAtTerminal = true;
        break;
      }
    }

    assert.equal(endedAtTerminal, true, 'the replay must end at the Host terminal frame');
    const wire = Buffer.concat(chunks).toString('utf8');
    assert.match(wire, /event: provider_event\n/, 'the terminal frame is on the wire');
    assert.doesNotMatch(
      wire,
      /event: gap\n/,
      `a stale replay must not deliver a gap frame after the Host terminal:\n${wire}`,
    );
  });
});

const OPERATION_ID = (prefix, index) =>
  `00000000-0000-4000-8000-${prefix}${String(index).padStart(11, '0')}`;

/** The real resync ending shape, plus test-only padding that fills the slot. */
const gapPayload = (operationId, padding) => ({
  reason: 'interrupted',
  operation_id: operationId,
  resync_required: true,
  inspect_url: `/v1/daemon/agent-host/operations/${operationId}`,
  padding,
});

/**
 * Padding that makes one control frame exactly `SSE_RESERVED_CONTROL_BYTES`
 * long, measured on throwaway hubs (same idiom as the 4 KiB bound case in
 * `security-stream.test.mjs`). Every operation id has the same width, so the
 * measured padding reproduces the bound on every hub.
 */
const measureControlSlotPadding = (OperationEventHub, reservedControlBytes) => {
  const terminalProbe = new OperationEventHub(OPERATION_ID('z', 1), OPERATION_ID('z', 2));
  const terminalBase = terminalProbe.recordEvent({ OpFinished: { reason: 'end_turn', transcript: '' } });
  assert.ok(terminalBase, 'the probe terminal must be retained');
  const padTerminal = reservedControlBytes - terminalBase.wireBytes;
  terminalProbe.dispose();

  const gapProbe = new OperationEventHub(OPERATION_ID('z', 3), OPERATION_ID('z', 4));
  assert.ok(
    gapProbe.recordEvent({ OpFinished: { reason: 'end_turn', transcript: 'x'.repeat(padTerminal) } }),
    'the probe terminal must be retained',
  );
  const gapBase = gapProbe.recordGap(gapPayload(gapProbe.operationId, ''));
  assert.ok(gapBase, 'the probe gap must be retained');
  const padGap = reservedControlBytes - gapBase.wireBytes;
  gapProbe.dispose();

  assert.ok(padTerminal > 0 && padGap > 0, 'both control slots must have room for the padding');
  return { padTerminal, padGap };
};

/**
 * Fill one hub's two control slots to the per-hub ceiling: the retained
 * terminal (`OpFinished`) and the resync gap a stream ends with. Both frames are
 * exactly `SSE_RESERVED_CONTROL_BYTES`, so
 * `retainedMemoryBytes() - chargedBytes()` is exactly the
 * `2 * SSE_RESERVED_CONTROL_BYTES` the reserve bills for one hub.
 */
const fillControlSlots = (hub, padTerminal, padGap) => ({
  terminal: hub.recordEvent({
    OpFinished: { reason: 'end_turn', transcript: 'x'.repeat(padTerminal) },
  }),
  gap: hub.recordGap(gapPayload(hub.operationId, 'x'.repeat(padGap))),
});

const controlBytesOf = ({ hub }) => hub.retainedMemoryBytes() - hub.chargedBytes();

/**
 * Saturate the combined control reserve with the populations it is derived
 * from — retained terminal provider hubs, live provider-only hubs, retained
 * Actor hubs, and the stream-pinned retirements the registry defers — each
 * holding both of its maximum-size control slots. Returns the fixture so a case
 * can assert retention, exhaustion and release behaviour on real hubs.
 */
const saturateCombinedReserve = async () => {
  const cfg = await import(join(serviceRoot, 'dist/config.js'));
  const budget = await import(join(serviceRoot, 'dist/environment-budget.js'));
  const { ProviderRegistry } = await import(join(serviceRoot, 'dist/provider-registry.js'));
  const { OperationEventHub } = sse;
  budget.resetEnvironmentBudgetForTests();
  const { padTerminal, padGap } = measureControlSlotPadding(
    OperationEventHub,
    cfg.SSE_RESERVED_CONTROL_BYTES,
  );

  const registry = new ProviderRegistry();
  const arms = { terminal: [], actor: [], pinned: [], active: [] };
  const newHub = (id) => registry.ensureHub(id, () => new OperationEventHub(id, id));
  const admit = (id, hub, arm) => {
    const { terminal, gap } = fillControlSlots(hub, padTerminal, padGap);
    arm.push({ id, hub, terminal, gap });
  };
  const openProvider = (id, status) => {
    registry.registerSession({ sessionId: id, providerId: 'mock-acp', state: 'Ready', activeOpId: null });
    registry.registerOperation({
      operationId: id,
      sessionId: id,
      providerId: 'mock-acp',
      status,
      terminalEvent: null,
      terminalTranscript: null,
    });
    return newHub(id);
  };

  // Actor mirror arm: the Actor retention path (`markActorOperation`, the one
  // admission every Actor stream performs), at its own bound.
  for (let index = 0; index < cfg.REGISTRY_MAX_ACTOR_OPERATIONS; index += 1) {
    const id = OPERATION_ID('a', index);
    registry.markActorOperation(id, id, 'mock-acp');
    assert.equal(
      registry.operationRecord(id)?.actorBacked,
      true,
      'the Actor arm must retain every row it admits up to its bound',
    );
    admit(id, newHub(id), arms.actor);
  }
  // Stream-pinned retirements: a live reader is reading when the row's release
  // is requested, so `disposeOperation` defers it to that reader's detach and the
  // row keeps charging the reserve until then. This arm is the terminal one
  // because its bound is a FIFO: a deferred row leaves the queue without being
  // released, which is exactly how an arm holds its bound plus one pinned row per
  // live reader. Readers are bounded by the socket budget.
  for (let index = 0; index < cfg.SSE_MAX_TOTAL_SUBSCRIBERS; index += 1) {
    const id = OPERATION_ID('p', index);
    registry.registerSession({ sessionId: id, providerId: 'mock-acp', state: 'Running', activeOpId: null });
    registry.attachOperationStream(id);
    registry.registerOperation({
      operationId: id,
      sessionId: id,
      providerId: 'mock-acp',
      status: 'finished',
      terminalEvent: null,
      terminalTranscript: null,
    });
    const hub = newHub(id);
    // A session shutdown retires the row while its reader is still mid-stream.
    registry.removeSession(id);
    assert.equal(
      registry.operationRecord(id)?.operationId,
      id,
      'a retirement pinned by a live reader must be deferred, not applied',
    );
    admit(id, hub, arms.pinned);
  }
  // Retained terminal provider arm.
  for (let index = 0; index < cfg.REGISTRY_MAX_TERMINAL_OPERATIONS; index += 1) {
    const id = OPERATION_ID('t', index);
    admit(id, openProvider(id, 'finished'), arms.terminal);
  }
  // Live provider-only arm.
  for (let index = 0; index < cfg.MAX_ACTIVE_PROVIDER_OPERATIONS; index += 1) {
    const id = OPERATION_ID('n', index);
    admit(id, openProvider(id, 'started'), arms.active);
  }

  return { cfg, budget, registry, arms };
};

/** Release every hub through the path that owns it, then clear the ledger. */
const releaseAll = ({ budget, registry, arms }) => {
  for (const { id } of arms.pinned) registry.detachOperationStream(id);
  for (const { id } of arms.actor) registry.retireActorOperation(id);
  for (const { id } of [...arms.terminal, ...arms.active]) registry.removeSession(id);
  for (const arm of Object.values(arms)) {
    for (const { hub } of arm) hub.dispose();
  }
  budget.resetEnvironmentBudgetForTests();
};

/**
 * The control reserve is a *combined* ceiling: the provider-only arms and the
 * Actor mirror arm charge the same pool, and a retirement the registry could not
 * apply because a live stream was still reading it holds its hub's slots until
 * that reader detaches. These cases saturate every population at the per-hub
 * maximum (one terminal + one gap slot of exactly `SSE_RESERVED_CONTROL_BYTES`)
 * through the registry's own admission and assert the consequences a consumer
 * sees: every admitted hub keeps its typed ending, the proven population consumes
 * the reserve exactly, a row beyond it is refused with no effect, an ending the
 * reserve cannot charge is still counted and logged, and every release returns
 * its bytes.
 */
describe('combined control reserve (terminal + active + Actor + stream-pinned hubs)', () => {
  test("a saturated combined population keeps every admitted hub's typed terminal and gap", async () => {
    const fixture = await saturateCombinedReserve();
    const { cfg, budget, registry, arms } = fixture;
    try {
      const hubs = [...arms.terminal, ...arms.actor, ...arms.pinned, ...arms.active];
      // Each arm at its own bound: the four populations the reserve is sized for.
      const expectedHubs =
        cfg.REGISTRY_MAX_TERMINAL_OPERATIONS +
        cfg.MAX_ACTIVE_PROVIDER_OPERATIONS +
        cfg.REGISTRY_MAX_ACTOR_OPERATIONS +
        cfg.SSE_MAX_TOTAL_SUBSCRIBERS;
      assert.equal(arms.terminal.length, cfg.REGISTRY_MAX_TERMINAL_OPERATIONS);
      assert.equal(arms.actor.length, cfg.REGISTRY_MAX_ACTOR_OPERATIONS);
      assert.equal(arms.pinned.length, cfg.SSE_MAX_TOTAL_SUBSCRIBERS);
      assert.equal(registry.activeOperationCount(), cfg.MAX_ACTIVE_PROVIDER_OPERATIONS);
      assert.equal(hubs.length, expectedHubs);
      assert.equal(
        expectedHubs,
        cfg.ENVIRONMENT_MAX_TRACKED_HUBS,
        'the control reserve must cover exactly these populations',
      );

      // Every admitted hub keeps both typed endings, at the maximum slot size.
      for (const entry of hubs) {
        const { id, hub, terminal, gap } = entry;
        assert.equal(terminal?.wireBytes, cfg.SSE_RESERVED_CONTROL_BYTES, `${id} must retain its typed terminal`);
        assert.equal(terminal.isTerminal, true);
        assert.equal(terminal.isControl, true);
        assert.equal(hub.hasTerminal(), true);
        assert.equal(gap?.wireBytes, cfg.SSE_RESERVED_CONTROL_BYTES, `${id} must retain its typed resync gap`);
        assert.equal(gap.event, 'gap');
        assert.equal(gap.isControl, true);
        assert.equal(hub.hasGap(), true);
        assert.equal(controlBytesOf(entry), 2 * cfg.SSE_RESERVED_CONTROL_BYTES);
      }

      // The proven population consumes the reserve exactly — no byte is left.
      const charged = hubs.reduce((total, entry) => total + controlBytesOf(entry), 0);
      assert.equal(charged, cfg.SSE_CONTROL_RESERVED_TOTAL_BYTES);
      assert.equal(
        budget.tryReserveControlBytes(1),
        false,
        'a saturated proven population must leave no control byte',
      );

      const proof = budget.environmentBudgetProof();
      assert.equal(charged, proof.nodeControlBytes, 'the environment proof must bill exactly this reserve');
      assert.equal(proof.ceilingBytes, 32 * 1024 * 1024, 'the frozen environment ceiling must not be raised');
      assert.ok(
        proof.totalBytes <= proof.ceilingBytes,
        `the combined proof must stay inside the ceiling: ${proof.totalBytes} > ${proof.ceilingBytes}`,
      );

      // Wire behaviour at the ceiling: the hub that closed the reserve still
      // delivers its typed terminal, and the replay ends there.
      const last = hubs[hubs.length - 1];
      const chunks = [];
      const res = new EventEmitter();
      res.write = (chunk) => {
        chunks.push(Buffer.from(chunk));
        return true;
      };
      res.writableEnded = false;
      res.destroyed = false;
      res.end = () => {};
      const writer = new sse.SseWriter(res, last.hub);
      for (const frame of last.hub.planReplay(undefined).frames) {
        assert.equal(await writer.writeFrame(frame), 'ok', `every retained frame is writable: ${frame.event}`);
        if (frame.isTerminal) break;
      }
      const wire = Buffer.concat(chunks).toString('utf8');
      assert.match(wire, /event: provider_event\n/, 'the typed terminal must reach the wire at the ceiling');
    } finally {
      releaseAll(fixture);
    }
  });

  test('a live row beyond the proven population is refused with no effect', async () => {
    const fixture = await saturateCombinedReserve();
    const { cfg, budget, registry } = fixture;
    try {
      assert.equal(
        budget.tryReserveControlBytes(1),
        false,
        'the proven population must consume the reserve exactly',
      );
      // The registry is the admission authority for every live row, so a row it
      // was not sized for is refused whole: no record, no session state, no hub —
      // and no control byte returned, because nothing was released or retained.
      const id = OPERATION_ID('x', 1);
      registry.registerSession({ sessionId: id, providerId: 'mock-acp', state: 'Ready', activeOpId: null });
      const retained = registry.registerOperation({
        operationId: id,
        sessionId: id,
        providerId: 'mock-acp',
        status: 'started',
        terminalEvent: null,
        terminalTranscript: null,
      });
      assert.equal(registry.operationRecord(id), undefined, 'a refused row must leave no record');
      assert.equal(registry.hubForOperation(id), undefined, 'a refused row must leave no hub');
      assert.equal(registry.sessionRecord(id)?.activeOpId, null, 'a refused row must not mark its session busy');
      assert.equal(registry.sessionRecord(id)?.state, 'Ready', 'a refused row must not mark its session running');
      assert.equal(
        registry.activeOperationCount(),
        cfg.MAX_ACTIVE_PROVIDER_OPERATIONS,
        'a refusal must not change the live population',
      );
      assert.equal(
        budget.tryReserveControlBytes(1),
        false,
        'a refusal must return no control byte — the proven population keeps every one of them',
      );
      assert.equal(retained, false, 'a live row beyond the cap must be refused, not retained');
    } finally {
      releaseAll(fixture);
    }
  });

  test('an ending the reserve cannot hold is counted and logged, never silently dropped', async () => {
    const cfg = await import(join(serviceRoot, 'dist/config.js'));
    const budget = await import(join(serviceRoot, 'dist/environment-budget.js'));
    const { ProviderRegistry } = await import(join(serviceRoot, 'dist/provider-registry.js'));
    budget.resetEnvironmentBudgetForTests();
    const registry = new ProviderRegistry();
    const id = OPERATION_ID('x', 1);
    let hub = null;
    try {
      registry.registerSession({ sessionId: id, providerId: 'mock-acp', state: 'Running', activeOpId: id });
      registry.registerOperation({
        operationId: id,
        sessionId: id,
        providerId: 'mock-acp',
        status: 'started',
        terminalEvent: null,
        terminalTranscript: null,
      });
      assert.equal(
        registry.operationRecord(id)?.status,
        'started',
        'the live row must be admitted while the population has room',
      );
      hub = registry.ensureHub(id, () => new sse.OperationEventHub(id, id));
      for (let index = 0; index < cfg.HUB_MAX_DATA_FRAMES + 1; index += 1) {
        hub.recordEvent({ Progress: { message: `line-${index}` } });
      }
      assert.ok(hub.evictionWatermark() > 1, 'the hub must have evicted history for a stale plan');

      // Byte-exact exhaustion: the remaining reserve stands for the rest of the
      // proven population, so the ending below has nothing left to charge. That
      // is the one state the admission bounds keep the registry from growing
      // into — and it must stay diagnosable if it is ever reached.
      while (budget.tryReserveControlBytes(1024)) {}
      while (budget.tryReserveControlBytes(1)) {}
      assert.equal(budget.tryReserveControlBytes(1), false, 'the reserve must be exhausted');

      const res = new EventEmitter();
      res.writeHead = () => {};
      res.write = () => true;
      res.end = () => {};
      res.writableEnded = false;
      res.destroyed = false;

      const before = sse.sseUnretainedGapEvents.count;
      const logged = [];
      const originalError = console.error;
      console.error = (...args) => {
        logged.push(args.join(' '));
      };
      try {
        await sse.streamSessionEvents(
          { providerRegistry: registry },
          id,
          new URLSearchParams({ cursor: `${hub.epoch}:1` }),
          res,
        );
      } finally {
        console.error = originalError;
      }

      assert.equal(
        sse.sseUnretainedGapEvents.count,
        before + 1,
        'an ending the saturated reserve cannot charge must be counted',
      );
      assert.match(
        logged.join('\n'),
        /ended without a typed resync gap at stale-plan-gap: control-frame reserve exhausted/,
      );
      assert.equal(hub.hasGap(), false, 'the exhausted reserve cannot retain the ending');
      assert.equal(hub.isClosed(), false, 'the stream ends bare — which is why the state is counted and logged');
    } finally {
      hub?.dispose();
      budget.resetEnvironmentBudgetForTests();
    }
  });

  test('every release returns the control bytes it charged', async () => {
    const fixture = await saturateCombinedReserve();
    const { cfg, budget, registry, arms } = fixture;
    try {
      assert.equal(budget.tryReserveControlBytes(1), false, 'the fixture must start saturated');

      // The stream-pinned arm: the deferred retirement lands at the reader's
      // detach, and exactly that arm's slots come back — no more, no less.
      for (const { id } of arms.pinned) registry.detachOperationStream(id);
      for (const { id } of arms.pinned) {
        assert.equal(
          registry.operationRecord(id),
          undefined,
          'the retirement pinned by the reader must land at its last detach',
        );
      }
      const pinnedBytes = arms.pinned.length * 2 * cfg.SSE_RESERVED_CONTROL_BYTES;
      assert.equal(budget.tryReserveControlBytes(pinnedBytes), true, 'the pinned arm must return all of its slots');
      assert.equal(budget.tryReserveControlBytes(1), false, 'and exactly its slots');
      budget.releaseControlBytes(pinnedBytes);

      // The capped arms retire through their own paths.
      for (const { id } of arms.actor) registry.retireActorOperation(id);
      for (const { id } of arms.terminal) registry.removeSession(id);
      for (const { id } of arms.active) registry.removeSession(id);
      assert.equal(
        budget.tryReserveControlBytes(cfg.SSE_CONTROL_RESERVED_TOTAL_BYTES),
        true,
        'every release must return the control bytes it charged',
      );
      budget.releaseControlBytes(cfg.SSE_CONTROL_RESERVED_TOTAL_BYTES);
      for (const arm of Object.values(arms)) {
        for (const { hub } of arm) {
          assert.equal(hub.retainedMemoryBytes(), 0, 'a released hub must hold no control slot');
        }
      }
    } finally {
      releaseAll(fixture);
    }
  });
});

/**
 * A hub exists for a row the registry retains, and the bounds in the reserve
 * derivation are enforced on the *production* creation routes, not just counted:
 * the Actor execute registration, the live provider-only dispatch, and the cold
 * hydration of a natively-live operation. Each case drives the real function
 * (with the native authority stubbed at the `ServiceCore` seam) rather than
 * registering rows on the fixture's behalf, so the proof covers the path that
 * creates the hub.
 */
describe('provider lane admission (execute registration)', () => {
  /** A response the SSE writer can write to, recording every byte it emits. */
  const recordingResponse = () => {
    const chunks = [];
    const res = new EventEmitter();
    res.socket = null;
    res.writeHead = () => {};
    res.write = (chunk) => {
      chunks.push(Buffer.from(chunk));
      return true;
    };
    res.writableEnded = false;
    res.destroyed = false;
    res.end = function end() {
      this.writableEnded = true;
    };
    return { res, wire: () => Buffer.concat(chunks).toString('utf8') };
  };

  test('>64 Actor operations registered by execute stay inside the Actor arm with their endings', async () => {
    const cfg = await import(join(serviceRoot, 'dist/config.js'));
    const budget = await import(join(serviceRoot, 'dist/environment-budget.js'));
    const { ProviderRegistry } = await import(join(serviceRoot, 'dist/provider-registry.js'));
    const { executeProviderOperation } = await import(join(serviceRoot, 'dist/provider.js'));
    budget.resetEnvironmentBudgetForTests();
    const registry = new ProviderRegistry();
    const actorSessionId = randomUUID();
    const characterId = `chr_${'a'.repeat(32)}`;
    const operationIds = [];
    const service = {
      providerRegistry: registry,
      domainOnly: false,
      core: {
        activePrincipal: async () => ({ holder: 'actor-admission-test' }),
        hostQuery: async (request) =>
          request.query === 'get_session'
            ? {
                session: {
                  session_id: request.session_id,
                  provider_id: 'mock-acp',
                  state: 'Running',
                  active_op_id: null,
                  actor_ref: { actor_kind: 'character', character_id: characterId },
                },
              }
            : {},
        hostExecuteOperation: async () => {
          const operationId = OPERATION_ID('e', operationIds.length);
          operationIds.push(operationId);
          return { operation_id: operationId, session_id: actorSessionId, status: 'started' };
        },
        // The authority still serves every run: the age sweep is not what bounds
        // the arm here — the retention bound itself is under test.
        hostCharacterOperation: async (principal, operationId) => ({
          operation_id: operationId,
          session_id: actorSessionId,
          run_status: 'running',
        }),
      },
    };
    const { padTerminal, padGap } = measureControlSlotPadding(
      sse.OperationEventHub,
      cfg.SSE_RESERVED_CONTROL_BYTES,
    );
    const total = cfg.REGISTRY_MAX_ACTOR_OPERATIONS + 6;
    try {
      for (let index = 0; index < total; index += 1) {
        await executeProviderOperation(service, actorSessionId, {
          kind: 'prompt',
          content: `actor-${index}`,
        });
      }
      assert.equal(operationIds.length, total, 'every execute must have been admitted by the authority');
      const retained = registry.actorBackedOperations();
      assert.equal(
        retained.length,
        cfg.REGISTRY_MAX_ACTOR_OPERATIONS,
        `the Actor arm must hold exactly its bound: ${retained.length}`,
      );
      assert.equal(
        registry.operationRecord(operationIds[0]),
        undefined,
        'the oldest Actor row must be retired to admit the later ones',
      );
      assert.notEqual(registry.operationRecord(operationIds[total - 1]), undefined, 'the newest row is retained');

      // No hub outside the proof: a hub exists exactly for the rows retained.
      for (const operationId of operationIds) {
        assert.equal(
          registry.hubForOperation(operationId) !== undefined,
          registry.operationRecord(operationId) !== undefined,
          `hub and record must be paired for ${operationId}`,
        );
      }

      // Every retained row still keeps both of its typed endings, and the whole
      // arm's measured charge stays inside the reserve the proof derives.
      let charged = 0;
      for (const record of retained) {
        const hub = registry.hubForOperation(record.operationId);
        assert.ok(hub, 'a retained Actor row must have its hub');
        const { terminal, gap } = fillControlSlots(hub, padTerminal, padGap);
        assert.equal(terminal?.wireBytes, cfg.SSE_RESERVED_CONTROL_BYTES, `${record.operationId} keeps its typed terminal`);
        assert.equal(gap?.wireBytes, cfg.SSE_RESERVED_CONTROL_BYTES, `${record.operationId} keeps its typed resync gap`);
        charged += controlBytesOf({ hub });
      }
      assert.equal(
        charged,
        retained.length * 2 * cfg.SSE_RESERVED_CONTROL_BYTES,
        'each retained Actor row must hold exactly its two control slots',
      );
      assert.equal(
        budget.tryReserveControlBytes(cfg.SSE_CONTROL_RESERVED_TOTAL_BYTES - charged),
        true,
        `the retained Actor arm must leave the rest of the reserve free: ${charged}`,
      );

      // A retirement request under a live reader is deferred to that reader's
      // detach, never applied under the stream it is reading.
      const pinned = retained[0].operationId;
      registry.attachOperationStream(pinned);
      registry.retireActorOperation(pinned);
      assert.notEqual(registry.operationRecord(pinned), undefined, 'a retirement must never land under a live reader');
      registry.detachOperationStream(pinned);
      assert.equal(registry.operationRecord(pinned), undefined, 'the deferred retirement lands at the last detach');

      // Settled-row preservation: a later mark is admission, not observation, so
      // it never rewinds the row it already holds.
      const settledId = retained[1].operationId;
      const terminalEvent = { OpFinished: { reason: 'end_turn' } };
      assert.equal(registry.finishOperation(settledId, terminalEvent, 'the captured transcript'), true);
      assert.equal(registry.markActorOperation(settledId, actorSessionId, 'mock-acp'), true);
      const settled = registry.operationRecord(settledId);
      assert.equal(settled.status, 'finished', 'a re-mark must not rewind a settled row');
      assert.deepEqual(settled.terminalEvent, terminalEvent, 'a re-mark must not drop the settled terminal');
    } finally {
      for (const record of registry.actorBackedOperations()) registry.retireActorOperation(record.operationId);
      budget.resetEnvironmentBudgetForTests();
    }
  });

  test('concurrent provider dispatches stop at the live cap, which counts work in flight', async () => {
    const cfg = await import(join(serviceRoot, 'dist/config.js'));
    const budget = await import(join(serviceRoot, 'dist/environment-budget.js'));
    const { ProviderRegistry } = await import(join(serviceRoot, 'dist/provider-registry.js'));
    const { executeProviderOperation } = await import(join(serviceRoot, 'dist/provider.js'));
    budget.resetEnvironmentBudgetForTests();
    const registry = new ProviderRegistry();
    const sessionIds = Array.from({ length: cfg.MAX_ACTIVE_PROVIDER_OPERATIONS + 1 }, () => randomUUID());
    const dispatched = [];
    let releaseDispatch = () => {};
    const gate = new Promise((resolve) => {
      releaseDispatch = resolve;
    });
    const service = {
      providerRegistry: registry,
      domainOnly: false,
      core: {
        hostQuery: async (request) =>
          request.query === 'get_session'
            ? {
                session: {
                  session_id: request.session_id,
                  provider_id: 'mock-acp',
                  state: 'Ready',
                  active_op_id: null,
                },
              }
            : {},
        // Every dispatched execute stays in flight: none of them has registered
        // a row yet when the next request arrives — the exact race the cap missed
        // when it only read the registered rows.
        providerCall: (request) => {
          dispatched.push(request.session_id);
          return gate.then(() => ({ ok: true, operation_id: request.request_id }));
        },
      },
    };
    const { padTerminal, padGap } = measureControlSlotPadding(
      sse.OperationEventHub,
      cfg.SSE_RESERVED_CONTROL_BYTES,
    );
    try {
      const admissions = sessionIds.map((sessionId) =>
        executeProviderOperation(service, sessionId, { kind: 'prompt', content: 'concurrent' }).then(
          () => 'admitted',
          (error) => error,
        ),
      );
      // Every request has to resolve its native session read first: a couple of
      // macrotasks let all of them reach admission while none has registered a
      // row yet — the exact race a cap that reads only registered rows misses.
      await new Promise((resolve) => setTimeout(resolve, 20));
      assert.equal(
        dispatched.length,
        cfg.MAX_ACTIVE_PROVIDER_OPERATIONS,
        `only the cap may reach the provider: ${dispatched.length} dispatched`,
      );
      assert.equal(registry.activeOperationCount(), 0, 'no dispatch has registered a row yet');

      releaseDispatch();
      const outcomes = await Promise.all(admissions);
      assert.deepEqual(
        outcomes.slice(0, cfg.MAX_ACTIVE_PROVIDER_OPERATIONS),
        Array.from({ length: cfg.MAX_ACTIVE_PROVIDER_OPERATIONS }, () => 'admitted'),
        'every reserved dispatch must register its row',
      );
      const rejected = outcomes[cfg.MAX_ACTIVE_PROVIDER_OPERATIONS];
      assert.equal(rejected.status, 503, `the request past the cap must be refused: ${JSON.stringify(rejected)}`);
      assert.equal(rejected.code, 'busy');
      assert.equal(
        registry.activeOperationCount(),
        cfg.MAX_ACTIVE_PROVIDER_OPERATIONS,
        'the admitted dispatches are the live population',
      );

      // The admitted population keeps its typed endings, and the arm at the cap
      // still refuses the next dispatch *before* it reaches the provider.
      const liveRows = registry.actorBackedOperations().length;
      assert.equal(liveRows, 0, 'no admitted provider-only row may be marked Actor-backed');
      let charged = 0;
      for (const sessionId of sessionIds.slice(0, cfg.MAX_ACTIVE_PROVIDER_OPERATIONS)) {
        const record = registry.sessionRecord(sessionId);
        assert.ok(record.activeOpId, 'an admitted dispatch must mark its session busy');
        const hub = registry.hubForOperation(record.activeOpId);
        assert.ok(hub, 'an admitted dispatch must have its hub');
        const { terminal, gap } = fillControlSlots(hub, padTerminal, padGap);
        assert.equal(terminal?.wireBytes, cfg.SSE_RESERVED_CONTROL_BYTES, 'an admitted hub keeps its typed terminal');
        assert.equal(gap?.wireBytes, cfg.SSE_RESERVED_CONTROL_BYTES, 'an admitted hub keeps its typed resync gap');
        charged += 2 * cfg.SSE_RESERVED_CONTROL_BYTES;
      }
      assert.ok(
        charged <= cfg.SSE_CONTROL_RESERVED_TOTAL_BYTES,
        `the live arm must stay inside the reserve: ${charged}`,
      );

      const extra = randomUUID();
      const refused = await executeProviderOperation(service, extra, {
        kind: 'prompt',
        content: 'over-limit',
      }).then(
        () => null,
        (error) => error,
      );
      assert.equal(refused?.status, 503, 'a dispatch at the cap must be refused before any effect');
      assert.equal(refused?.code, 'busy');
      assert.equal(
        dispatched.length,
        cfg.MAX_ACTIVE_PROVIDER_OPERATIONS,
        'a refused dispatch must never reach the provider',
      );
    } finally {
      for (const sessionId of sessionIds) registry.removeSession(sessionId);
      budget.resetEnvironmentBudgetForTests();
    }
  });

  test('cold hydration of a live operation beyond the cap is refused with no effect', async () => {
    const cfg = await import(join(serviceRoot, 'dist/config.js'));
    const budget = await import(join(serviceRoot, 'dist/environment-budget.js'));
    const { ProviderRegistry } = await import(join(serviceRoot, 'dist/provider-registry.js'));
    budget.resetEnvironmentBudgetForTests();
    const registry = new ProviderRegistry();
    const liveSessions = [];
    const coldSessionId = randomUUID();
    const coldOperationId = OPERATION_ID('h', cfg.MAX_ACTIVE_PROVIDER_OPERATIONS);
    try {
      // The mirror holds the live population the transport admitted.
      for (let index = 0; index < cfg.MAX_ACTIVE_PROVIDER_OPERATIONS; index += 1) {
        const sessionId = randomUUID();
        const operationId = OPERATION_ID('h', index);
        liveSessions.push(sessionId);
        registry.registerSession({ sessionId, providerId: 'mock-acp', state: 'Running', activeOpId: operationId });
        registry.registerOperation({
          operationId,
          sessionId,
          providerId: 'mock-acp',
          status: 'started',
          terminalEvent: null,
          terminalTranscript: null,
        });
        assert.equal(
          registry.operationRecord(operationId)?.status,
          'started',
          'a live row inside the cap must be admitted',
        );
      }
      const service = {
        providerRegistry: registry,
        core: {
          // Native truth for the cold session and its running operation.
          hostQuery: async (request) => {
            if (request.query === 'get_operation') {
              return {
                operation: {
                  operation_id: request.operation_id,
                  session_id: coldSessionId,
                  status: 'running',
                },
              };
            }
            if (request.query === 'get_session') {
              return {
                session: {
                  session_id: request.session_id,
                  provider_id: 'mock-acp',
                  state: 'Running',
                  active_op_id: coldOperationId,
                },
              };
            }
            return {};
          },
          nextProviderEvents: async () => ({
            events: [{ OpFinished: { session_id: coldSessionId, op_id: coldOperationId, reason: 'end_turn' } }],
            gap: null,
            has_more: false,
          }),
        },
      };
      const params = new URLSearchParams({ operation_id: coldOperationId });

      const refused = recordingResponse();
      await assert.rejects(
        () => sse.streamSessionEvents(service, coldSessionId, params, refused.res),
        (error) => error.status === 503 && error.code === 'busy',
        'the cold stream must be refused with busy, never served by a hub outside the proof',
      );
      assert.equal(refused.wire(), '', 'a refused stream must write nothing');
      assert.equal(registry.operationRecord(coldOperationId), undefined, 'a refused hydration leaves no record');
      assert.equal(registry.hubForOperation(coldOperationId), undefined, 'a refused hydration leaves no hub');
      assert.equal(
        registry.activeOperationCount(),
        cfg.MAX_ACTIVE_PROVIDER_OPERATIONS,
        'a refused hydration must not change the live population',
      );
      assert.equal(
        budget.tryReserveControlBytes(cfg.SSE_CONTROL_RESERVED_TOTAL_BYTES),
        true,
        'a refused hydration must charge no control byte',
      );
      budget.releaseControlBytes(cfg.SSE_CONTROL_RESERVED_TOTAL_BYTES);

      // The hydration path asks the one registration authority to admit the row,
      // so the refusal is also observable there — with nothing retained.
      assert.equal(
        registry.registerOperation({
          operationId: coldOperationId,
          sessionId: coldSessionId,
          providerId: 'mock-acp',
          status: 'running',
          terminalEvent: null,
          terminalTranscript: null,
        }),
        false,
        'the registration authority must refuse the cold live row',
      );

      // Positive control: with one slot released, the same cold operation is
      // admitted and ends its stream with the typed terminal the authority
      // delivered — the refusal above was capacity, not a broken lane.
      registry.removeSession(liveSessions[0]);
      const priorDelay = process.env.NEXUS_SSE_FIRST_PULL_DELAY_MS;
      process.env.NEXUS_SSE_FIRST_PULL_DELAY_MS = '1';
      const admitted = recordingResponse();
      try {
        await sse.streamSessionEvents(service, coldSessionId, params, admitted.res);
      } finally {
        if (priorDelay === undefined) delete process.env.NEXUS_SSE_FIRST_PULL_DELAY_MS;
        else process.env.NEXUS_SSE_FIRST_PULL_DELAY_MS = priorDelay;
      }
      assert.notEqual(registry.operationRecord(coldOperationId), undefined, 'the admitted row is retained');
      assert.match(admitted.wire(), /event: provider_event/, 'the admitted cold stream must deliver its typed terminal');
      assert.match(admitted.wire(), /OpFinished/, 'the terminal the authority delivered must reach the wire');
    } finally {
      for (const sessionId of liveSessions) registry.removeSession(sessionId);
      registry.removeSession(coldSessionId);
      budget.resetEnvironmentBudgetForTests();
    }
  });
});
