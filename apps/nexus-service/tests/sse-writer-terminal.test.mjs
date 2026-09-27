import assert from 'node:assert/strict';
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

  // Actor mirror arm: the retained Actor admission path, at its own cap.
  for (let index = 0; index < cfg.REGISTRY_MAX_ACTOR_OPERATIONS; index += 1) {
    const id = OPERATION_ID('a', index);
    registry.markActorOperation(id, id, 'mock-acp');
    admit(id, newHub(id), arms.actor);
  }
  // Stream-pinned retirements: a live reader is attached, so the retirement the
  // Actor arm applies is deferred to that reader's detach and the record keeps
  // charging the reserve until then. Readers are bounded by the socket budget.
  for (let index = 0; index < cfg.SSE_MAX_TOTAL_SUBSCRIBERS; index += 1) {
    const id = OPERATION_ID('p', index);
    registry.registerSession({ sessionId: id, providerId: 'mock-acp', state: 'Ready', activeOpId: null });
    registry.registerOperation({
      operationId: id,
      sessionId: id,
      providerId: 'mock-acp',
      status: 'started',
      terminalEvent: null,
      terminalTranscript: null,
      actorBacked: true,
    });
    registry.attachOperationStream(id);
    const hub = newHub(id);
    registry.retireActorOperation(id);
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
 * and assert the consequences a consumer sees: every admitted hub keeps its
 * typed ending, the proven population consumes the reserve exactly, an ending
 * beyond it is still counted and logged, and every release returns its bytes.
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

  test('an ending beyond the proven population is counted and logged when the reserve is saturated', async () => {
    const fixture = await saturateCombinedReserve();
    const { cfg, budget, registry } = fixture;
    let extraHub = null;
    try {
      // A hub admitted outside the proven population: the transport admits six
      // live provider-only operations, so a seventh stands for any hub the
      // reserve was not sized for. Its history is already evicted, which is how
      // a stale cursor gets a resync ending.
      const id = OPERATION_ID('x', 1);
      registry.registerSession({ sessionId: id, providerId: 'mock-acp', state: 'Ready', activeOpId: null });
      registry.registerOperation({
        operationId: id,
        sessionId: id,
        providerId: 'mock-acp',
        status: 'started',
        terminalEvent: null,
        terminalTranscript: null,
      });
      extraHub = registry.ensureHub(id, () => new sse.OperationEventHub(id, id));
      for (let index = 0; index < cfg.HUB_MAX_DATA_FRAMES + 1; index += 1) {
        extraHub.recordEvent({ Progress: { message: `line-${index}` } });
      }
      assert.ok(extraHub.evictionWatermark() > 1, 'the extra hub must have evicted history for a stale plan');

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
          new URLSearchParams({ cursor: `${extraHub.epoch}:1` }),
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
      assert.equal(extraHub.hasGap(), false, 'the exhausted reserve cannot retain the ending');
      assert.equal(extraHub.isClosed(), false, 'the stream ends bare — which is why the state is counted and logged');
    } finally {
      extraHub?.dispose();
      releaseAll(fixture);
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
