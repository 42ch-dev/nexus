import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { EventEmitter } from 'node:events';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { before, describe, test } from 'node:test';

const __dirname = dirname(fileURLToPath(import.meta.url));
const serviceRoot = join(__dirname, '..');

/**
 * The consumer observable is the wire itself: the real `OperationEventHub`
 * replay plan is written through the real `SseWriter` against a recording
 * response, and the assertion is on the frames that reached it.
 */
let sse;

before(async () => {
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

const fillControlSlots = (hub, padTerminal, padGap) => ({
  terminal: hub.recordEvent({
    OpFinished: { reason: 'end_turn', transcript: 'x'.repeat(padTerminal) },
  }),
  gap: hub.recordGap(gapPayload(hub.operationId, 'x'.repeat(padGap))),
});

const controlBytesOf = ({ hub }) => hub.retainedMemoryBytes() - hub.chargedBytes();

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
    registry.removeSession(id);
    assert.equal(
      registry.operationRecord(id)?.operationId,
      id,
      'a retirement pinned by a live reader must be deferred, not applied',
    );
    admit(id, hub, arms.pinned);
  }
  for (let index = 0; index < cfg.REGISTRY_MAX_TERMINAL_OPERATIONS; index += 1) {
    const id = OPERATION_ID('t', index);
    admit(id, openProvider(id, 'finished'), arms.terminal);
  }
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

describe('combined control reserve (terminal + active + Actor + stream-pinned hubs)', () => {
  test("a saturated combined population keeps every admitted hub's typed terminal and gap", async () => {
    const fixture = await saturateCombinedReserve();
    const { cfg, budget, registry, arms } = fixture;
    try {
      const hubs = [...arms.terminal, ...arms.actor, ...arms.pinned, ...arms.active];
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

      for (const operationId of operationIds) {
        assert.equal(
          registry.hubForOperation(operationId) !== undefined,
          registry.operationRecord(operationId) !== undefined,
          `hub and record must be paired for ${operationId}`,
        );
      }

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

      const pinned = retained[0].operationId;
      registry.attachOperationStream(pinned);
      registry.retireActorOperation(pinned);
      assert.notEqual(registry.operationRecord(pinned), undefined, 'a retirement must never land under a live reader');
      registry.detachOperationStream(pinned);
      assert.equal(registry.operationRecord(pinned), undefined, 'the deferred retirement lands at the last detach');

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
