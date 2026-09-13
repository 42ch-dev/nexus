import { describe, test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { realpathSync } from 'node:fs';
import { PassThrough } from 'node:stream';
import { createTestEngine, observeProcessIdentity, OperationDelivery } from '../dist/index.js';

function resolveNode() {
  return realpathSync(execFileSync('which', ['node'], { encoding: 'utf8' }).trim());
}

function makeOwner(child, generation = 'gen-cancel') {
  const boundIdentity = observeProcessIdentity(child);
  return {
    recipeGeneration: generation,
    child,
    connection: null,
    acpSessionId: 'acp-session-1',
    stdinWritable: child.stdin ?? new PassThrough(),
    admittedIdentity: null,
    boundIdentity,
  };
}

function childAlive(child) {
  if (child.exitCode !== null || child.signalCode !== null) return false;
  try {
    process.kill(child.pid, 0);
    return true;
  } catch {
    return false;
  }
}

async function waitForDeath(child, budgetMs = 3000) {
  const deadline = Date.now() + budgetMs;
  while (Date.now() < deadline) {
    if (!childAlive(child)) return true;
    await new Promise((r) => setTimeout(r, 25));
  }
  return !childAlive(child);
}

describe('cancel honours the request deadline', () => {
  test('a never-settling prompt does not block the owned-child reap', async () => {
    // A long-lived owned child, as a real ACP session would have.
    const child = spawn(resolveNode(), ['-e', 'setInterval(() => {}, 1000)'], {
      stdio: ['pipe', 'pipe', 'pipe'],
    });
    await new Promise((resolve) => setTimeout(resolve, 150));
    assert.ok(child.pid, 'owned child must be running');

    const engine = createTestEngine();
    const owner = makeOwner(child);
    engine.adoptSession('sess-1', owner);
    // A prompt that NEVER settles, and a cancel notification that returns
    // immediately without settling it — the shape that used to hang the adapter
    // forever because the prompt join was unbounded.
    owner.connection = { cancel: async () => undefined };
    engine.adoptOperation('sess-1', 'op-1', new Promise(() => {}), new OperationDelivery('op-1'));

    const started = Date.now();
    const reply = await engine.call({
      request_id: 'cancel-1',
      method: 'cancel',
      session_id: 'sess-1',
      operation_id: 'op-1',
      deadline_ms: 300,
      payload: {},
    });
    const elapsed = Date.now() - started;

    // The deadline bounds the cancel + prompt join; the call must come back
    // promptly instead of awaiting a prompt that never settles.
    assert.ok(
      elapsed < 2000,
      `cancel must respect deadline_ms, took ${elapsed}ms`,
    );

    // A never-settling prompt consumes the whole deadline, so nothing is left to
    // confirm a reap. The verdict must be an honest `interrupted` with the owner
    // RETAINED — never a silent success, and never a kill it cannot verify.
    assert.equal(reply.ok, false, `must not claim success: ${JSON.stringify(reply)}`);
    assert.equal(reply.error?.code, 'interrupted');
    assert.equal(
      reply.error?.details?.cleanup_unconfirmed,
      true,
      'an unconfirmed reap must be fenced as interrupted',
    );
    assert.equal(engine.cleanupFenceCount(), 1, 'the owner must be retained for retry');

    // The retained owner settles on a later attempt, and only then is the child
    // confirmed gone.
    const settled = await engine.trySettleCleanupFences();
    assert.equal(settled.pending.length, 0, `retry must confirm: ${JSON.stringify(settled)}`);
    assert.equal(engine.cleanupFenceCount(), 0);
    assert.ok(
      await waitForDeath(child),
      'the retained owner must be reaped by the later settlement',
    );
  });

  test('shutdown honours the deadline for a busy session', async () => {
    const child = spawn(resolveNode(), ['-e', 'setInterval(() => {}, 1000)'], {
      stdio: ['pipe', 'pipe', 'pipe'],
    });
    await new Promise((resolve) => setTimeout(resolve, 150));

    const engine = createTestEngine();
    const owner = makeOwner(child, 'gen-shutdown');
    engine.adoptSession('sess-2', owner, 'gen-shutdown');
    owner.connection = { cancel: async () => undefined };
    engine.adoptOperation('sess-2', 'op-2', new Promise(() => {}), new OperationDelivery('op-2'));

    const started = Date.now();
    await engine.call({
      request_id: 'shutdown-1',
      method: 'shutdown',
      session_id: 'sess-2',
      deadline_ms: 300,
      payload: {},
    });
    const elapsed = Date.now() - started;

    assert.ok(
      elapsed < 2000,
      `shutdown must respect deadline_ms, took ${elapsed}ms`,
    );
    // Same contract as cancel: no budget left means fence, retain, retry.
    assert.equal(engine.cleanupFenceCount(), 1, 'the owner must be retained for retry');
    const settled = await engine.trySettleCleanupFences();
    assert.equal(settled.pending.length, 0, `retry must confirm: ${JSON.stringify(settled)}`);
    assert.ok(
      await waitForDeath(child),
      'shutdown must reap the owned child once a settlement attempt has budget',
    );
  });
});

describe('one absolute deadline per handler', () => {
  /**
   * A child that IGNORES SIGTERM. The handler can only confirm the reap by
   * escalating to SIGKILL, which costs far more than the supplied deadline — so
   * a handler that grants itself a fresh default reap budget is instantly
   * visible here, both in wall time and in whether it claims a confirmed reap.
   */
  function spawnTermResistantChild() {
    return spawn(
      resolveNode(),
      ['-e', "process.on('SIGTERM', () => {}); setInterval(() => {}, 1000);"],
      { stdio: ['pipe', 'pipe', 'pipe'] },
    );
  }

  /** Total handler time is the deadline plus scheduling/timer tolerance. */
  const DEADLINE_TOLERANCE_MS = 600;

  test('cancel spends only the supplied deadline, then fences a live owner', async () => {
    const child = spawnTermResistantChild();
    await new Promise((resolve) => setTimeout(resolve, 150));

    const engine = createTestEngine();
    const owner = makeOwner(child, 'gen-term');
    engine.adoptSession('sess-term', owner, 'gen-term');
    owner.connection = { cancel: async () => undefined };
    engine.adoptOperation(
      'sess-term',
      'op-term',
      new Promise(() => {}),
      new OperationDelivery('op-term'),
    );

    const deadlineMs = 400;
    const started = Date.now();
    const reply = await engine.call({
      request_id: 'term-cancel',
      method: 'cancel',
      session_id: 'sess-term',
      operation_id: 'op-term',
      deadline_ms: deadlineMs,
      payload: {},
    });
    const elapsed = Date.now() - started;

    // The whole handler is bounded by the ONE supplied deadline — the cancel
    // phase plus the reap, not each with its own budget.
    assert.ok(
      elapsed <= deadlineMs + DEADLINE_TOLERANCE_MS,
      `handler must finish within its deadline (${deadlineMs}ms), took ${elapsed}ms`,
    );

    // With the budget spent it must NOT claim a confirmed reap.
    assert.equal(reply.ok, false, `must not claim success: ${JSON.stringify(reply)}`);
    assert.equal(reply.error?.code, 'interrupted');
    assert.equal(reply.error?.details?.cleanup_unconfirmed, true);

    // The owner is RETAINED for a later settlement, and the child is untouched —
    // a kill we cannot confirm is not issued.
    assert.equal(
      engine.cleanupFenceCount(),
      1,
      'the unconfirmed owner must be fenced for retry',
    );
    assert.equal(childAlive(child), true, 'no unconfirmable kill may be issued');

    // A later settlement retries and, with a budget it can actually use,
    // confirms the reap (SIGKILL escalation) and clears the fence.
    const settled = await engine.trySettleCleanupFences();
    assert.equal(settled.pending.length, 0, `settlement must confirm: ${JSON.stringify(settled)}`);
    assert.equal(settled.settled.length, 1, 'the retained owner must settle on retry');
    assert.equal(engine.cleanupFenceCount(), 0, 'a confirmed retry clears the fence');
    assert.equal(await waitForDeath(child), true, 'the retry must actually reap the child');
  });

  test('shutdown spends only the supplied deadline, then fences a live owner', async () => {
    const child = spawnTermResistantChild();
    await new Promise((resolve) => setTimeout(resolve, 150));

    const engine = createTestEngine();
    const owner = makeOwner(child, 'gen-term-sd');
    engine.adoptSession('sess-term-sd', owner, 'gen-term-sd');
    owner.connection = { cancel: async () => undefined };
    engine.adoptOperation(
      'sess-term-sd',
      'op-term-sd',
      new Promise(() => {}),
      new OperationDelivery('op-term-sd'),
    );

    const deadlineMs = 400;
    const started = Date.now();
    const reply = await engine.call({
      request_id: 'term-shutdown',
      method: 'shutdown',
      session_id: 'sess-term-sd',
      deadline_ms: deadlineMs,
      payload: {},
    });
    const elapsed = Date.now() - started;

    assert.ok(
      elapsed <= deadlineMs + DEADLINE_TOLERANCE_MS,
      `handler must finish within its deadline (${deadlineMs}ms), took ${elapsed}ms`,
    );
    assert.equal(reply.ok, false, `must not claim success: ${JSON.stringify(reply)}`);
    assert.equal(reply.error?.details?.cleanup_unconfirmed, true);
    assert.equal(engine.cleanupFenceCount(), 1, 'the owner must be retained');
    assert.equal(childAlive(child), true, 'no unconfirmable kill may be issued');

    const settled = await engine.trySettleCleanupFences();
    assert.equal(settled.pending.length, 0, `settlement must confirm: ${JSON.stringify(settled)}`);
    assert.equal(engine.cleanupFenceCount(), 0);
    assert.equal(await waitForDeath(child), true);
  });

  test('a generous deadline still confirms the reap in one call', async () => {
    // Contrast case: with enough budget the same child is reaped inside the
    // handler, so the deadline bounds the work rather than always fencing.
    const child = spawnTermResistantChild();
    await new Promise((resolve) => setTimeout(resolve, 150));

    const engine = createTestEngine();
    const owner = makeOwner(child, 'gen-generous');
    engine.adoptSession('sess-generous', owner, 'gen-generous');
    engine.adoptOperation(
      'sess-generous',
      'op-generous',
      Promise.resolve(),
      new OperationDelivery('op-generous'),
    );

    const reply = await engine.call({
      request_id: 'generous-shutdown',
      method: 'shutdown',
      session_id: 'sess-generous',
      deadline_ms: 12_000,
      payload: {},
    });
    assert.equal(reply.ok, true, `expected a confirmed reap: ${JSON.stringify(reply)}`);
    assert.equal(engine.cleanupFenceCount(), 0);
    assert.equal(await waitForDeath(child), true);
  });
});
