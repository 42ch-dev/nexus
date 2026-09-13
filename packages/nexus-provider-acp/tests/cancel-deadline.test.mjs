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

    // The owned child must be reaped even though the prompt never settled: that
    // is the whole point of bounding the join.
    assert.ok(
      await waitForDeath(child),
      'the owned child must be terminated despite the never-settling prompt',
    );

    // And the verdict must be honest: either the reap confirmed, or the reply
    // reports interrupted — never a silent success with a live child.
    const interrupted = reply.ok === false && reply.error?.code === 'interrupted';
    assert.ok(
      reply.ok === true || interrupted,
      `reply must be ok or interrupted: ${JSON.stringify(reply)}`,
    );
    if (reply.ok !== true) {
      assert.equal(
        reply.error?.details?.cleanup_unconfirmed,
        true,
        'an unconfirmed reap must be fenced as interrupted',
      );
    }
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
    assert.ok(
      await waitForDeath(child),
      'shutdown must reap the owned child even with a never-settling prompt',
    );
  });
});
