import { describe, test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, realpathSync, rmSync } from 'node:fs';
import { PassThrough } from 'node:stream';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  CleanupUnconfirmedError,
  createAcpProvider,
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

  test('public retries recover only unpublished owners within one exclusive deadline', { timeout: 20_000 }, async (t) => {
    const workspace = mkdtempSync(join(tmpdir(), 'acp-public-recovery-'));
    const logPath = join(workspace, 'peer.jsonl');
    const records = () => existsSync(logPath)
      ? readFileSync(logPath, 'utf8').trim().split('\n').filter(Boolean).map((line) => JSON.parse(line))
      : [];
    const starts = () => records().filter((record) => record.event === 'start');
    const alive = (pid) => {
      try { process.kill(pid, 0); return true; } catch { return false; }
    };
    const waitUntil = async (predicate) => {
      const deadline = Date.now() + 3000;
      while (!predicate()) {
        assert.ok(Date.now() < deadline, 'bounded fixture observation');
        await new Promise((resolve) => setTimeout(resolve, 10));
      }
    };
    t.after(async () => {
      // Only PIDs recorded by this test's freshly spawned protocol peers.
      for (const { pid } of starts()) if (alive(pid)) process.kill(pid, 'SIGKILL');
      await waitUntil(() => starts().every(({ pid }) => !alive(pid)));
      rmSync(workspace, { recursive: true, force: true });
    });
    const peer = String.raw`
const fs = require('node:fs');
const readline = require('node:readline');
const record = (event) => fs.appendFileSync(process.env.LOG, JSON.stringify({event, pid: process.pid}) + '\n');
process.on('SIGTERM', () => record('term'));
setInterval(() => {}, 1000); // EOF alone must not pretend the owned child died.
record('start');
readline.createInterface({input: process.stdin}).on('line', (line) => {
  const request = JSON.parse(line);
  let result;
  if (request.method === 'initialize') {
    result = {protocolVersion: 1, agentCapabilities: {}};
  } else if (request.method === 'session/new') {
    if (!fs.existsSync(process.env.MARKER)) {
      fs.writeFileSync(process.env.MARKER, 'failed');
      process.stdout.write(JSON.stringify({jsonrpc: '2.0', id: request.id, error: {code: -32603, message: 'session_failed'}}) + '\n');
      return;
    }
    result = {sessionId: 'real-session'};
  } else if (request.method === 'session/cancel') {
    return;
  } else {
    throw new Error('unexpected method ' + request.method);
  }
  process.stdout.write(JSON.stringify({jsonrpc: '2.0', id: request.id, result}) + '\n');
});
`;
    const provider = createAcpProvider();
    const request = (id, deadlineMs = 3000, method = 'launch') => ({
      request_id: id,
      method,
      deadline_ms: deadlineMs,
      payload: { provider_id: id, recipe: {
        provider_id: id, recipe_generation: `gen-${id}`,
        executable: resolveNode(), args: ['-e', peer], cwd: workspace,
        env: { LOG: logPath, MARKER: join(workspace, `${id}.marker`) },
      } },
    });

    // Both public launches enter before either fence exists. Their failed
    // session/new occurs after initialize's retained 10ms settle, exhausting
    // the 1ms deadline before cleanup; neither child may be signalled blindly.
    const failed = await Promise.all([
      provider.call(request('one', 1)), provider.call(request('two', 1)),
    ]);
    for (const reply of failed) {
      assert.equal(reply.ok, false);
      assert.equal(reply.error?.code, 'interrupted');
      assert.equal(reply.error?.details?.cleanup_unconfirmed, true);
      assert.equal(reply.session_id, undefined, 'failed launch must not publish a fake session');
    }
    assert.equal(starts().length, 2);
    assert.ok(starts().every(({ pid }) => alive(pid)));

    // Clock advancement deterministically spends the deadline before a reap;
    // process identities, children, ACP traffic and public calls remain real.
    const expired = async (call) => {
      let now = Date.now();
      const clock = t.mock.method(Date, 'now', () => { now += 100; return now; });
      try { return await call(); } finally { clock.mock.restore(); }
    };
    const blocked = await expired(() => provider.call(request('one', 1)));
    assert.equal(blocked.ok, false);
    assert.equal(blocked.error?.details?.cleanup_unconfirmed, true);
    assert.equal(starts().length, 2, 'a fenced retry must not spawn another child');
    assert.ok(starts().every(({ pid }) => alive(pid)));

    // One TERM-resistant owner spends the entire recovery budget. A concurrent
    // public retry must not join/re-enter cleanup, nor spend another full
    // budget on the second owner.
    const recovering = provider.call(request('one', 100));
    const concurrent = await provider.call(request('two', 3000));
    assert.equal(concurrent.ok, false);
    assert.equal(concurrent.error?.code, 'interrupted');
    assert.equal(concurrent.error?.message, 'cleanup_recovery_in_flight');
    const budgetSpent = await recovering;
    assert.equal(budgetSpent.ok, false);
    assert.equal(budgetSpent.error?.code, 'interrupted');
    assert.equal(starts().length, 2);
    await waitUntil(() => starts().filter(({ pid }) => alive(pid)).length === 1);
    assert.equal(records().filter((record) => record.event === 'term').length, 1,
      'the request must not grant the second owner a fresh reap deadline');

    // Only public launch retries settle these owners; no engine adoption,
    // identity mutation or test settlement hook substitutes for recovery.
    const recovered = await provider.call(request('one', 5000));
    assert.equal(recovered.ok, true, JSON.stringify(recovered));
    assert.ok(recovered.session_id);
    assert.equal(starts().length, 3, 'exactly one replacement launch follows confirmed cleanup');
    assert.ok(starts().slice(0, 2).every(({ pid }) => !alive(pid)));

    // Published-session fences stay on their original shutdown route.
    const shutdown = (deadlineMs) => provider.call({
      request_id: 'shutdown', method: 'shutdown', session_id: recovered.session_id,
      deadline_ms: deadlineMs, payload: {},
    });
    const failedShutdown = await expired(() => shutdown(1));
    assert.equal(failedShutdown.ok, false);
    assert.equal(failedShutdown.error?.details?.cleanup_unconfirmed, true);
    const termCount = records().filter((record) => record.event === 'term').length;
    const liveBlocked = await provider.call(request('one', 3000, 'probe'));
    assert.equal(liveBlocked.ok, false);
    assert.equal(liveBlocked.error?.details?.cleanup_unconfirmed, true);
    assert.equal(starts().length, 3);
    assert.equal(records().filter((record) => record.event === 'term').length, termCount);
    assert.ok(alive(starts()[2].pid), 'public recovery must not consume a published session');
    assert.equal((await shutdown(3000)).ok, true);
    await waitUntil(() => starts().every(({ pid }) => !alive(pid)));
  });
