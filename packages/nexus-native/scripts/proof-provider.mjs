#!/usr/bin/env node
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, realpathSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
import { randomUUID } from 'node:crypto';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = join(__dirname, '..', '..', '..');

const adapter = process.argv.includes('--adapter')
  ? process.argv[process.argv.indexOf('--adapter') + 1]
  : 'wire';
const caseName = process.argv.includes('--case')
  ? process.argv[process.argv.indexOf('--case') + 1]
  : 'wire';
const outDir = process.argv.includes('--out')
  ? process.argv[process.argv.indexOf('--out') + 1]
  : join(root, '.mstar', 'iterations', 'v1.189', 'guides', 'evidence', 'native-wire');

const home = mkdtempSync(join(tmpdir(), 'nexus-provider-proof-'));
const seed = spawnSync(
  'cargo',
  ['run', '-q', '-p', 'nexus-core-node', '--bin', 'native-wire-fixture-seed', '--', home],
  { cwd: root },
);
if (seed.status !== 0) {
  console.error(seed.stderr?.toString());
  process.exit(seed.status ?? 1);
}

const require = createRequire(import.meta.url);
const { loadNodePath } = await import('../dist/loader.js');
const nodePath = loadNodePath();
const binding = require(nodePath);

const encode = (value) => new TextEncoder().encode(JSON.stringify(value));
const decode = (buffer) => JSON.parse(new TextDecoder().decode(buffer));

async function runWireProof(core) {
  const query = (request) => core.hostQuery(encode(request)).then(decode);
  const health = await query({ query: 'health' });
  if (health.health?.running !== true) {
    console.error('host health not running', health);
    process.exit(1);
  }
  const catalog = await query({ query: 'catalog' });
  for (const provider of catalog.catalog?.providers ?? []) {
    if (provider.protocol_kind !== 'acp' && provider.protocol_kind !== 'native_cli') {
      console.error('protocol_kind is not contract snake_case', provider);
      process.exit(1);
    }
  }
  const listed = await query({ query: 'list_sessions' });
  const ids = (listed.sessions?.items ?? []).map((item) => item.session_id);
  for (let index = 1; index < ids.length; index += 1) {
    if (ids[index - 1] > ids[index]) {
      console.error('session snapshot is not sorted', ids);
      process.exit(1);
    }
  }
  const probeErr = await core
    .providerCall(encode({ request_id: 'probe-1', method: 'probe', deadline_ms: 30_000, payload: { provider_id: 'missing-provider' } }))
    .then(() => null)
    .catch((error) => String(error));
  if (!probeErr) {
    console.error('expected not-found for missing provider');
    process.exit(1);
  }
}
function unpackCallbackPayload(...args) {
  const payload = args.length > 1 ? args[1] : args[0];
  return typeof payload === 'string' ? JSON.parse(payload) : payload;
}

function resolveAdmittedPython() {
  const candidates = [process.env.PYTHON, process.env.PYTHON3, 'python3'].filter(Boolean);
  for (const candidate of candidates) {
    try {
      const resolved = execFileSync('which', [candidate], { encoding: 'utf8' }).trim();
      if (resolved.startsWith('/')) {
        return realpathSync(resolved);
      }
    } catch {
      // try next candidate
    }
  }
  throw new Error('no_absolute_python_executable');
}

function writeAgentHostConfig(home, fixturePath, workspace, extraEnv = {}) {
  const python = resolveAdmittedPython();
  const toml = `[[providers]]
id = "mock-acp"
protocol = "acp"
command = "${python}"
args = ["${fixturePath.replaceAll('\\', '/')}"]
enabled = true

[providers.env]
ACP_FIXTURE_LOG = "${join(workspace, 'fixture.log').replaceAll('\\', '/')}"
${Object.entries(extraEnv).map(([k, v]) => `${k} = "${String(v).replaceAll('"', '\\"')}"`).join('\n')}
`;
  const configDir = join(home, 'config');
  mkdirSync(configDir, { recursive: true });
  writeFileSync(join(configDir, 'agent-host.toml'), toml);
}



function isStaleGenerationError(err) {
  const s = String(err ?? '');
  return /generation|dead_env|invalid principal|tombstone|closing|interrupted/i.test(s);
}

function isTransportEofError(err) {
  const s = String(err ?? '');
  return /eof|broken pipe|connection reset|transport|child process|exited|closed/i.test(s);
}

async function runEofWhileLive(core, adapter, home, providers) {
  const eofHome = mkdtempSync(join(tmpdir(), 'nexus-eof-live-'));
  const eofWorkspace = mkdtempSync(join(tmpdir(), 'nexus-eof-ws-'));
  const fixture = resolve(root, 'crates/nexus-agent-host/tests/fixtures/mock_acp_workflow.py');
  writeAgentHostConfig(eofHome, fixture, eofWorkspace, { EOF_AFTER_INIT: '1' });
  const eofCore = binding.open(
    JSON.stringify({ user_home: eofHome, access: 'engine_owner', allow_uninitialized: false }),
    providers,
  );
  const launchErr = await eofCore
    .providerCall(
      encode({
        request_id: 'eof-live-launch',
        method: 'launch',
        deadline_ms: 5_000,
        payload: adapter === 'rust-acp' ? rustLaunchPayload(eofHome) : tsLaunchPayload(),
      }),
    )
    .then(() => null)
    .catch((error) => String(error));
  await eofCore.close().catch(() => {});
  if (!launchErr || !isTransportEofError(launchErr)) {
    return failScenario('eof_after_close', `expected live EOF failure, got: ${launchErr}`);
  }
  return passScenario('eof_after_close', { error: launchErr, adapter, mode: 'EOF_AFTER_INIT_live' });
}

async function runFullQueueShutdown(core, adapter, home) {
  const burst = [];
  for (let i = 0; i < 17; i += 1) {
    burst.push(
      core
        .providerCall(
          encode({
            request_id: `queue-${i}`,
            method: 'probe',
            deadline_ms: 30_000,
            payload: adapter === 'rust-acp' ? rustProbePayload(home) : tsProbePayload(),
          }),
        )
        .catch((error) => ({ ok: false, error: String(error) })),
    );
  }
  const started = Date.now();
  await Promise.allSettled(burst);
  const closeStarted = Date.now();
  const closeReport = decode(await core.close());
  const closeMs = Date.now() - closeStarted;
  const totalMs = Date.now() - started;
  const survivors = findFixtureChildPids();
  if (closeMs > 5_000) {
    return failScenario('full_queue_shutdown', `close exceeded 5s: ${closeMs}ms`);
  }
  if (!closeReport.cleanup_confirmed) {
    return failScenario('full_queue_shutdown', 'cleanup unconfirmed after burst');
  }
  if (survivors.length > 0) {
    return failScenario('full_queue_shutdown', `surviving child pids: ${survivors.join(',')}`);
  }
  return passScenario('full_queue_shutdown', {
    burst_count: 17,
    close_ms: closeMs,
    total_ms: totalMs,
    adapter,
    surviving_child_pids: survivors,
  });
}

const SCENARIO_KEYS = [
  'terminal_identity',
  'cooperative_cancel_2s',
  'full_queue_shutdown',
  'never_settling_callback',
  'failed_open_child',
  'eof_after_close',
  'late_generation',
  'worker_termination',
  'reentrant_call',
  'multibyte_overflow',
  'open_close_100',
  'happy_lifecycle',
];

function failScenario(key, message) {
  return { key, ok: false, error: message };
}

function passScenario(key, detail = {}) {
  return { key, ok: true, ...detail };
}

function rssSnapshot() {
  const mem = process.memoryUsage();
  return { rss: mem.rss, heapUsed: mem.heapUsed, external: mem.external };
}

function findFixtureChildPids() {
  try {
    const out = execFileSync('pgrep', ['-f', 'mock_acp_workflow.py'], { encoding: 'utf8' }).trim();
    return out ? out.split('\n').map((line) => Number(line)).filter(Boolean) : [];
  } catch {
    return [];
  }
}

async function drainTerminal(core, operationId) {
  let terminalOpId = null;
  let terminalSessionId = null;
  let sawDelta = false;
  let terminal = false;
  for (let i = 0; i < 40 && !terminal; i += 1) {
    const batch = decode(await core.nextProviderEvents(operationId, 16, 256 * 1024));
    for (const event of batch.events ?? []) {
      if (event.MessageDelta) sawDelta = true;
      if (event.OpFinished || event.OpFailed) {
        terminal = true;
        terminalOpId = event.OpFinished?.operation_id ?? event.OpFailed?.operation_id ?? operationId;
        terminalSessionId = event.OpFinished?.session_id ?? event.OpFailed?.session_id ?? null;
      }
    }
    if (!batch.has_more && terminal) break;
    await new Promise((r) => setTimeout(r, 50));
  }
  return { sawDelta, terminal, terminalOpId, terminalSessionId };
}

async function runOpenClose100(home, accessJson, providers = undefined) {
  const rssStart = rssSnapshot();
  let rssPeak = rssStart.rss;
  for (let cycle = 0; cycle < 100; cycle += 1) {
    const core = binding.open(accessJson, providers);
    const report = decode(await core.close());
    const rss = rssSnapshot();
    rssPeak = Math.max(rssPeak, rss.rss);
    if (!report.cleanup_confirmed || report.state !== 'closed') {
      return failScenario('open_close_100', `cycle ${cycle} close unconfirmed`);
    }
  }
  const rssEnd = rssSnapshot();
  return passScenario('open_close_100', {
    cycles: 100,
    rss_start: rssStart.rss,
    rss_end: rssEnd.rss,
    rss_peak: rssPeak,
  });
}

async function runAdapterScenarios(core, ctx) {
  const { adapter, home, sessionId, operationId, terminalPayload, accessJson, providers } = ctx;
  const scenarios = {};
  const rssStart = rssSnapshot();
  let rssPeak = rssStart.rss;

  scenarios.terminal_identity =
    terminalPayload?.terminal && terminalPayload.terminalOpId === operationId
      ? passScenario('terminal_identity', {
          operation_id: operationId,
          terminal_operation_id: terminalPayload.terminalOpId,
          session_id: sessionId,
          terminal_session_id: terminalPayload.terminalSessionId,
        })
      : failScenario('terminal_identity', 'terminal operation/session identity mismatch');

  // cooperative cancel <=2s
  try {
    const launch = decode(
      await core.providerCall(
        encode({
          request_id: 'cancel-launch',
          method: 'launch',
          deadline_ms: 30_000,
          payload: adapter === 'rust-acp' ? rustLaunchPayload(home) : tsLaunchPayload(),
        }),
      ),
    );
    const cancelSession = launch.session_id;
    const exec = decode(
      await core.providerCall(
        encode({
          request_id: 'cancel-exec',
          method: 'execute',
          session_id: cancelSession,
          deadline_ms: 30_000,
          payload: adapter === 'rust-acp' ? rustExecutePayload() : tsExecutePayload(),
        }),
      ),
    );
    const started = Date.now();
    await core.providerCall(
      encode({
        request_id: 'cancel-op',
        method: 'cancel',
        session_id: cancelSession,
        operation_id: exec.operation_id,
        deadline_ms: 30_000,
        payload: {},
      }),
    );
    const cancelMs = Date.now() - started;
    scenarios.cooperative_cancel_2s = cancelMs <= 2000
      ? passScenario('cooperative_cancel_2s', { cancel_ms: cancelMs })
      : failScenario('cooperative_cancel_2s', `cancel took ${cancelMs}ms`);
    await core.providerCall(
      encode({
        request_id: 'cancel-shutdown',
        method: 'shutdown',
        session_id: cancelSession,
        deadline_ms: 30_000,
        payload: {},
      }),
    );
  } catch (error) {
    scenarios.cooperative_cancel_2s = failScenario('cooperative_cancel_2s', String(error));
  }

  // never-settling: BLOCK_PROMPT fixture blocks until cancel
  try {
    const blockHome = mkdtempSync(join(tmpdir(), 'nexus-block-prompt-'));
    const blockWs = mkdtempSync(join(tmpdir(), 'nexus-block-ws-'));
    const fixture = resolve(root, 'crates/nexus-agent-host/tests/fixtures/mock_acp_workflow.py');
    writeAgentHostConfig(blockHome, fixture, blockWs, { BLOCK_PROMPT: '1' });
    const blockCore = binding.open(
      JSON.stringify({ user_home: blockHome, access: 'engine_owner', allow_uninitialized: false }),
      providers,
    );
    const launch = decode(
      await blockCore.providerCall(
        encode({
          request_id: 'block-launch',
          method: 'launch',
          deadline_ms: 30_000,
          payload: adapter === 'rust-acp' ? rustLaunchPayload(blockHome) : tsLaunchPayload(),
        }),
      ),
    );
    const hangSession = launch.session_id;
    const exec = decode(
      await blockCore.providerCall(
        encode({
          request_id: 'block-exec',
          method: 'execute',
          session_id: hangSession,
          deadline_ms: 30_000,
          payload: adapter === 'rust-acp' ? rustExecutePayload() : tsExecutePayload(),
        }),
      ),
    );
    const started = Date.now();
    await blockCore.providerCall(
      encode({
        request_id: 'block-cancel',
        method: 'cancel',
        session_id: hangSession,
        operation_id: exec.operation_id,
        deadline_ms: 30_000,
        payload: {},
      }),
    );
    const cancelMs = Date.now() - started;
    const closeStarted = Date.now();
    const closeReport = decode(await blockCore.close());
    const closeMs = Date.now() - closeStarted;
    scenarios.never_settling_callback =
      cancelMs <= 2000 && closeReport.cleanup_confirmed && closeMs <= 5_000
        ? passScenario('never_settling_callback', { cancel_ms: cancelMs, close_ms: closeMs, adapter })
        : failScenario('never_settling_callback', `cancel_ms=${cancelMs} close_ms=${closeMs} confirmed=${closeReport.cleanup_confirmed}`);
  } catch (error) {
    scenarios.never_settling_callback = failScenario('never_settling_callback', String(error));
  }

  // failed open child
  try {
    const badHome = mkdtempSync(join(tmpdir(), 'nexus-bad-child-'));
    mkdirSync(join(badHome, 'config'), { recursive: true });
    writeFileSync(
      join(badHome, 'config', 'agent-host.toml'),
      `[[providers]]\nid = "bad"\nprotocol = "acp"\ncommand = "/nonexistent/nexus-bad-acp"\nargs = []\nenabled = true\n`,
    );
    const badCore = binding.open(
      JSON.stringify({ user_home: badHome, access: 'engine_owner', allow_uninitialized: false }),
      providers,
    );
    const badLaunch = await badCore
      .providerCall(
        encode({
          request_id: 'bad-launch',
          method: 'launch',
          deadline_ms: 5_000,
          payload: adapter === 'rust-acp' ? rustLaunchPayload(badHome) : tsLaunchPayload(),
        }),
      )
      .then((buf) => decode(buf))
      .catch((error) => ({ ok: false, error: String(error) }));
    scenarios.failed_open_child = !badLaunch.ok
      ? passScenario('failed_open_child', { error: badLaunch.error ?? null })
      : failScenario('failed_open_child', 'expected launch failure for bad child');
    await badCore.close().catch(() => {});
  } catch (error) {
    scenarios.failed_open_child = failScenario('failed_open_child', String(error));
  }

  // full queue: run on a dedicated core so main session can continue
  try {
    const queueCore = binding.open(accessJson, providers);
    scenarios.full_queue_shutdown = await runFullQueueShutdown(queueCore, adapter, home);
    if (!scenarios.full_queue_shutdown.ok) {
      // already failed
    }
  } catch (error) {
    scenarios.full_queue_shutdown = failScenario('full_queue_shutdown', String(error));
  }

  // multibyte overflow via oversized UTF-8 prompt payload
  try {
    const launch = decode(
      await core.providerCall(
        encode({
          request_id: 'mb-launch',
          method: 'launch',
          deadline_ms: 30_000,
          payload: adapter === 'rust-acp' ? rustLaunchPayload(home) : tsLaunchPayload(),
        }),
      ),
    );
    const mbSession = launch.session_id;
    const big = '🎉'.repeat(400_000);
    const execErr = await core
      .providerCall(
        encode({
          request_id: 'mb-exec',
          method: 'execute',
          session_id: mbSession,
          deadline_ms: 5_000,
          payload:
            adapter === 'rust-acp'
              ? {
                  Prompt: {
                    op_id: randomUUID(),
                    content: [{ Text: { text: big } }],
                    permission_scope: null,
                  },
                }
              : { kind: 'prompt', content: big },
        }),
      )
      .then(() => null)
      .catch((error) => String(error));
    scenarios.multibyte_overflow = execErr
      ? passScenario('multibyte_overflow', { error: execErr, utf8_bytes: new TextEncoder().encode(big).length })
      : failScenario('multibyte_overflow', 'expected oversize rejection');
    await core.providerCall(
      encode({
        request_id: 'mb-shutdown',
        method: 'shutdown',
        session_id: mbSession,
        deadline_ms: 30_000,
        payload: {},
      }),
    ).catch(() => {});
  } catch (error) {
    scenarios.multibyte_overflow = failScenario('multibyte_overflow', String(error));
  }

  // worker termination: kill fixture child and ensure shutdown reports
  try {
    const beforePids = findFixtureChildPids();
    const launch = decode(
      await core.providerCall(
        encode({
          request_id: 'kill-launch',
          method: 'launch',
          deadline_ms: 30_000,
          payload: adapter === 'rust-acp' ? rustLaunchPayload(home) : tsLaunchPayload(),
        }),
      ),
    );
    const killSession = launch.session_id;
    const afterLaunchPids = findFixtureChildPids();
    const victim = afterLaunchPids.find((pid) => !beforePids.includes(pid)) ?? afterLaunchPids[0];
    if (victim) {
      try {
        process.kill(victim, 'SIGKILL');
      } catch {
        // already dead
      }
    }
    const eofErr = await core
      .providerCall(
        encode({
          request_id: 'kill-probe',
          method: 'probe',
          deadline_ms: 2_000,
          payload: adapter === 'rust-acp' ? rustProbePayload(home) : tsProbePayload(),
        }),
      )
      .then(() => null)
      .catch((error) => String(error));
    if (!victim) {
      scenarios.worker_termination = failScenario('worker_termination', 'no fixture child pid observed');
    } else if (!eofErr) {
      scenarios.worker_termination = failScenario('worker_termination', 'post-kill probe succeeded unexpectedly');
    } else if (!isTransportEofError(eofErr)) {
      scenarios.worker_termination = failScenario('worker_termination', `unexpected post-kill error: ${eofErr}`);
    } else {
      scenarios.worker_termination = passScenario('worker_termination', {
        child_pid: victim,
        launch_pids: afterLaunchPids,
        post_kill_probe_error: eofErr,
        adapter,
      });
    }
    await core.providerCall(
      encode({
        request_id: 'kill-shutdown',
        method: 'shutdown',
        session_id: killSession,
        deadline_ms: 5_000,
        payload: {},
      }),
    ).catch((error) => ({ ok: false, error: String(error) }));
  } catch (error) {
    scenarios.worker_termination = failScenario('worker_termination', String(error));
  }

  // reentrant probe during execute stream
  try {
    const launch = decode(
      await core.providerCall(
        encode({
          request_id: 'reentrant-launch',
          method: 'launch',
          deadline_ms: 30_000,
          payload: adapter === 'rust-acp' ? rustLaunchPayload(home) : tsLaunchPayload(),
        }),
      ),
    );
    const reSession = launch.session_id;
    const exec = decode(
      await core.providerCall(
        encode({
          request_id: 'reentrant-exec',
          method: 'execute',
          session_id: reSession,
          deadline_ms: 30_000,
          payload: adapter === 'rust-acp' ? rustExecutePayload() : tsExecutePayload(),
        }),
      ),
    );
    const probeDuring = await core
      .providerCall(
        encode({
          request_id: 'reentrant-probe',
          method: 'probe',
          deadline_ms: 5_000,
          payload: adapter === 'rust-acp' ? rustProbePayload(home) : tsProbePayload(),
        }),
      )
      .then((buf) => decode(buf))
      .catch((error) => ({ ok: false, error: String(error) }));
    const pullDuring = await core
      .nextProviderEvents(exec.operation_id, 4, 64 * 1024)
      .then((buf) => decode(buf))
      .catch((error) => ({ error: String(error) }));
    const reentrantOk =
      probeDuring.ok ||
      (pullDuring && !pullDuring.error && Array.isArray(pullDuring.events));
    scenarios.reentrant_call = reentrantOk
      ? passScenario('reentrant_call', {
          probe_ok: probeDuring.ok,
          pull_events: pullDuring?.events?.length ?? 0,
          adapter,
        })
      : failScenario('reentrant_call', `probe and pull both failed: ${probeDuring.error ?? pullDuring?.error}`);
    await drainTerminal(core, exec.operation_id);
    await core.providerCall(
      encode({
        request_id: 'reentrant-shutdown',
        method: 'shutdown',
        session_id: reSession,
        deadline_ms: 30_000,
        payload: {},
      }),
    );
  } catch (error) {
    scenarios.reentrant_call = failScenario('reentrant_call', String(error));
  }

  scenarios.open_close_100 = await runOpenClose100(home, accessJson, providers);

  try {
    scenarios.eof_after_close = await runEofWhileLive(core, adapter, home, providers);
  } catch (error) {
    scenarios.eof_after_close = failScenario('eof_after_close', String(error));
  }
  scenarios.late_generation = failScenario('late_generation', 'resolved after env close');

  const rssEnd = rssSnapshot();
  rssPeak = Math.max(rssPeak, rssEnd.rss);
  scenarios._rss = { rss_start: rssStart.rss, rss_end: rssEnd.rss, rss_peak: rssPeak };
  return scenarios;
}


function hostOwner(home) {
  return {
    creator_id: 'proof-provider',
    workspace_root: home,
    orchestration_run_id: null,
  };
}

function rustProbePayload(home) {
  return {
    provider_id: 'mock-acp',
    timeout_ms: 30_000,
    cwd: home,
    owner: hostOwner(home),
  };
}

function rustLaunchPayload(home) {
  return {
    provider_id: 'mock-acp',
    cwd: home,
    mcp_servers: [],
    owner: hostOwner(home),
  };
}

function rustExecutePayload() {
  return {
    Prompt: {
      op_id: randomUUID(),
      content: [{ Text: { text: 'hello' } }],
      permission_scope: null,
    },
  };
}

function tsProbePayload() {
  return { provider_id: 'mock-acp' };
}

function tsLaunchPayload() {
  return { provider_id: 'mock-acp' };
}

function tsExecutePayload() {
  return { kind: 'prompt', content: 'hello' };
}

async function runAcpLifecycleSession(core, { adapter, sdk, admittedMeta = {}, home, accessJson, providers = undefined }) {
  const probePayload = adapter === 'rust-acp' ? rustProbePayload(home) : tsProbePayload();
  const launchPayload = adapter === 'rust-acp' ? rustLaunchPayload(home) : tsLaunchPayload();
  const executePayload = adapter === 'rust-acp' ? rustExecutePayload() : tsExecutePayload();

  let capturedGeneration = null;
  const probeReply = decode(
    await core.providerCall(
      encode({
        request_id: 'probe',
        method: 'probe',
        deadline_ms: 30_000,
        payload: probePayload,
      }),
    ),
  );
  if (!probeReply.ok || !probeReply.health?.available) {
    console.error('probe failed', probeReply);
    process.exit(1);
  }
  try {
    capturedGeneration = JSON.parse(await core.activePrincipal()).split(':')[1];
  } catch {
    capturedGeneration = null;
  }

  const launchReply = decode(
    await core.providerCall(
      encode({
        request_id: 'launch',
        method: 'launch',
        deadline_ms: 30_000,
        payload: launchPayload,
      }),
    ),
  );
  const sessionId = launchReply.session_id;
  if (!launchReply.ok || !sessionId) {
    console.error('launch failed', launchReply);
    process.exit(1);
  }

  const executeReply = decode(
    await core.providerCall(
      encode({
        request_id: 'execute',
        method: 'execute',
        session_id: sessionId,
        deadline_ms: 30_000,
        payload: executePayload,
      }),
    ),
  );
  const operationId = executeReply.operation_id;
  if (!executeReply.ok || !operationId) {
    console.error('execute failed', executeReply);
    process.exit(1);
  }

  const stream = await drainTerminal(core, operationId);
  const { sawDelta, terminal, terminalOpId, terminalSessionId } = stream;
  if (!sawDelta || !terminal) {
    console.error('missing prompt stream evidence', { sawDelta, terminal });
    process.exit(1);
  }
  for (let i = 0; i < 20; i += 1) {
    const batch = decode(await core.nextProviderEvents(operationId, 16, 256 * 1024));
    if (!batch.has_more && (batch.events?.length ?? 0) === 0) break;
    await new Promise((r) => setTimeout(r, 25));
  }
  await new Promise((r) => setTimeout(r, 200));

  let shutdownReply = { ok: false, error: null };
  try {
    shutdownReply = decode(
      await core.providerCall(
        encode({
          request_id: 'shutdown',
          method: 'shutdown',
          session_id: sessionId,
          deadline_ms: 30_000,
          payload: {},
        }),
      ),
    );
  } catch (error) {
    shutdownReply = { ok: false, error: String(error) };
  }

  if (!shutdownReply.ok) {
    console.error('session shutdown failed', shutdownReply);
    process.exit(1);
  }

  // Adapter scenarios need an open provider port; run before env close.
  const adapterScenarios = await runAdapterScenarios(core, {
    adapter,
    home,
    sessionId,
    operationId,
    terminalPayload: { terminal, terminalOpId, terminalSessionId },
    accessJson,
    providers,
  });

  const closeStarted = Date.now();
  const closeReport = decode(await core.close());
  const closeMs = Date.now() - closeStarted;
  if (closeMs > 5_000) {
    console.error('close exceeded 5s budget (5000ms)', closeMs);
    process.exit(1);
  }
  if (!closeReport.cleanup_confirmed || closeReport.state !== 'closed') {
    console.error('close did not confirm cleanup', closeReport);
    process.exit(1);
  }

  const postCloseErr = await core
    .providerCall(
      encode({
        request_id: 'post-close',
        method: 'probe',
        deadline_ms: 1_000,
        payload: adapter === 'rust-acp' ? rustProbePayload(home) : tsProbePayload(),
      }),
    )
    .then(() => null)
    .catch((error) => String(error));
  if (!postCloseErr) {
    console.error('expected post-close probe to fail');
    process.exit(1);
  }
  const scenarios = {
    happy_lifecycle: passScenario('happy_lifecycle', {
      session_shutdown_ok: shutdownReply.ok,
      close_ms: closeMs,
      cleanup_confirmed: closeReport.cleanup_confirmed,
    }),
    ...adapterScenarios,
  };
  if (!scenarios.eof_after_close?.ok) {
    scenarios.eof_after_close = failScenario('eof_after_close', 'live EOF scenario missing or failed');
  }
  scenarios.late_generation = isStaleGenerationError(postCloseErr)
    ? passScenario('late_generation', { error: postCloseErr, captured_generation: capturedGeneration })
    : failScenario('late_generation', `expected stale generation/tombstone error, got: ${postCloseErr}`);
  for (const key of SCENARIO_KEYS) {
    if (!scenarios[key]) {
      scenarios[key] = failScenario(key, 'missing scenario result');
    }
    if (!scenarios[key].ok) {
      console.error('scenario failed', key, scenarios[key]);
      process.exit(1);
    }
  }

  const rss = adapterScenarios._rss ?? rssSnapshot();
  delete scenarios._rss;
  const survivingPids = findFixtureChildPids();
  const evidence = {
    adapter,
    case: caseName,
    native_artifact: nodePath,
    native_artifact_bytes: require('node:fs').statSync(nodePath).size,
    probe_latency_ms: probeReply.health?.latency_ms ?? null,
    session_id: sessionId,
    operation_id: operationId,
    terminal_operation_id: terminalOpId,
    terminal_session_id: terminalSessionId,
    saw_delta: sawDelta,
    terminal,
    session_shutdown_ok: shutdownReply.ok,
    session_shutdown_error: shutdownReply.error ?? null,
    close_ms: closeMs,
    close_state: closeReport.state,
    cleanup_confirmed: closeReport.cleanup_confirmed,
    pending_operations: closeReport.pending_operations ?? [],
    rss_start: rss.rss_start,
    rss_end: rss.rss_end,
    rss_peak: rss.rss_peak,
    surviving_child_pids: survivingPids,
    scenarios,
    sdk,
    ...admittedMeta,
  };
  mkdirSync(outDir, { recursive: true });
  writeFileSync(join(outDir, 'lifecycle.json'), JSON.stringify(evidence, null, 2));
}

async function runRustAcpLifecycleProof() {
  const fixture = resolve(root, 'crates/nexus-agent-host/tests/fixtures/mock_acp_workflow.py');
  const workspace = mkdtempSync(join(tmpdir(), 'nexus-acp-ws-'));
  writeAgentHostConfig(home, fixture, workspace);
  const core = binding.open(
    JSON.stringify({ user_home: home, access: 'engine_owner', allow_uninitialized: false }),
  );
  const accessJson = JSON.stringify({ user_home: home, access: 'engine_owner', allow_uninitialized: false });
  await runAcpLifecycleSession(core, {
    adapter: 'rust-acp',
    home,
    accessJson,
    sdk: 'agent-client-protocol=2.1.0',
    admittedMeta: {
      fixture,
      localset_bridge: 'nexus-acp-host',
      rust_admission_boundary: 'host_catalog_only',
    },
  });
}


async function runTsAcpLifecycleProof() {
  const build = spawnSync('pnpm', ['--filter', '@42ch/nexus-provider-acp', 'build'], {
    cwd: root,
    stdio: 'inherit',
  });
  if (build.status !== 0) process.exit(build.status ?? 1);

  const { createAcpProvider } = await import('../../nexus-provider-acp/dist/index.js');
  const fixture = resolve(root, 'crates/nexus-agent-host/tests/fixtures/mock_acp_workflow.py');
  const workspace = mkdtempSync(join(tmpdir(), 'nexus-acp-ws-'));
  writeAgentHostConfig(home, fixture, workspace);

  let capturedAdmittedRecipe = null;
  const providers = createAcpProvider();
  const core = binding.open(
    JSON.stringify({ user_home: home, access: 'engine_owner', allow_uninitialized: false }),
    {
      call: async (...args) => {
        const req = unpackCallbackPayload(...args);
        if (req.payload?.recipe) capturedAdmittedRecipe = req.payload.recipe;
        return JSON.stringify(await providers.call(req));
      },
      next: async (...args) => {
        const req = unpackCallbackPayload(...args);
        return JSON.stringify(
          await providers.next(req.operation_id, req.max_events, req.max_bytes),
        );
      },
    },
  );

  const fakeRecipe = {
    provider_id: 'mock-acp',
    recipe_generation: '999999',
    executable: '/tmp/evil',
    args: [],
    env: {},
    cwd: '/tmp',
  };
  const fakeProbeErr = await core
    .providerCall(
      encode({
        request_id: 'fake-recipe',
        method: 'probe',
        deadline_ms: 30_000,
        payload: { provider_id: 'mock-acp', recipe: fakeRecipe },
      }),
    )
    .then(() => null)
    .catch((error) => String(error));
  if (!fakeProbeErr || !/recipe rejected|invalid_input|policy/i.test(fakeProbeErr)) {
    console.error('expected rejection for caller-supplied recipe', fakeProbeErr);
    process.exit(1);
  }

  const admitProbeReply = decode(
    await core.providerCall(
      encode({
        request_id: 'admit-probe',
        method: 'probe',
        deadline_ms: 30_000,
        payload: tsProbePayload(),
      }),
    ),
  );
  if (!admitProbeReply.ok) {
    console.error('admitting probe failed', admitProbeReply);
    process.exit(1);
  }

  if (!capturedAdmittedRecipe) {
    console.error('callback never received Rust-admitted recipe');
    process.exit(1);
  }
  if (capturedAdmittedRecipe.recipe_generation === '999999') {
    console.error('callback used caller fake recipe generation');
    process.exit(1);
  }
  if (!capturedAdmittedRecipe.executable?.startsWith('/')) {
    console.error('admitted executable not canonical absolute', capturedAdmittedRecipe);
    process.exit(1);
  }

  const accessJson = JSON.stringify({ user_home: home, access: 'engine_owner', allow_uninitialized: false });
  const providerCallbacks = {
    call: async (...args) => {
      const req = unpackCallbackPayload(...args);
      if (req.payload?.recipe) capturedAdmittedRecipe = req.payload.recipe;
      return JSON.stringify(await providers.call(req));
    },
    next: async (...args) => {
      const req = unpackCallbackPayload(...args);
      return JSON.stringify(
        await providers.next(req.operation_id, req.max_events, req.max_bytes),
      );
    },
  };
  await runAcpLifecycleSession(core, {
    adapter: 'ts-acp',
    home,
    accessJson,
    providers: providerCallbacks,
    sdk: '@agentclientprotocol/sdk@1.4.0',
    admittedMeta: {
      fixture,
      rust_admission_boundary: 'exercised_via_admitting_provider_port',
      admitted_provider_id: capturedAdmittedRecipe.provider_id,
      admitted_generation: capturedAdmittedRecipe.recipe_generation,
      admitted_executable: capturedAdmittedRecipe.executable,
      admitted_env_keys: Object.keys(capturedAdmittedRecipe.env ?? {}).sort(),
    },
  });
}

if (adapter === 'rust-acp' && caseName === 'lifecycle') {
  await runRustAcpLifecycleProof();
  console.log('proof-provider rust-acp lifecycle passed');
  process.exit(0);
}

if (adapter === 'ts-acp' && caseName === 'lifecycle') {
  await runTsAcpLifecycleProof();
  console.log('proof-provider ts-acp lifecycle passed');
  process.exit(0);
}

if (caseName !== 'wire' || adapter !== 'wire') {
  console.error(`unsupported proof profile adapter=${adapter} case=${caseName}`);
  process.exit(2);
}

const core = binding.open(
  JSON.stringify({ user_home: home, access: 'engine_owner', allow_uninitialized: false }),
);
await runWireProof(core);
await core.close();
console.log('proof-provider passed');
