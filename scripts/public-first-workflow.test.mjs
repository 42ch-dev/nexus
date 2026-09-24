#!/usr/bin/env node
/**
 * Focused contract tests for the P3 public first-workflow driver (P3-T2).
 *
 * Scope: the driver's own observable contracts, exercised against loopback
 * servers and temporary directories it creates itself. Nothing here needs the
 * prepared service/native artifacts, a real `dsh`, an upstream model or the
 * product database — the composed journey belongs to the driver run, not to
 * this file.
 *
 *   node --test scripts/public-first-workflow.test.mjs
 *
 * Every case fails on a plausible regression of the contract it names:
 *   * the bounded read must end on ITS OWN window (a returning socket-idle
 *     timer reported an ordinary idle reconnect as `ECONNRESET`/
 *     `socket hang up`), and a real mid-stream reset must stay a failure
 *     instead of being read as a short stream;
 *   * a cursor reconnect must replay exactly the successors the cursor
 *     promised, exclusively (off-by-one → duplicate handoff), and neither an
 *     empty idle window nor an uncovered gap may be accepted as a replay;
 *   * a lost retained history must be the explicit `history_unavailable`
 *     control frame and nothing else;
 *   * the sealed request structure must be checked (a request that advertises
 *     tools refuses) and the endpoint must not retain request content;
 *   * an unsolicited tool side effect — a hostile marker or any scope entry the
 *     fixture never declared — must be detected rather than ignored.
 */
import { strict as assert } from 'node:assert';
import { createServer } from 'node:http';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';

import {
  applyCleanupDisposition,
  assertDeterministicReceipt,
  assertLiveCredentialChannel,
  assertGuardAttempt,
  assertHistoryLossExplicit,
  assertNoHostileMarkers,
  assertScopeEffectOnly,
  assertSealedToolPolicy,
  boundedServerClose,
  buildChildEnv,
  classifySameRunReplay,
  parseGapFrame,
  readAttemptDir,
  readEventStream,
  readEventStreamOrStop,
  readSpentToken,
  startModelEndpoint,
  summarizeChildEnv,
} from './public-first-workflow.mjs';

const RUN_ID = 'first-workflow:test-run';
const EPOCH = '11111111-2222-3333-4444-555555555555';
const READ_WINDOW_MS = 300;

/** One SSE `run_state` frame carrying the ring's own `<epoch>:<sequence>` cursor. */
function stateFrame(sequence, epoch = EPOCH) {
  return `id: ${epoch}:${sequence}\nevent: run_state\ndata: ${JSON.stringify({ state_revision: sequence })}\n\n`;
}

/** Frame identity list `1..count` of the test epoch. */
function observedIds(count, epoch = EPOCH) {
  return Array.from({ length: count }, (_, index) => `${epoch}:${index + 1}`);
}

/**
 * One owned loopback SSE server. `mode` selects the transport behaviour, not a
 * data fixture: `no-response` accepts the connection and answers nothing at all
 * (the live run's own shape — the workflow transport writes its headers with the
 * first frame, so a run with nothing new to send has not flushed any yet),
 * `idle` answers headers and then says nothing, `replay` answers the given frame
 * text and then idles, `reset` aborts the connection after writing its frames,
 * `close` ends the stream, `json-404` is a pre-header refusal.
 */
function startSseServer(mode, frames = '') {
  const server = createServer((request, response) => {
    if (mode === 'no-response') return;
    if (mode === 'json-404') {
      response.writeHead(404, { 'content-type': 'application/json' });
      response.end(JSON.stringify({ error: { code: 'not_found', message: 'no such run' } }));
      return;
    }
    response.writeHead(200, { 'content-type': 'text/event-stream; charset=utf-8' });
    response.flushHeaders();
    if (mode === 'idle') return;
    response.write(frames);
    if (mode === 'reset') {
      setTimeout(() => response.socket?.destroy(), 10);
      return;
    }
    if (mode === 'close') response.end();
  });
  return new Promise((resolvePromise) => {
    server.listen(0, '127.0.0.1', () => {
      resolvePromise({
        port: server.address().port,
        close: () => new Promise((done) => server.close(done)),
      });
    });
  });
}

/** Run `body` against one owned server and always release it. */
async function withSseServer(mode, frames, body) {
  const server = await startSseServer(mode, frames);
  try {
    return await body(server.port);
  } finally {
    await server.close();
  }
}

/** Assert a driver refusal by its stable outcome/category. */
function assertDriverFailure(error, outcome, category) {
  assert.equal(error?.name, 'DriverFailure', `expected DriverFailure, got ${error?.name}: ${error?.message}`);
  assert.equal(error.outcome, outcome);
  assert.equal(error.category, category);
}

/**
 * The classified results of `classifySameRunReplay` for one replayed frame set.
 * The default read shape is the CLOSED one (`closed`, not the driver's window):
 * the §4 outcome a gap-carrying reconnect must have.
 */
function classifyReplay(replayFrames, { cursor = observedIds(5)[0], observed = observedIds(5), read = {} } = {}) {
  return classifySameRunReplay({
    runId: RUN_ID,
    cursor,
    observedIds: observed,
    read: { status: 200, json: null, frames: replayFrames, closed: true, timed_out: false, ...read },
  });
}

test('a silent same-run stream ends as the driver window, never as a connection failure', async () => {
  const silent = await withSseServer('no-response', '', (port) =>
    readEventStream(port, RUN_ID, { maxFrames: 16, timeoutMs: READ_WINDOW_MS }),
  );
  assert.equal(silent.timed_out, true);
  assert.equal(silent.closed, false);
  assert.deepEqual(silent.frames, []);

  const idle = await withSseServer('idle', '', (port) =>
    readEventStream(port, RUN_ID, { maxFrames: 16, timeoutMs: READ_WINDOW_MS }),
  );
  assert.equal(idle.status, 200);
  assert.equal(idle.timed_out, true);
  assert.equal(idle.closed, false);
  assert.deepEqual(idle.frames, []);
});

test('a connection reset mid-stream stays a transport failure, not a short read', async () => {
  const reset = await withSseServer('reset', stateFrame(1) + stateFrame(2), (port) =>
    readEventStreamOrStop('test read', () => readEventStream(port, RUN_ID, { maxFrames: 16, timeoutMs: READ_WINDOW_MS })),
  ).catch((error) => error);
  assertDriverFailure(reset, 'failed', 'event_stream_transport');
});

test('a cursor reconnect replays exactly the promised successors, in order, and refuses a repeat or reorder', async () => {
  const observed = observedIds(5);
  const cursor = observed[0];
  const replay = await withSseServer('replay', stateFrame(2) + stateFrame(3) + stateFrame(4) + stateFrame(5), (port) =>
    readEventStream(port, RUN_ID, { lastEventId: cursor, maxFrames: 16, timeoutMs: READ_WINDOW_MS }),
  );
  const facts = classifySameRunReplay({ runId: RUN_ID, cursor, observedIds: observed, read: replay });
  assert.equal(facts.kind, 'successors');
  assert.deepEqual(facts.replayed_ids, observed.slice(1));
  assert.deepEqual(facts.missing_successors, []);
  assert.deepEqual(facts.gaps, []);
  assert.equal(facts.exclusive, true);
  assert.equal(facts.duplicate_handoff, false);

  // A repeated successor and a reordered pair are both duplicated/replayed
  // events: every promised ID is present, so set membership alone would call
  // them a faithful replay.
  const repeated = [
    { id: `${EPOCH}:2`, event: 'run_state', data: '{}' },
    { id: `${EPOCH}:2`, event: 'run_state', data: '{}' },
    { id: `${EPOCH}:3`, event: 'run_state', data: '{}' },
  ];
  assert.throws(
    () => classifyReplay(repeated, { cursor: observedIds(3)[0], observed: observedIds(3) }),
    (error) => {
      assertDriverFailure(error, 'failed', 'replay_out_of_order');
      return true;
    },
  );
  const reordered = [
    { id: `${EPOCH}:3`, event: 'run_state', data: '{}' },
    { id: `${EPOCH}:2`, event: 'run_state', data: '{}' },
    { id: `${EPOCH}:2`, event: 'run_state', data: '{}' },
  ];
  assert.throws(
    () => classifyReplay(reordered, { cursor: observedIds(3)[0], observed: observedIds(3) }),
    (error) => {
      assertDriverFailure(error, 'failed', 'replay_out_of_order');
      return true;
    },
  );
});

test('an explicit post-cursor gap cannot be certified as complete successor replay', () => {
  const cursor = `${EPOCH}:1`;
  const observed = observedIds(2);
  const facts = classifyReplay([
    { id: `${EPOCH}:4`, event: 'gap', data: JSON.stringify({
      run_id: RUN_ID, epoch: EPOCH, from_sequence: 3, to_sequence: 4,
    }) },
    { id: `${EPOCH}:2`, event: 'run_state', data: '{}' },
  ], { cursor, observed, read: { closed: false, timed_out: true } });
  assert.equal(facts.kind, 'gap');
  assert.deepEqual(facts.missing_successors, []);
  assert.deepEqual(facts.gaps, [{ from_sequence: 3, to_sequence: 4 }]);
});

test('deterministic children drop mixed-case credential names before reading their values', () => {
  const key = 'nexus_Fixture_aPi_KeY';
  process.env[key] = 'synthetic-test-only';
  try {
    const inherited = Object.keys(process.env);
    const child = buildChildEnv({ home: '/tmp/isolated-home', dshHome: '/tmp/isolated-dsh', modelPort: 12345, guardEnv: {} });
    assert.equal(Object.hasOwn(child, key), false);
    const summary = summarizeChildEnv(inherited, child);
    assert.equal(summary.inherited_credential_keys_forwarded.includes(key), false);
    assert.ok(summary.removed_credential_key_count > 0);
  } finally {
    delete process.env[key];
  }
});

test('unconfirmed owned cleanup overrides an otherwise successful journey receipt', () => {
  const receipt = { outcome: 'ok', blocker: null };
  applyCleanupDisposition(receipt, [{ confirmed: false, category: 'cleanup_unconfirmed', detail: 'owned service still alive' }]);
  assert.equal(receipt.outcome, 'failed');
  assert.equal(receipt.blocker.category, 'cleanup_unconfirmed');
});

test('a throwing owned-server close records an unconfirmed cleanup without rejecting', async () => {
  const result = await boundedServerClose({ close() { throw new Error('close failed'); } }, 50);
  assert.equal(result.confirmed, false);
  assert.match(result.detail, /close failed/);
});

test('an idle reconnect and a single-frame read are never accepted as a replay proof', async () => {
  const idle = await withSseServer('idle', '', (port) =>
    readEventStream(port, RUN_ID, { lastEventId: observedIds(5)[0], maxFrames: 16, timeoutMs: READ_WINDOW_MS }),
  );
  assert.throws(
    () => classifySameRunReplay({ runId: RUN_ID, cursor: observedIds(5)[0], observedIds: observedIds(5), read: idle }),
    (error) => {
      assertDriverFailure(error, 'failed', 'replay_incomplete');
      return true;
    },
  );
  assert.throws(
    () => classifyReplay([], { cursor: observedIds(1)[0], observed: observedIds(1) }),
    (error) => {
      assertDriverFailure(error, 'failed', 'replay_unprovable');
      return true;
    },
  );
});

test('a covering gap is accepted only as the closed eviction outcome, and unsafe or uncovered gaps are refused', () => {
  const gapData = (from, to) => JSON.stringify({ run_id: RUN_ID, epoch: EPOCH, from_sequence: from, to_sequence: to });
  const gapFrame = { id: `${EPOCH}:5`, event: 'gap', data: gapData(2, 5) };

  // §4 eviction close: the gap is the whole answer and the server closed on it.
  const covered = classifyReplay([gapFrame]);
  assert.equal(covered.kind, 'gap');
  assert.deepEqual(covered.gaps, [{ from_sequence: 2, to_sequence: 5 }]);
  assert.deepEqual(covered.missing_successors, observedIds(5).slice(1));
  assert.equal(covered.closed, true);
  assert.equal(covered.timed_out, false);

  // The same gap on a stream that then idles until the driver's window expires
  // is NOT the contract's outcome — it must not be reported as an honest gap.
  assert.throws(
    () => classifyReplay([gapFrame], { read: { closed: false, timed_out: true } }),
    (error) => {
      assertDriverFailure(error, 'failed', 'replay_gap_not_closed');
      return true;
    },
  );

  // A retention gap that arrives WITH the retained successors is the live replay
  // case: bounded record, gap plus the replayed tail, no fabricated content.
  const trimmed = classifyReplay([
    { id: `${EPOCH}:3`, event: 'gap', data: gapData(2, 3) },
    { id: `${EPOCH}:4`, event: 'run_state', data: '{}' },
    { id: `${EPOCH}:5`, event: 'run_state', data: '{}' },
  ], { read: { closed: false, timed_out: true } });
  assert.equal(trimmed.kind, 'gap');
  assert.deepEqual(trimmed.gaps, [{ from_sequence: 2, to_sequence: 3 }]);
  assert.deepEqual(trimmed.missing_successors, [`${EPOCH}:2`, `${EPOCH}:3`]);
  assert.deepEqual(trimmed.replayed_ids, [`${EPOCH}:4`, `${EPOCH}:5`]);

  // A gap may not contradict the very frames the same replay delivered: it
  // states they are gone, so its range and the delivered frames must be
  // disjoint. Adjacent (2..3 + :4/:5, above) is the legal trim shape; the two
  // shapes below deliver a frame the gap claims was lost.
  assert.throws(
    () =>
      classifyReplay([
        { id: `${EPOCH}:5`, event: 'gap', data: gapData(2, 5) },
        { id: `${EPOCH}:4`, event: 'run_state', data: '{}' },
        { id: `${EPOCH}:5`, event: 'run_state', data: '{}' },
      ]),
    (error) => {
      assertDriverFailure(error, 'failed', 'replay_gap_overlaps_delivered');
      return true;
    },
  );
  assert.throws(
    () =>
      classifyReplay([
        { id: `${EPOCH}:3`, event: 'gap', data: gapData(2, 3) },
        { id: `${EPOCH}:3`, event: 'run_state', data: '{}' },
        { id: `${EPOCH}:4`, event: 'run_state', data: '{}' },
        { id: `${EPOCH}:5`, event: 'run_state', data: '{}' },
      ]),
    (error) => {
      assertDriverFailure(error, 'failed', 'replay_gap_overlaps_delivered');
      return true;
    },
  );

  // Unsafe / out-of-vocabulary bounds are not a bounded gap at all.
  assert.equal(parseGapFrame({ event: 'gap', data: gapData(0, 1e100) }), null);
  assert.equal(parseGapFrame({ event: 'gap', data: gapData(2, 1e100) }), null);
  assert.equal(parseGapFrame({ event: 'gap', data: gapData(2.5, 5) }), null);
  assert.equal(parseGapFrame({ event: 'gap', data: gapData(5, 2) }), null);
  assert.throws(
    () => classifyReplay([{ ...gapFrame, data: gapData(0, 1e100) }]),
    (error) => {
      assertDriverFailure(error, 'failed', 'replay_contract_violation');
      return true;
    },
  );

  // A gap that does not reach the successors the cursor promised leaves them
  // uncovered.
  assert.throws(
    () => classifyReplay([{ ...gapFrame, data: gapData(2, 3) }]),
    (error) => {
      assertDriverFailure(error, 'failed', 'replay_incomplete');
      return true;
    },
  );
});

test('a lost retained history is the explicit control frame, and any other answer is refused', () => {
  const lossFrame = {
    id: null,
    event: 'history_unavailable',
    data: JSON.stringify({ run_id: RUN_ID, inspect_url: `/v1/daemon/orchestration/sessions/${RUN_ID}` }),
  };
  const explicit = assertHistoryLossExplicit({
    runId: RUN_ID,
    cursor: observedIds(5).at(-1),
    read: { frames: [lossFrame], closed: true, timed_out: false },
  });
  assert.equal(explicit.control_frame, 'history_unavailable');
  assert.equal(explicit.data_frames, 0);
  assert.throws(
    () =>
      assertHistoryLossExplicit({
        runId: RUN_ID,
        cursor: observedIds(5).at(-1),
        read: {
          frames: observedIds(3).map((id) => ({ id, event: 'run_state', data: '{"state_revision":1}' })),
          closed: true,
          timed_out: false,
        },
      }),
    (error) => {
      assertDriverFailure(error, 'failed', 'history_loss_not_explicit');
      return true;
    },
  );
  assert.throws(
    () =>
      assertHistoryLossExplicit({
        runId: RUN_ID,
        cursor: observedIds(5).at(-1),
        read: {
          frames: [{ ...lossFrame, data: JSON.stringify({ run_id: 'other-run', inspect_url: '/x' }) }],
          closed: true,
          timed_out: false,
        },
      }),
    (error) => {
      assertDriverFailure(error, 'failed', 'history_loss_not_explicit');
      return true;
    },
  );
});

test('the sealed request structure is checked, and an advertised-tools request is refused', async () => {
  const prompt = 'sealed prompt body that must never be retained';
  const sealed = await startModelEndpoint();
  const toolsAdvertised = await startModelEndpoint();
  try {
    const post = async (endpoint, body) => {
      const response = await fetch(`http://127.0.0.1:${endpoint.port}/chat/completions`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(body),
      });
      await response.text();
    };
    await post(sealed, { model: 'loopback-test', messages: [{ role: 'system' }, { role: 'user', content: prompt }] });
    await post(toolsAdvertised, {
      model: 'loopback-test',
      messages: [{ role: 'system' }, { role: 'user', content: prompt }],
      tools: [{ type: 'function', function: { name: 'shell' } }],
    });

    const policy = assertSealedToolPolicy(sealed.observations);
    assert.equal(policy.advertised_tools, false);
    assert.deepEqual(policy.first_request_roles, ['system', 'user']);
    assert.throws(
      () => assertSealedToolPolicy(toolsAdvertised.observations),
      (error) => {
        assertDriverFailure(error, 'failed', 'sealed_policy_violation');
        return true;
      },
    );
    assert.ok(
      !JSON.stringify(sealed.observations).includes(prompt),
      'the endpoint must record structure only, never the request body',
    );
  } finally {
    assert.equal((await sealed.close()).confirmed, true);
    assert.equal((await toolsAdvertised.close()).confirmed, true);
  }
});

test('an unsolicited tool side effect is detected in the isolated root and the opened scope', () => {
  const root = mkdtempSync(join(tmpdir(), 'public-first-workflow-test-'));
  const scope = join(root, 'workspace', 'notes');
  const fixture = { changePath: 'first-workflow.md' };
  try {
    mkdirSync(scope, { recursive: true });
    writeFileSync(join(scope, fixture.changePath), 'committed');
    assert.deepEqual(assertNoHostileMarkers(root).present, []);
    const inventory = assertScopeEffectOnly(scope, fixture);
    assert.deepEqual(
      inventory.map((entry) => entry.path),
      [fixture.changePath],
    );

    mkdirSync(join(root, 'markers'), { recursive: true });
    writeFileSync(join(root, 'markers', 'editor-marker'), 'side effect');
    assert.throws(
      () => assertNoHostileMarkers(root),
      (error) => {
        assertDriverFailure(error, 'failed', 'unsolicited_side_effect');
        return true;
      },
    );
    rmSync(join(root, 'markers'), { recursive: true, force: true });

    writeFileSync(join(scope, 'pwned.txt'), 'side effect');
    assert.throws(
      () => assertScopeEffectOnly(scope, fixture),
      (error) => {
        assertDriverFailure(error, 'failed', 'unsolicited_side_effect');
        return true;
      },
    );
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

/**
 * One attempt directory populated in the guard's own record shape, so these
 * cases exercise the driver's consumer-side contract rather than a fixture.
 * `mutate` receives the attempt directory after the clean attempt is written.
 */
function withAttemptDir(mutate) {
  const dir = mkdtempSync(join(tmpdir(), 'pfw-guard-evidence-'));
  const events = join(dir, 'events');
  mkdirSync(events, { recursive: true, mode: 0o700 });
  const event = (kind, runtime, category, extra = {}) =>
    writeFileSync(
      join(events, `${kind}-${process.pid}-${Math.random().toString(16).slice(2, 10)}-abcdef12.json`),
      JSON.stringify({ schema: 'nexus-request-guard-event/1', kind, runtime, category, at: '2026-09-24T01:00:00.000Z', ...extra }),
      { mode: 0o600 },
    );
  event('loaded', 'other', 'guard_loaded', { problems: [] });
  event('loaded', 'dsh', 'guard_loaded', { problems: [] });
  event('admitted', 'dsh', 'model_request_admitted');
  writeFileSync(
    join(dir, 'spent'),
    JSON.stringify({ schema: 'nexus-request-guard-spent/1', pid: process.pid, at: '2026-09-24T01:00:00.000Z', category: 'model_request_admitted' }),
    { mode: 0o600 },
  );
  try {
    mutate(dir, events);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

test('a clean attempt is accepted and carries an explicit evidence-integrity condition', () => {
  withAttemptDir((dir) => {
    const facts = assertGuardAttempt(readAttemptDir(dir, { final: true }), readSpentToken(dir));
    assert.equal(facts.admitted, 1);
    assert.equal(facts.denied, 0);
    assert.equal(facts.loaded_dsh, 1);
    assert.equal(facts.evidence_integrity, 'complete');
    assert.equal(facts.evidence_failed_marker, null);
    assert.deepEqual(facts.problems, []);
  });
});

test("the guard's evidence-failed marker disqualifies an otherwise clean attempt", () => {
  withAttemptDir((dir) => {
    // The guard's own marker shape (`nexus-request-guard-evidence-failed/1`): a
    // denied record could not be persisted, so the attempt's counts cannot be
    // believed even though they look perfect.
    writeFileSync(
      join(dir, 'evidence-failed'),
      JSON.stringify({
        schema: 'nexus-request-guard-evidence-failed/1',
        reason: 'event_write_failed',
        kind: 'denied',
        pid: process.pid,
        at: '2026-09-24T01:00:05.000Z',
      }),
    );
    assert.throws(
      () => assertGuardAttempt(readAttemptDir(dir, { final: true }), readSpentToken(dir)),
      (error) => {
        assertDriverFailure(error, 'failed', 'guard_evidence_failed');
        assert.match(error.message, /denied/);
        assert.match(error.message, /event_write_failed/);
        return true;
      },
    );
    // A foreign or unreadable payload still reports as present — presence is the
    // signal, never the decodability of the marker.
    writeFileSync(join(dir, 'evidence-failed'), 'not json');
    assert.throws(
      () => assertGuardAttempt(readAttemptDir(dir, { final: true }), readSpentToken(dir)),
      (error) => {
        assertDriverFailure(error, 'failed', 'guard_evidence_failed');
        return true;
      },
    );
  });
});

test('a degraded preload (non-empty problems, e.g. a replaceable fetch) disqualifies the attempt', () => {
  withAttemptDir((dir, events) => {
    writeFileSync(
      join(events, `loaded-${process.pid}-9-abcdef13.json`),
      JSON.stringify({
        schema: 'nexus-request-guard-event/1',
        kind: 'loaded',
        runtime: 'dsh',
        category: 'guard_loaded',
        at: '2026-09-24T01:00:00.000Z',
        problems: ['unsupported_transport'],
      }),
      { mode: 0o600 },
    );
    assert.throws(
      () => assertGuardAttempt(readAttemptDir(dir, { final: true }), readSpentToken(dir)),
      (error) => {
        assertDriverFailure(error, 'failed', 'guard_evidence_degraded');
        assert.match(error.message, /unsupported_transport/);
        return true;
      },
    );
  });
});

test('a truncated record is skipped while the attempt runs and disqualifies it at rest', () => {
  withAttemptDir((dir, events) => {
    writeFileSync(join(events, `denied-${process.pid}-8-abcdef14.json`), '');
    // Mid-run: the only in-progress state a single exclusive write can leave.
    assert.equal(readAttemptDir(dir).admitted, 1);
    assert.throws(
      () => readAttemptDir(dir, { final: true }),
      (error) => {
        assertDriverFailure(error, 'failed', 'guard_event_truncated');
        return true;
      },
    );
  });
});

test('the live receipt gate refuses a guard proof that is degraded, incomplete or missing integrity', () => {
  const recorded = { path: '/tmp/artifact', present: true, bytes: 1, mtime_ms: 1, sha256: 'aa' };
  const artifacts = {
    driver: { ...recorded },
    guard: { ...recorded },
    fixture: { ...recorded },
    service_entry: { ...recorded },
    cli: { ...recorded },
    dsh: { ...recorded },
    dsh_transport: { ...recorded },
    runtime: { node: '22.0.0', platform: 'darwin', arch: 'arm64' },
  };
  const clean = { admitted: 1, denied: 0, loaded_dsh: 1, event_files: 3, spent: 'model_request_admitted', problems: [], evidence_integrity: 'complete', evidence_failed_marker: null };
  const current = { ...artifacts, runtime: artifacts.runtime };
  const dir = mkdtempSync(join(tmpdir(), 'pfw-receipt-gate-'));
  const write = (guard) => {
    const path = join(dir, `receipt-${Math.random().toString(16).slice(2, 8)}.json`);
    writeFileSync(path, JSON.stringify({ schema: 'public-first-workflow-receipt/1', mode: 'deterministic', outcome: 'ok', facts: { artifacts, guard } }));
    return path;
  };
  const refuse = (guard) =>
    assert.throws(
      () => assertDeterministicReceipt(write(guard), { current }),
      (error) => {
        assertDriverFailure(error, 'blocked', 'receipt_mismatch');
        return true;
      },
    );
  try {
    assert.equal(assertDeterministicReceipt(write(clean), { current }).guard.admitted, 1);
    refuse({ ...clean, problems: ['dsh:unsupported_transport'] });
    refuse({ ...clean, evidence_failed_marker: { present: true, category: 'evidence_write_failed' } });
    refuse({ ...clean, evidence_integrity: 'partial' });
    refuse({ admitted: 1, denied: 0, loaded_dsh: 1, spent: 'model_request_admitted' });
    refuse({ ...clean, denied: 1 });
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('an explicitly selected live attempt requires an inherited channel name without reading a credential', () => {
  assert.throws(
    () => assertLiveCredentialChannel({ key: null, observation: 'absent', values_read: false }),
    (error) => {
      assertDriverFailure(error, 'blocked', 'credentials_unavailable');
      return true;
    },
  );
  const named = { key: 'DEEPSEEK_API_KEY', observation: 'present_unverifiable', values_read: false };
  assert.equal(assertLiveCredentialChannel(named), named);
});
