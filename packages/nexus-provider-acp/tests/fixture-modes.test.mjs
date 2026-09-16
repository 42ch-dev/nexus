import { describe, test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
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
      payload: { Prompt: { op_id: randomUUID(), content: [{ Text: { text: 'hello' } }], permission_scope: null } },
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
      payload: { Prompt: { op_id: randomUUID(), content: [{ Text: { text: 'block' } }], permission_scope: null } },
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

// A deterministic ACP protocol peer with real SDK validation. It advertises a
// non-hardcoded model config ID and requests permission during each prompt.
const controlPeer = String.raw`
const { createInterface } = require('node:readline');
const { appendFileSync } = require('node:fs');
const send = (value) => process.stdout.write(JSON.stringify(value) + '\n');
const config = { id: 'provider-model-selector', name: 'Model', category: 'model',
  type: 'select', currentValue: 'alpha',
  options: [{ value: 'alpha', name: 'Alpha' }, { value: 'beta', name: 'Beta' }] };
let prompt;
forAwait();
async function forAwait() {
  for await (const line of createInterface({ input: process.stdin })) {
    const req = JSON.parse(line);
    appendFileSync(process.env.PEER_LOG, JSON.stringify(req) + '\n');
    if (req.id === 'permission-1' && !req.method) {
      send({ jsonrpc: '2.0', method: 'session/update', params: {
        sessionId: prompt.params.sessionId, update: { sessionUpdate: 'agent_message_chunk',
          content: { type: 'text', text: process.env.PEER_MESSAGE ?? 'complete-message' } } } });
      send({ jsonrpc: '2.0', id: prompt.id, result: { stopReason: 'end_turn' } });
      continue;
    }
    const reply = (result) => send({ jsonrpc: '2.0', id: req.id, result });
    if (req.method === 'initialize') reply({ protocolVersion: 1, agentCapabilities: {} });
    else if (req.method === 'session/new') reply({ sessionId: 'peer-session',
      configOptions: process.env.NO_MODEL ? [] : [config] });
    else if (req.method === 'session/set_mode') {
      if (process.env.REJECT_MODE) send({ jsonrpc: '2.0', id: req.id,
        error: { code: -32601, message: 'unsupported mode' } });
      else reply({});
    } else if (req.method === 'session/set_config_option') {
      config.currentValue = req.params.value;
      reply({ configOptions: [config] });
    } else if (req.method === 'session/prompt') {
      prompt = req;
      send({ jsonrpc: '2.0', id: 'permission-1', method: 'session/request_permission',
        params: { sessionId: req.params.sessionId, toolCall: {
          toolCallId: 'write-1', title: 'Write file', kind: 'edit', status: 'pending' },
          options: [{ optionId: 'allow', name: 'Allow', kind: 'allow_once' },
            { optionId: 'deny', name: 'Deny', kind: 'reject_once' }] } });
    }
  }
}
`;

function controlRecipe(workspace, env = {}, providerId = 'configured-control', generation = 'generation-control') {
  return {
    provider_id: providerId, recipe_generation: generation,
    executable: realpathSync(process.execPath), args: ['-e', controlPeer],
    env: { PEER_LOG: join(workspace, `${providerId}.jsonl`), ...env }, cwd: workspace,
  };
}

function request(method, sessionId, payload) {
  return { request_id: randomUUID(), method, session_id: sessionId, deadline_ms: 30_000, payload };
}

async function finishedEvents(provider, operationId) {
  const events = [];
  for (let i = 0; i < 100; i += 1) {
    const batch = await provider.next(operationId, 16, 256 * 1024);
    assert.ok(!batch.gap, JSON.stringify(batch.gap));
    events.push(...batch.events);
    if (!batch.has_more) {
      assert.ok(events.some((event) => event.OpFinished), JSON.stringify(events));
      return events;
    }
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  assert.fail('missing terminal from deterministic peer');
}

describe('HostOperation callback contract', () => {
  test('controls, content and narrowing permissions stay on the selected session', async () => {
    const provider = createAcpProvider();
    const workspace = mkdtempSync(join(tmpdir(), 'acp-controls-'));
    const recipe = controlRecipe(workspace);
    const launch = await provider.call(request('launch', undefined, { provider_id: recipe.provider_id, recipe }));
    assert.equal(launch.ok, true, JSON.stringify(launch));
    try {
      for (const payload of [{ SetModel: { model: 'beta' } }, { SetMode: { mode: 'plan' } }]) {
        const reply = await provider.call(request('execute', launch.session_id, payload));
        assert.equal(reply.ok, true, JSON.stringify(reply));
        const events = await finishedEvents(provider, reply.operation_id);
        assert.ok(events.every((event) => !event.OpFailed));
      }
      const content = [{ Text: { text: 'one' } }, { ResourceLink: { name: 'source', uri: 'file:///source.txt' } },
        { Text: { text: 'two' } }];
      const opId = randomUUID();
      const payload = { Prompt: { op_id: opId, content,
        permission_scope: { allow_read: true, allow_write: false, allow_destructive: false } } };
      const reply = await provider.call(request('execute', launch.session_id, payload));
      assert.equal(reply.ok, true, JSON.stringify(reply));
      assert.equal(reply.operation_id, opId);
      const events = await finishedEvents(provider, opId);
      assert.deepEqual(events.filter((event) => event.MessageDelta).map((event) => event.MessageDelta.text), ['complete-message']);
      const duplicate = await provider.call(request('execute', launch.session_id, payload));
      assert.equal(duplicate.error?.code, 'invalid_input');
      const invalidScope = await provider.call(request('execute', launch.session_id, {
        Prompt: { op_id: randomUUID(), content, permission_scope: { allow_read: 'yes' } },
      }));
      assert.equal(invalidScope.error?.code, 'invalid_input');
      const log = readFileSync(recipe.env.PEER_LOG, 'utf8').trim().split('\n').map(JSON.parse);
      const model = log.find((entry) => entry.method === 'session/set_config_option');
      assert.deepEqual(model.params, { sessionId: 'peer-session', configId: 'provider-model-selector', value: 'beta' });
      assert.deepEqual(log.find((entry) => entry.method === 'session/set_mode').params, {
        sessionId: 'peer-session', modeId: 'plan',
      });
      const prompts = log.filter((entry) => entry.method === 'session/prompt');
      assert.equal(prompts.length, 1, 'invalid/duplicate operations have zero RPCs');
      assert.deepEqual(prompts[0].params.prompt, [
        { type: 'text', text: 'one' }, { type: 'resource_link', name: 'source', uri: 'file:///source.txt' },
        { type: 'text', text: 'two' },
      ]);
      assert.deepEqual(log.find((entry) => entry.id === 'permission-1' && !entry.method).result,
        { outcome: { outcome: 'cancelled' } }, 'a narrowing scope must never grant a write');
    } finally {
      assert.equal((await provider.call(request('shutdown', launch.session_id, {}))).ok, true);
    }
  });

  test('unsupported controls never acknowledge success or invoke another adapter', async () => {
    const provider = createAcpProvider();
    const workspace = mkdtempSync(join(tmpdir(), 'acp-unsupported-'));
    const recipe = controlRecipe(workspace, { NO_MODEL: '1', REJECT_MODE: '1' });
    const launch = await provider.call(request('launch', undefined, { recipe }));
    assert.equal(launch.ok, true);
    try {
      const model = await provider.call(request('execute', launch.session_id, { SetModel: { model: 'beta' } }));
      assert.equal(model.ok, false);
      assert.equal(model.error?.code, 'not_supported');
      const mode = await provider.call(request('execute', launch.session_id, { SetMode: { mode: 'plan' } }));
      assert.equal(mode.ok, false);
      assert.equal(mode.error?.code, 'not_supported');
      const log = readFileSync(recipe.env.PEER_LOG, 'utf8').trim().split('\n').map(JSON.parse);
      assert.equal(log.filter((entry) => entry.method === 'session/set_config_option').length, 0);
      assert.equal(log.filter((entry) => entry.method === 'session/set_mode').length, 1);
      assert.equal(log.filter((entry) => entry.method === 'session/new').length, 1);
    } finally {
      await provider.call(request('shutdown', launch.session_id, {}));
    }
  });

  test('different admitted provider generations cannot evict each other', async () => {
    const provider = createAcpProvider();
    const workspace = mkdtempSync(join(tmpdir(), 'acp-generations-'));
    const sessions = [];
    try {
      for (const [id, generation] of [['provider-a', 'generation-a'], ['provider-b', 'generation-b']]) {
        const recipe = controlRecipe(workspace, { PEER_MESSAGE: id }, id, generation);
        const launch = await provider.call(request('launch', undefined, { recipe }));
        assert.equal(launch.ok, true);
        sessions.push(launch.session_id);
      }
      for (const sessionId of sessions) {
        const reply = await provider.call(request('execute', sessionId, { SetMode: { mode: 'plan' } }));
        assert.equal(reply.ok, true, 'the other provider must not close this owned session');
        await finishedEvents(provider, reply.operation_id);
      }
      // Both connections deliberately use the same ACP session ID. Their
      // notification callbacks must still target distinct Nexus operations.
      const replies = await Promise.all(sessions.map((sessionId) => provider.call(request('execute', sessionId, {
        Prompt: { op_id: randomUUID(), content: [{ Text: { text: 'hello' } }], permission_scope: null },
      }))));
      assert.ok(replies.every((reply) => reply.ok));
      const events = await Promise.all(replies.map((reply) => finishedEvents(provider, reply.operation_id)));
      assert.deepEqual(events.map((batch) => batch.filter((event) => event.MessageDelta).map((event) => event.MessageDelta.text)),
        [['provider-a'], ['provider-b']]);
    } finally {
      for (const sessionId of sessions) await provider.call(request('shutdown', sessionId, {}));
    }
  });
});
