import { describe, test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { realpathSync } from 'node:fs';
import { PassThrough } from 'node:stream';
import {
  CleanupUnconfirmedError,
  createTestEngine,
  observeProcessIdentity,
} from '../dist/index.js';

function resolveNode() {
  return realpathSync(execFileSync('which', ['node'], { encoding: 'utf8' }).trim());
}

function makeOwner(child, generation = 'gen-1') {
  const boundIdentity = observeProcessIdentity(child);
  return {
    recipeGeneration: generation,
    child,
    connection: null,
    acpSessionId: null,
    stdinWritable: child.stdin ?? new PassThrough(),
    admittedIdentity: null,
    boundIdentity,
  };
}

describe('cleanup retry fence idempotency', () => {
  test('confirmed retry within same call clears fence and propagates original error', async () => {
    const engine = createTestEngine();
    const child = spawn(resolveNode(), ['-e', ''], { stdio: ['pipe', 'pipe', 'pipe'] });
    await new Promise((resolve) => child.once('exit', resolve));
    const owner = {
      recipeGeneration: 'gen-1',
      child,
      connection: null,
      acpSessionId: null,
      stdinWritable: child.stdin ?? new PassThrough(),
      admittedIdentity: null,
      boundIdentity: { pid: child.pid, process_birth: null, group_id: null },
    };
    const originalCause = new Error('session_failed');
    const error = new CleanupUnconfirmedError(
      'new_session_cleanup_unconfirmed',
      owner,
      originalCause,
    );

    await assert.rejects(
      () => engine.finalizeLaunchFailure(error, owner, 'gen-1:sess-a', 'req-1'),
      (thrown) => thrown === originalCause,
    );
    assert.equal(engine.cleanupFenceCount(), 0);
  });

  test('genuinely unconfirmed cleanup remains fenced and returns interrupted', async () => {
    const prev = process.env.NEXUS_ACP_TEST_REAP_MS;
    process.env.NEXUS_ACP_TEST_REAP_MS = '20';
    const engine = createTestEngine();
    const child = spawn(
      resolveNode(),
      ['-e', 'process.on("SIGTERM",()=>{});setInterval(()=>{},1e6)'],
      { stdio: ['pipe', 'pipe', 'pipe'] },
    );
    const owner = makeOwner(child);
    owner.boundIdentity = {
      ...owner.boundIdentity,
      process_birth: 'stale-birth-token',
    };
    const error = new CleanupUnconfirmedError(
      'new_session_cleanup_unconfirmed',
      owner,
      new Error('session_failed'),
    );

    const reply = await engine.finalizeLaunchFailure(
      error,
      owner,
      'gen-1:sess-b',
      'req-2',
    );
    assert.equal(reply.ok, false);
    assert.equal(reply.error?.code, 'interrupted');
    assert.equal(engine.cleanupFenceCount(), 1);

    try {
      child.kill('SIGKILL');
    } catch {
      // ignore
    }
    process.env.NEXUS_ACP_TEST_REAP_MS = prev;
  });

  test('connect path registers exactly one fence key for same owner identity', async () => {
    const prev = process.env.NEXUS_ACP_TEST_REAP_MS;
    process.env.NEXUS_ACP_TEST_REAP_MS = '20';
    const engine = createTestEngine();
    const child = spawn(
      resolveNode(),
      ['-e', 'process.on("SIGTERM",()=>{});setInterval(()=>{},1e6)'],
      { stdio: ['pipe', 'pipe', 'pipe'] },
    );
    const owner = makeOwner(child, 'gen-connect');
    const error = new CleanupUnconfirmedError('init_cleanup_unconfirmed', owner);

    const reply = await engine.finalizeLaunchFailure(
      error,
      null,
      'gen-connect:sess-c',
      'req-3',
    );
    assert.equal(reply.ok, false);
    assert.equal(engine.cleanupFenceCount(), 1);

    engine.adoptCleanupOwner('duplicate-key', owner);
    assert.equal(engine.cleanupFenceCount(), 1);

    try {
      child.kill('SIGKILL');
    } catch {
      // ignore
    }
    process.env.NEXUS_ACP_TEST_REAP_MS = prev;
  });
});
