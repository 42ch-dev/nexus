import { describe, test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, realpathSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { execFileSync, spawn } from 'node:child_process';
import { PassThrough } from 'node:stream';
import {
  createTestEngine,
  observeProcessIdentity,
  reapChild,
} from '../dist/index.js';

function resolveNode() {
  return realpathSync(execFileSync('which', ['node'], { encoding: 'utf8' }).trim());
}

function resolvePython() {
  return realpathSync(execFileSync('which', ['python3'], { encoding: 'utf8' }).trim());
}

describe('pre-engine cleanup fence', () => {
  test('unconfirmed pre-init owner is fenced, blocks calls, then settles', async () => {
    const prev = process.env.NEXUS_ACP_TEST_REAP_MS;
    process.env.NEXUS_ACP_TEST_REAP_MS = '50';
    const workspace = mkdtempSync(join(tmpdir(), 'nexus-pre-engine-'));
    const fixture = resolve(
      '../../crates/nexus-agent-host/tests/fixtures/mock_acp_workflow.py',
    );

    const child = spawn(
      resolveNode(),
      ['-e', 'process.on("SIGTERM",()=>{});setInterval(()=>{},1e6)'],
      { stdio: ['pipe', 'pipe', 'pipe'] },
    );
    const boundIdentity = observeProcessIdentity(child);
    const owner = {
      recipeGeneration: 'pre-engine',
      child,
      connection: null,
      acpSessionId: null,
      stdinWritable: child.stdin ?? new PassThrough(),
      admittedIdentity: null,
      boundIdentity,
    };

    const reap = await reapChild(child, boundIdentity, 50);
    assert.equal(reap.confirmed, false);

    const engine = createTestEngine();
    engine.adoptCleanupOwner('pre-init', owner);
    assert.equal(engine.cleanupFenceCount(), 1);

    const blocked = await engine.call({
      request_id: 'blocked',
      method: 'probe',
      deadline_ms: 5000,
      payload: {
        provider_id: 'mock-acp',
        recipe: {
          provider_id: 'mock-acp',
          recipe_generation: 'x',
          executable: resolvePython(),
          args: [fixture],
          env: {},
          cwd: resolve(workspace),
        },
      },
    });
    assert.equal(blocked.ok, false);
    assert.match(blocked.error?.message ?? '', /cleanup_fence_active/);

    child.kill('SIGKILL');
    const settlement = await engine.trySettleCleanupFences();
    assert.ok(settlement.settled.includes('pre-init'));
    assert.equal(engine.cleanupFenceCount(), 0);

    process.env.NEXUS_ACP_TEST_REAP_MS = prev;
  });
});
