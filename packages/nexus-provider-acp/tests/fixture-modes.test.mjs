import { describe, test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, realpathSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createAcpProvider, parseAdmittedRecipe } from '../dist/index.js';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = join(__dirname, '..', '..', '..');
const fixture = resolve(root, 'crates/nexus-agent-host/tests/fixtures/mock_acp_workflow.py');

function resolvePython() {
  const which = execFileSync('which', ['python3'], { encoding: 'utf8' }).trim();
  return realpathSync(which);
}

function admittedRecipe(env, workspace, generation = 'gen-1') {
  return {
    provider_id: 'mock-acp',
    recipe_generation: generation,
    executable: resolvePython(),
    args: [fixture],
    env,
    cwd: workspace,
  };
}

function pidAlive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

async function drainTerminal(provider, opId, _sessionId) {
  let terminal = false;
  for (let i = 0; i < 30 && !terminal; i += 1) {
    const batch = await provider.next(opId, 16, 256 * 1024);
    for (const event of batch.events ?? []) {
      if (event.OpFinished || event.OpFailed) terminal = true;
    }
    if (!batch.has_more && terminal) break;
    await new Promise((r) => setTimeout(r, 50));
  }
  return terminal;
}

describe('fixture modes (real SDK)', () => {
  test('rejects non-absolute executable recipe', () => {
    assert.throws(
      () =>
        parseAdmittedRecipe({
          recipe: {
            provider_id: 'mock-acp',
            recipe_generation: '1',
            executable: 'python3',
            args: [fixture],
            env: {},
            cwd: '/tmp',
          },
        }),
      /invalid_recipe/,
    );
  });

  test('EOF_AFTER_INIT probe reports unavailable', async () => {
    const provider = createAcpProvider();
    const workspace = mkdtempSync(join(tmpdir(), 'acp-eof-'));
    const log = join(workspace, 'fixture.log');
    const reply = await provider.call({
      request_id: 'probe-eof',
      method: 'probe',
      deadline_ms: 30_000,
      payload: {
        provider_id: 'mock-acp',
        recipe: admittedRecipe({ EOF_AFTER_INIT: '1', ACP_FIXTURE_LOG: log }, workspace),
      },
    });
    assert.equal(reply.ok, true);
    assert.equal(reply.health?.available, false);
    assert.ok((reply.health?.latency_ms ?? 0) > 0);
  });

  test('STALL_AFTER_INIT probe still succeeds and reaps', async () => {
    const provider = createAcpProvider();
    const workspace = mkdtempSync(join(tmpdir(), 'acp-stall-'));
    const reply = await provider.call({
      request_id: 'probe-stall',
      method: 'probe',
      deadline_ms: 30_000,
      payload: {
        provider_id: 'mock-acp',
        recipe: admittedRecipe({ STALL_AFTER_INIT: '1' }, workspace),
      },
    });
    assert.equal(reply.ok, true);
    assert.equal(reply.health?.available, true);
  });

  test('OVERSIZED_UPDATE overflows delivery', async () => {
    const provider = createAcpProvider();
    const workspace = mkdtempSync(join(tmpdir(), 'acp-big-'));
    const recipe = admittedRecipe({ OVERSIZED_UPDATE: '1' }, workspace);
    const launch = await provider.call({
      request_id: 'launch',
      method: 'launch',
      deadline_ms: 30_000,
      payload: { provider_id: 'mock-acp', recipe },
    });
    const exec = await provider.call({
      request_id: 'execute',
      method: 'execute',
      session_id: launch.session_id,
      deadline_ms: 30_000,
      payload: { kind: 'prompt', content: 'hello' },
    });
    let failed = false;
    for (let i = 0; i < 20; i += 1) {
      const batch = await provider.next(exec.operation_id, 16, 256 * 1024);
      for (const event of batch.events ?? []) {
        if (event.OpFailed?.error_message === 'delivery_overflow') failed = true;
      }
      if (!batch.has_more && failed) break;
      await new Promise((r) => setTimeout(r, 50));
    }
    assert.equal(failed, true);
    await provider.call({
      request_id: 'shutdown',
      method: 'shutdown',
      session_id: launch.session_id,
      deadline_ms: 30_000,
      payload: {},
    });
  });

  test('DELAYED_CANCEL_ACK cooperative cancel', async () => {
    const provider = createAcpProvider();
    const workspace = mkdtempSync(join(tmpdir(), 'acp-cancel-'));
    const log = join(workspace, 'fixture.log');
    const recipe = admittedRecipe(
      { BLOCK_PROMPT: '1', DELAYED_CANCEL_ACK: '1', ACP_FIXTURE_LOG: log },
      workspace,
    );
    const launch = await provider.call({
      request_id: 'launch',
      method: 'launch',
      deadline_ms: 30_000,
      payload: { provider_id: 'mock-acp', recipe },
    });
    const exec = await provider.call({
      request_id: 'execute',
      method: 'execute',
      session_id: launch.session_id,
      deadline_ms: 30_000,
      payload: { kind: 'prompt', content: 'block' },
    });
    const cancel = await provider.call({
      request_id: 'cancel',
      method: 'cancel',
      operation_id: exec.operation_id,
      deadline_ms: 30_000,
      payload: {},
    });
    assert.equal(cancel.ok, true);
    const terminal = await drainTerminal(provider, exec.operation_id, launch.session_id);
    assert.equal(terminal, true);
    const logText = readFileSync(log, 'utf8');
    assert.match(logText, /"event": "cancel"/);
  });

  test('DESCENDANT process tree is reaped on shutdown', async () => {
    const provider = createAcpProvider();
    const workspace = mkdtempSync(join(tmpdir(), 'acp-desc-'));
    const log = join(workspace, 'fixture.log');
    const recipe = admittedRecipe({ DESCENDANT: '1', ACP_FIXTURE_LOG: log }, workspace);
    const launch = await provider.call({
      request_id: 'launch',
      method: 'launch',
      deadline_ms: 30_000,
      payload: { provider_id: 'mock-acp', recipe },
    });
    let descendantPid = null;
    for (const line of readFileSync(log, 'utf8').split('\n')) {
      if (!line.trim()) continue;
      const entry = JSON.parse(line);
      if (entry.event === 'descendant_spawned') descendantPid = entry.child_pid;
    }
    assert.ok(descendantPid, 'descendant pid recorded');
    const shutdown = await provider.call({
      request_id: 'shutdown',
      method: 'shutdown',
      session_id: launch.session_id,
      deadline_ms: 30_000,
      payload: {},
    });
    assert.equal(shutdown.ok, true);
    await new Promise((r) => setTimeout(r, 500));
    assert.equal(pidAlive(descendantPid), false, `descendant pid ${descendantPid} still alive`);
  });
});
