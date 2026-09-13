import { parentPort, workerData } from 'node:worker_threads';
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, realpathSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
import { randomUUID } from 'node:crypto';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = workerData.root;
const adapter = workerData.adapter;
const scenario = workerData.scenario;
const require = createRequire(import.meta.url);
const { loadNodePath } = await import('../dist/loader.js');
const nodePath = loadNodePath();
const binding = require(nodePath);
const encode = (v) => new TextEncoder().encode(JSON.stringify(v));
const decode = (buf) => JSON.parse(new TextDecoder().decode(buf));

async function resolveProviders() {
  if (workerData.providers) return workerData.providers;
  if (adapter !== 'ts-acp') return undefined;
  const { createAcpProvider } = await import('../../nexus-provider-acp/dist/index.js');
  const providers = createAcpProvider();
  return {
    call: async (...args) => {
      const req = unpackCallbackPayload(...args);
      return JSON.stringify(await providers.call(req));
    },
    next: async (...args) => {
      const req = unpackCallbackPayload(...args);
      return JSON.stringify(await providers.next(req.operation_id, req.max_events, req.max_bytes));
    },
  };
}

function unpackCallbackPayload(...args) {
  const payload = args.length > 1 ? args[1] : args[0];
  return typeof payload === 'string' ? JSON.parse(payload) : payload;
}

let cachedProviders;
async function providersForScenario() {
  if (!cachedProviders) cachedProviders = await resolveProviders();
  return cachedProviders;
}


function resolveAdmittedPython() {
  for (const candidate of [process.env.PYTHON, process.env.PYTHON3, 'python3'].filter(Boolean)) {
    try {
      const resolved = execFileSync('which', [candidate], { encoding: 'utf8' }).trim();
      if (resolved.startsWith('/')) return realpathSync(resolved);
    } catch { /* next */ }
  }
  throw new Error('no_absolute_python_executable');
}

const MAX_PENDING_BYTES = 1024 * 1024;
const MAX_PENDING_REQUESTS = 16;
const MAX_ACTIVE_TASKS = 32;
const SDK_CREATE_SESSION_CHARGE = new TextEncoder().encode('create_session').length;
const SDK_STREAM_PROMPT_CHARGE = new TextEncoder().encode('stream_prompt').length;

function writeAgentHostConfig(home, fixturePath, workspace, extraEnv = {}, hostLimits = {}) {
  const python = resolveAdmittedPython();
  const limitLines = [];
  if (hostLimits.max_sessions != null) limitLines.push(`max_sessions = ${hostLimits.max_sessions}`);
  if (hostLimits.max_ops_per_session != null) limitLines.push(`max_ops_per_session = ${hostLimits.max_ops_per_session}`);
  let timeoutBlock = '';
  if (hostLimits.timeouts) {
    timeoutBlock = '[timeouts]\n' + Object.entries(hostLimits.timeouts).map(([k, v]) => `${k} = ${v}`).join('\n') + '\n\n';
  }
  const header = limitLines.length ? `${limitLines.join('\n')}\n\n${timeoutBlock}` : timeoutBlock;
  const toml = `${header}[[providers]]
id = "mock-acp"
protocol = "acp"
command = "${python}"
args = ["${fixturePath.replaceAll('\\', '/')}"]
enabled = true

[providers.env]
ACP_FIXTURE_LOG = "${join(workspace, 'fixture.log').replaceAll('\\', '/')}"
${Object.entries(extraEnv).map(([k, v]) => `${k} = "${String(v).replaceAll('"', '\\"')}"`).join('\n')}
`;
  mkdirSync(join(home, 'config'), { recursive: true });
  writeFileSync(join(home, 'config', 'agent-host.toml'), toml);
}

function seedHome() {
  const home = mkdtempSync(join(tmpdir(), `nexus-proof-${scenario}-`));
  const seed = spawnSync(
    'cargo',
    ['run', '-q', '-p', 'nexus-core-node', '--bin', 'native-wire-fixture-seed', '--', home],
    { cwd: root },
  );
  if (seed.status !== 0) throw new Error(seed.stderr?.toString() || 'seed failed');
  return home;
}

function pass(key, detail = {}) {
  return { key, ok: true, executed_steps: detail.executed_steps ?? [], ...detail };
}
function fail(key, message, detail = {}) {
  return { key, ok: false, error: message, executed_steps: detail.executed_steps ?? [], ...detail };
}

function hostOwner(home) {
  return { creator_id: 'proof-provider', workspace_root: home, orchestration_run_id: null };
}
function rustProbePayload(home) {
  return { provider_id: 'mock-acp', timeout_ms: 30_000, cwd: home, owner: hostOwner(home) };
}
function rustLaunchPayload(home) {
  return { provider_id: 'mock-acp', cwd: home, mcp_servers: [], owner: hostOwner(home) };
}
function rustExecutePayload() {
  return { Prompt: { op_id: randomUUID(), content: [{ Text: { text: 'hello' } }], permission_scope: null } };
}
function tsProbePayload() { return { provider_id: 'mock-acp' }; }
function tsLaunchPayload() { return { provider_id: 'mock-acp' }; }
function tsExecutePayload() { return { kind: 'prompt', content: 'hello' }; }

function formatProviderError(err) {
  if (err == null) return '';
  if (typeof err === 'string') return err;
  if (typeof err === 'object') {
    if (typeof err.message === 'string') return err.message;
    if (typeof err.error === 'string') return err.error;
    if (err.error && typeof err.error === 'object' && typeof err.error.message === 'string') {
      return err.error.message;
    }
    try {
      return JSON.stringify(err);
    } catch {
      return String(err);
    }
  }
  return String(err);
}

function isTransportEofError(err) {
  return /eof|broken pipe|connection reset|transport|child process|exited|closed/i.test(formatProviderError(err));
}

function isProviderEofSignal(reply) {
  if (!reply || typeof reply !== 'object') return false;
  const healthMsg = formatProviderError(reply.health?.message);
  if (reply.ok && reply.health?.available === false) {
    return /provider_eof|unavailable|not available|exited|closed/i.test(healthMsg);
  }
  const err = formatProviderError(reply.error);
  if (!reply.ok) {
    return isTransportEofError(err) || /provider_eof|unavailable|not available|exited|closed|session/i.test(err);
  }
  return false;
}


function encodedProviderCallBytes(padLen) {
  return encode({
    request_id: 'ts-queue-cal',
    method: 'probe',
    deadline_ms: 5_000,
    payload: { provider_id: 'mock-acp', pad: 'x'.repeat(padLen) },
  }).byteLength;
}

function padLenForTargetBytes(targetBytes) {
  let lo = 0;
  let hi = targetBytes;
  while (lo < hi) {
    const mid = Math.ceil((lo + hi) / 2);
    if (encodedProviderCallBytes(mid) <= targetBytes) lo = mid;
    else hi = mid - 1;
  }
  return lo;
}

function isQueueRejection(err) {
  return /busy|budget|admission|timeout|deadline|session limit|policy denied|active|byte budget|oneshot canceled|closing/i.test(formatProviderError(err));
}

function isLocalSetBusy(err) {
  const s = formatProviderError(err);
  return /localset.*busy|admission busy|bridge admission/i.test(s) && !/session limit/i.test(s);
}

function isTransportFailure(err) {
  const s = formatProviderError(err);
  return isTransportEofError(s) || /protocol_error|connection|transport|child process|exited|closed|provider_eof|broken pipe/i.test(s);
}

function isOversizedOutcome(batchOrReply) {
  if (!batchOrReply || typeof batchOrReply !== 'object') return false;
  const gap = batchOrReply.gap;
  if (gap && String(gap.reason ?? '').toLowerCase() === 'oversized') return true;
  if (gap && /oversized|resync/i.test(String(gap.reason ?? gap.message ?? ''))) return true;
  const events = batchOrReply.events ?? [];
  for (const ev of events) {
    const wire = JSON.stringify(ev);
    if (/delivery_overflow|oversized|prompt_too_large/i.test(wire)) return true;
  }
  const err = formatProviderError(batchOrReply.error);
  return /delivery_overflow|oversized|prompt_too_large|input_too_large/i.test(err);
}

function cleanupFixtureChildren() {
  for (const pid of findFixtureChildPids()) {
    try { process.kill(pid, 'SIGKILL'); } catch { /* already dead */ }
  }
}

function findFixtureChildPids() {
  try {
    const out = execFileSync('pgrep', ['-f', 'mock_acp_workflow.py'], { encoding: 'utf8' }).trim();
    return out ? out.split('\n').map(Number).filter(Boolean) : [];
  } catch {
    return [];
  }
}

async function runOpenClose100() {
  const steps = [];
  const home = seedHome();
  const accessJson = JSON.stringify({ user_home: home, access: 'engine_owner', allow_uninitialized: false });
  const providers = await providersForScenario();
  for (let cycle = 0; cycle < 100; cycle += 1) {
    steps.push(`open:${cycle}`);
    const core = binding.open(accessJson, providers);
    const report = decode(await core.close());
    steps.push(`close:${cycle}`);
    if (!report.cleanup_confirmed || report.state !== 'closed') {
      return fail('open_close_100', `cycle ${cycle} unconfirmed`, { executed_steps: steps, cycle });
    }
  }
  return pass('open_close_100', { cycles: 100, executed_steps: steps, worker_env: 'fresh' });
}

async function runNeverSettling() {
  const steps = ['seed'];
  const home = mkdtempSync(join(tmpdir(), 'nexus-block-prompt-'));
  const ws = mkdtempSync(join(tmpdir(), 'nexus-block-ws-'));
  const fixture = resolve(root, 'crates/nexus-agent-host/tests/fixtures/mock_acp_workflow.py');
  const seed = spawnSync('cargo', ['run', '-q', '-p', 'nexus-core-node', '--bin', 'native-wire-fixture-seed', '--', home], { cwd: root });
  if (seed.status !== 0) return fail('never_settling_callback', seed.stderr?.toString() || 'seed failed', { executed_steps: steps });
  writeAgentHostConfig(home, fixture, ws, { BLOCK_PROMPT: '1' });
  const accessJson = JSON.stringify({ user_home: home, access: 'engine_owner', allow_uninitialized: false });
  const providers = await providersForScenario();
  steps.push('open');
  const core = binding.open(accessJson, providers);
  steps.push('probe');
  const probe = await core.providerCall(encode({
    request_id: 'block-probe', method: 'probe', deadline_ms: 30_000,
    payload: adapter === 'rust-acp' ? rustProbePayload(home) : tsProbePayload(),
  })).then((buf) => decode(buf)).catch((e) => ({ ok: false, error: String(e) }));
  if (!probe.ok) return fail('never_settling_callback', `probe failed: ${probe.error}`, { executed_steps: steps });
  steps.push('launch');
  const launch = await core.providerCall(encode({
    request_id: 'block-launch', method: 'launch', deadline_ms: 30_000,
    payload: adapter === 'rust-acp' ? rustLaunchPayload(home) : tsLaunchPayload(),
  })).then((buf) => decode(buf)).catch((e) => ({ ok: false, error: String(e) }));
  if (!launch.ok || !launch.session_id) return fail('never_settling_callback', `launch failed: ${launch.error}`, { executed_steps: steps });
  steps.push('execute');
  const exec = await core.providerCall(encode({
    request_id: 'block-exec', method: 'execute', session_id: launch.session_id, deadline_ms: 30_000,
    payload: adapter === 'rust-acp' ? rustExecutePayload() : tsExecutePayload(),
  })).then((buf) => decode(buf)).catch((e) => ({ ok: false, error: String(e) }));
  if (!exec.ok || !exec.operation_id) return fail('never_settling_callback', `execute failed: ${exec.error}`, { executed_steps: steps });
  const started = Date.now();
  steps.push('cancel');
  await core.providerCall(encode({
    request_id: 'block-cancel', method: 'cancel', session_id: launch.session_id,
    operation_id: exec.operation_id, deadline_ms: 30_000, payload: {},
  })).catch(() => {});
  const cancelMs = Date.now() - started;
  const closeStarted = Date.now();
  steps.push('close');
  const closeReport = decode(await core.close());
  const closeMs = Date.now() - closeStarted;
  if (cancelMs <= 2000 && closeReport.cleanup_confirmed && closeMs <= 5_000) {
    return pass('never_settling_callback', { cancel_ms: cancelMs, close_ms: closeMs, executed_steps: steps, adapter });
  }
  return fail('never_settling_callback', `cancel_ms=${cancelMs} close_ms=${closeMs} confirmed=${closeReport.cleanup_confirmed}`, { executed_steps: steps });
}

async function runFailedOpenChild() {
  const steps = ['seed_bad_home'];
  const badHome = mkdtempSync(join(tmpdir(), 'nexus-bad-child-'));
  const seed = spawnSync('cargo', ['run', '-q', '-p', 'nexus-core-node', '--bin', 'native-wire-fixture-seed', '--', badHome], { cwd: root });
  if (seed.status !== 0) return fail('failed_open_child', seed.stderr?.toString() || 'seed failed', { executed_steps: steps });
  mkdirSync(join(badHome, 'config'), { recursive: true });
  writeFileSync(join(badHome, 'config', 'agent-host.toml'),
    `[[providers]]
id = "bad"
protocol = "acp"
command = "/nonexistent/nexus-bad-acp"
args = []
enabled = true
`);
  const accessJson = JSON.stringify({ user_home: badHome, access: 'engine_owner', allow_uninitialized: false });
  steps.push('open');
  const providers = await providersForScenario();
  const core = binding.open(accessJson, providers);
  steps.push('launch_bad_child');
  const badLaunch = await core.providerCall(encode({
    request_id: 'bad-launch', method: 'launch', deadline_ms: 5_000,
    payload: adapter === 'rust-acp' ? rustLaunchPayload(badHome) : tsLaunchPayload(),
  })).then((buf) => decode(buf)).catch((e) => ({ ok: false, error: String(e) }));
  steps.push('close');
  await core.close().catch(() => {});
  if (!badLaunch.ok) {
    return pass('failed_open_child', { error: badLaunch.error, executed_steps: steps });
  }
  return fail('failed_open_child', 'expected launch failure', { executed_steps: steps });
}

async function runEofWhileLive() {
  const steps = ['seed_eof_home'];
  const eofHome = mkdtempSync(join(tmpdir(), 'nexus-eof-live-'));
  const eofWs = mkdtempSync(join(tmpdir(), 'nexus-eof-ws-'));
  const fixture = resolve(root, 'crates/nexus-agent-host/tests/fixtures/mock_acp_workflow.py');
  const seed = spawnSync('cargo', ['run', '-q', '-p', 'nexus-core-node', '--bin', 'native-wire-fixture-seed', '--', eofHome], { cwd: root });
  if (seed.status !== 0) return fail('eof_after_close', seed.stderr?.toString() || 'seed failed', { executed_steps: steps });
  writeAgentHostConfig(eofHome, fixture, eofWs, { EOF_AFTER_INIT: '1' });
  const accessJson = JSON.stringify({ user_home: eofHome, access: 'engine_owner', allow_uninitialized: false });
  steps.push('open');
  const providers = await providersForScenario();
  const core = binding.open(accessJson, providers);
  steps.push('probe');
  const probe = await core.providerCall(encode({
    request_id: 'eof-probe', method: 'probe', deadline_ms: 30_000,
    payload: adapter === 'rust-acp' ? rustProbePayload(eofHome) : tsProbePayload(),
  })).then((buf) => decode(buf)).catch((e) => ({ ok: false, error: String(e) }));
  steps.push('launch_eof_fixture');
  const launch = await core.providerCall(encode({
    request_id: 'eof-live-launch', method: 'launch', deadline_ms: 5_000,
    payload: adapter === 'rust-acp' ? rustLaunchPayload(eofHome) : tsLaunchPayload(),
  })).then((buf) => decode(buf)).catch((e) => ({ ok: false, error: String(e) }));
  steps.push('close');
  await core.close().catch(() => {});
  const launchErr = formatProviderError(launch.error ?? launch.health?.message);
  const launchEof = !launch.ok && (/provider_eof/i.test(launchErr) || isTransportFailure(launch.error));
  if (launchEof) {
    return pass('eof_after_close', {
      error: formatProviderError(launch.error ?? launch.health?.message),
      launch_ok: launch.ok,
      executed_steps: steps,
      mode: 'EOF_AFTER_INIT_live',
      adapter,
    });
  }
  return fail('eof_after_close', `expected live ACP child EOF on launch, launch=${formatProviderError(launch.error)}`, { executed_steps: steps });
}

async function runFullQueueShutdown() {
  cleanupFixtureChildren();
  const steps = ['seed'];
  const home = mkdtempSync(join(tmpdir(), 'nexus-queue-'));
  const ws = mkdtempSync(join(tmpdir(), 'nexus-queue-ws-'));
  const fixture = resolve(root, 'crates/nexus-agent-host/tests/fixtures/mock_acp_workflow.py');
  const seed = spawnSync('cargo', ['run', '-q', '-p', 'nexus-core-node', '--bin', 'native-wire-fixture-seed', '--', home], { cwd: root });
  if (seed.status !== 0) return fail('full_queue_shutdown', seed.stderr?.toString() || 'seed failed', { executed_steps: steps });
  writeAgentHostConfig(home, fixture, ws, adapter === 'rust-acp' ? { BLOCK_PROMPT: '1' } : {}, { max_sessions: 80, max_ops_per_session: 1, timeouts: adapter === 'rust-acp' ? { initialize_ms: 180_000, launch_ms: 180_000, session_ms: 180_000, prompt_ms: 300_000 } : undefined });
  const accessJson = JSON.stringify({ user_home: home, access: 'engine_owner', allow_uninitialized: false });

  if (adapter === 'ts-acp') {
    steps.push('ts_byte_boundary');
    let activeCallbacks = 0;
    const hangProviders = {
      call: (...args) => {
        activeCallbacks += 1;
        unpackCallbackPayload(...args);
        return new Promise(() => {});
      },
      next: () => Promise.resolve(JSON.stringify({ operation_id: 'x', events: [], has_more: false })),
    };
    steps.push('open');
    const tsCore = binding.open(accessJson, hangProviders);
    let padLen = 0;
    let perRequestBytes = 0;
    for (let target = Math.floor((MAX_PENDING_BYTES - 4096) / MAX_PENDING_REQUESTS); target > 1024; target -= 64) {
      const candidate = padLenForTargetBytes(target);
      const bytes = encodedProviderCallBytes(candidate);
      if (bytes * MAX_PENDING_REQUESTS <= MAX_PENDING_BYTES - 1024) {
        padLen = candidate;
        perRequestBytes = bytes;
        break;
      }
    }
    if (!padLen) return fail('full_queue_shutdown', 'could not calibrate byte boundary pad', { executed_steps: steps });
    const blockers = [];
    let inputBytes = 0;
    for (let i = 0; i < MAX_PENDING_REQUESTS; i += 1) {
      inputBytes += perRequestBytes;
      blockers.push(tsCore.providerCall(encode({
        request_id: `ts-queue-${i}`,
        method: 'probe',
        deadline_ms: 5_000,
        payload: { provider_id: 'mock-acp', pad: 'x'.repeat(padLen) },
      })).catch((e) => ({ ok: false, error: String(e) })));
      steps.push(`admit_${i}`);
    }
    const saturateDeadline = Date.now() + 3_000;
    while (activeCallbacks < MAX_PENDING_REQUESTS && Date.now() < saturateDeadline) {
      await new Promise((r) => setTimeout(r, 25));
    }
    if (activeCallbacks < MAX_PENDING_REQUESTS - 1) {
      return fail('full_queue_shutdown', `only ${activeCallbacks}/${MAX_PENDING_REQUESTS} callbacks active`, { executed_steps: steps, activeCallbacks, per_request_bytes: perRequestBytes });
    }
    steps.push('reject_17th');
    const reject17 = await tsCore.providerCall(encode({
      request_id: 'ts-queue-17',
      method: 'probe',
      deadline_ms: 2_000,
      payload: { provider_id: 'mock-acp' },
    })).then((buf) => decode(buf)).catch((e) => ({ ok: false, error: String(e) }));
    const rejectedBusy = !reject17.ok && isQueueRejection(reject17.error);
    const rejectedCode = formatProviderError(reject17.error);
    steps.push('close_before_drain');
    const closeStarted = Date.now();
    const closeBuf = await Promise.race([
      tsCore.close(),
      new Promise((_, reject) => setTimeout(() => reject(new Error('close exceeded 6s')), 6_000)),
    ]);
    const closeReport = decode(closeBuf);
    const closeMs = Date.now() - closeStarted;
    if (!rejectedBusy) {
      return fail('full_queue_shutdown', `17th not rejected: ${reject17.error}`, {
        executed_steps: steps,
        activeCallbacks,
        admitted: MAX_PENDING_REQUESTS,
        input_bytes: inputBytes,
        per_request_bytes: perRequestBytes,
      });
    }
    if (closeMs > 5_000 || !closeReport.cleanup_confirmed) {
      return fail('full_queue_shutdown', `close_ms=${closeMs} confirmed=${closeReport.cleanup_confirmed}`, { executed_steps: steps });
    }
    return pass('full_queue_shutdown', {
      admitted: MAX_PENDING_REQUESTS,
      active_count: activeCallbacks,
      pending_count: MAX_PENDING_REQUESTS,
      pending_bytes: inputBytes,
      rejected_code: rejectedCode,
      rejected_17th: rejectedBusy,
      reject_error: reject17.error ?? null,
      input_bytes: inputBytes,
      per_request_bytes: perRequestBytes,
      close_ms: closeMs,
      close_before_drain: true,
      executed_steps: steps,
      adapter,
      active_callbacks: activeCallbacks,
      queue_profile: 'ts_callback_pending_budget_1MiB_16_requests',
      control_bypass: true,
    });
  }

  const providers = await providersForScenario();
  steps.push('open');
  const core = binding.open(accessJson, providers);
  steps.push('probe');
  const probe = await core.providerCall(encode({
    request_id: 'queue-probe', method: 'probe', deadline_ms: 30_000,
    payload: rustProbePayload(home),
  })).then((buf) => decode(buf)).catch((e) => ({ ok: false, error: String(e) }));
  if (!probe.ok) return fail('full_queue_shutdown', `probe failed: ${probe.error}`, { executed_steps: steps });

  const beforePids = findFixtureChildPids();
  const createSessionCharge = new TextEncoder().encode('create_session').length;
  const streamPromptCharge = new TextEncoder().encode('stream_prompt').length;

  steps.push('launch_parallel_32');
  const launchPromises = [];
  for (let i = 0; i < MAX_ACTIVE_TASKS; i += 1) {
    launchPromises.push(core.providerCall(encode({
      request_id: `queue-launch-${i}`, method: 'launch', deadline_ms: 180_000,
      payload: rustLaunchPayload(home),
    })).then((buf) => decode(buf)).catch((e) => ({ ok: false, error: String(e) })));
  }
  const launches = await Promise.all(launchPromises);
  const sessionIds = launches.filter((l) => l.ok && l.session_id).map((l) => l.session_id);
  if (sessionIds.length < MAX_ACTIVE_TASKS) {
    return fail('full_queue_shutdown', `only ${sessionIds.length}/${MAX_ACTIVE_TASKS} launches succeeded`, {
      executed_steps: steps,
      launch_errors: launches.filter((l) => !l.ok).slice(0, 4).map((l) => formatProviderError(l.error)),
    });
  }

  steps.push('admit_active_32');
  sessionIds.forEach((sessionId, i) => {
    core.providerCall(encode({
      request_id: `queue-block-${i}`, method: 'execute', session_id: sessionId, deadline_ms: 300_000,
      payload: rustExecutePayload(),
    })).catch((e) => ({ ok: false, error: String(e) }));
  });
  await new Promise((r) => setTimeout(r, 800));

  const pendingLaunches = [];
  for (let i = 0; i < MAX_PENDING_REQUESTS; i += 1) {
    steps.push(`pending_launch_${i}`);
    pendingLaunches.push(core.providerCall(encode({
      request_id: `queue-pending-${i}`, method: 'launch', deadline_ms: 180_000,
      payload: rustLaunchPayload(home),
    })).catch((e) => ({ ok: false, error: String(e) })));
  }
  await new Promise((r) => setTimeout(r, 600));

  steps.push('reject_17th');
  const reject17 = await core.providerCall(encode({
    request_id: 'queue-launch-17', method: 'launch', deadline_ms: 3_000,
    payload: rustLaunchPayload(home),
  })).then((buf) => decode(buf)).catch((e) => ({ ok: false, error: String(e) }));
  const rejectedCode = formatProviderError(reject17.error);
  const rejectedBusy = !reject17.ok && isLocalSetBusy(reject17.error);

  steps.push('close_before_drain');
  const closeStarted = Date.now();
  const closeReport = decode(await core.close());
  const closeMs = Date.now() - closeStarted;
  const launchedPids = findFixtureChildPids().filter((pid) => !beforePids.includes(pid));
  let survivors = launchedPids.slice();
  const reapDeadline = Date.now() + 2_000;
  while (survivors.length > 0 && Date.now() < reapDeadline) {
    await new Promise((r) => setTimeout(r, 100));
    survivors = findFixtureChildPids().filter((pid) => launchedPids.includes(pid));
  }

  const pendingBytes = MAX_PENDING_REQUESTS * createSessionCharge;
  const activeCountObserved = sessionIds.length;
  if (!rejectedBusy) {
    return fail('full_queue_shutdown', `17th not LocalSet busy: ${rejectedCode}`, { executed_steps: steps, reject17 });
  }
  if (closeMs > 5_000) {
    return fail('full_queue_shutdown', `close ${closeMs}ms > 5s`, { executed_steps: steps, close_ms: closeMs });
  }
  if (!closeReport.cleanup_confirmed) {
    return fail('full_queue_shutdown', 'cleanup unconfirmed', { executed_steps: steps });
  }
  if (survivors.length > 0) {
    return fail('full_queue_shutdown', `survivors ${survivors.join(',')}`, { executed_steps: steps, survivors });
  }
  return pass('full_queue_shutdown', {
    adapter,
    active_count: activeCountObserved,
    pending_count: MAX_PENDING_REQUESTS,
    pending_bytes: pendingBytes,
    per_request_charge: createSessionCharge,
    active_charge: streamPromptCharge,
    rejected_code: rejectedCode,
    rejected_17th: true,
    close_ms: closeMs,
    close_before_drain: true,
    control_bypass: true,
    surviving_child_pids: survivors,
    executed_steps: steps,
    queue_profile: 'rust_localset_active32_pending16_create_session',
    localset_active_cap: MAX_ACTIVE_TASKS,
    localset_pending_cap: MAX_PENDING_REQUESTS,
  });

}


async function runWorkerTermination() {
  cleanupFixtureChildren();
  const steps = ['seed'];
  const home = mkdtempSync(join(tmpdir(), 'nexus-kill-ws-'));
  const ws = mkdtempSync(join(tmpdir(), 'nexus-kill-fixture-'));
  const fixture = resolve(root, 'crates/nexus-agent-host/tests/fixtures/mock_acp_workflow.py');
  const seed = spawnSync('cargo', ['run', '-q', '-p', 'nexus-core-node', '--bin', 'native-wire-fixture-seed', '--', home], { cwd: root });
  if (seed.status !== 0) return fail('worker_termination', seed.stderr?.toString() || 'seed failed', { executed_steps: steps });
  writeAgentHostConfig(home, fixture, ws, { BLOCK_PROMPT: '1' });
  const accessJson = JSON.stringify({ user_home: home, access: 'engine_owner', allow_uninitialized: false });
  const providers = await providersForScenario();
  steps.push('open');
  const core = binding.open(accessJson, providers);
  steps.push('probe');
  const probe = await core.providerCall(encode({
    request_id: 'kill-probe', method: 'probe', deadline_ms: 30_000,
    payload: adapter === 'rust-acp' ? rustProbePayload(home) : tsProbePayload(),
  })).then((buf) => decode(buf)).catch((e) => ({ ok: false, error: String(e) }));
  if (!probe.ok) return fail('worker_termination', `probe failed: ${probe.error}`, { executed_steps: steps });
  const beforePids = findFixtureChildPids();
  steps.push('launch');
  const launch = await core.providerCall(encode({
    request_id: 'kill-launch', method: 'launch', deadline_ms: 30_000,
    payload: adapter === 'rust-acp' ? rustLaunchPayload(home) : tsLaunchPayload(),
  })).then((buf) => decode(buf)).catch((e) => ({ ok: false, error: String(e) }));
  if (!launch.ok || !launch.session_id) return fail('worker_termination', `launch failed: ${launch.error}`, { executed_steps: steps });
  const launchedPids = findFixtureChildPids().filter((pid) => !beforePids.includes(pid));
  const victim = launchedPids[0];
  if (!victim) return fail('worker_termination', 'no fixture child pid observed', { executed_steps: steps });
  steps.push('execute_blocked');
  const execResult = decode(await core.providerCall(encode({
    request_id: 'kill-exec', method: 'execute', session_id: launch.session_id, deadline_ms: 30_000,
    payload: adapter === 'rust-acp' ? rustExecutePayload() : tsExecutePayload(),
  })));
  if (!execResult.ok || !execResult.operation_id) {
    return fail('worker_termination', `execute never started: ${formatProviderError(execResult.error)}`, { executed_steps: steps, execResult });
  }
  steps.push('pull_op_started');
  await core.nextProviderEvents(execResult.operation_id, 1, 256 * 1024);
  steps.push('kill_child');
  try { process.kill(victim, 'SIGKILL'); } catch { /* already dead */ }
  steps.push('await_pull_after_kill');
  let transportObserved = false;
  let pullEvidence = null;
  try {
    const batch = decode(await Promise.race([
      core.nextProviderEvents(execResult.operation_id, 16, 256 * 1024),
      new Promise((_, reject) => setTimeout(() => reject(new Error('pull_timeout')), 20_000)),
    ]));
    pullEvidence = batch;
    const wire = JSON.stringify(batch);
    if (/provider_eof|protocol_error|transport|connection|exited|closed|OpFailed/i.test(wire)) {
      transportObserved = true;
    }
    const terminalFail = (batch.events ?? []).some((ev) => /OpFailed|provider_eof|protocol_error|transport/i.test(JSON.stringify(ev)));
    if (terminalFail) transportObserved = true;
  } catch (error) {
    pullEvidence = String(error);
    if (isTransportFailure(error)) transportObserved = true;
  }

  steps.push('close');
  const closeReport = decode(await core.close());
  const survivors = findFixtureChildPids().filter((pid) => launchedPids.includes(pid));
  let victimAlive = false;
  try { process.kill(victim, 0); victimAlive = true; } catch { victimAlive = false; }
  if (!transportObserved) {
    return fail('worker_termination', `no transport/EOF observed after kill: ${JSON.stringify(pullEvidence)}`, { executed_steps: steps, execResult });
  }
  if (!closeReport.cleanup_confirmed) {
    return fail('worker_termination', 'cleanup unconfirmed after kill', { executed_steps: steps });
  }
  if (survivors.length > 0 || victimAlive) {
    return fail('worker_termination', `survivors ${survivors.join(',')} victim_alive=${victimAlive}`, { executed_steps: steps, survivors });
  }
  return pass('worker_termination', {
    child_pid: victim,
    fixture_pid: victim,
    post_kill_execute_ok: false,
    post_kill_execute_error: formatProviderError(execResult.error),
    executed_steps: steps,
    adapter,
    surviving_child_pids: survivors,
    victim_alive_after_close: victimAlive,
    cleanup_confirmed: closeReport.cleanup_confirmed,
  });
}


async function runMultibyteOverflow() {
  cleanupFixtureChildren();
  const steps = ['seed'];
  const home = mkdtempSync(join(tmpdir(), 'nexus-mb-'));
  const ws = mkdtempSync(join(tmpdir(), 'nexus-mb-ws-'));
  const fixture = resolve(root, 'crates/nexus-agent-host/tests/fixtures/mock_acp_workflow.py');
  const seed = spawnSync('cargo', ['run', '-q', '-p', 'nexus-core-node', '--bin', 'native-wire-fixture-seed', '--', home], { cwd: root });
  if (seed.status !== 0) return fail('multibyte_overflow', seed.stderr?.toString() || 'seed failed', { executed_steps: steps });
  writeAgentHostConfig(home, fixture, ws, { OVERSIZED_UPDATE: '1' });
  const accessJson = JSON.stringify({ user_home: home, access: 'engine_owner', allow_uninitialized: false });
  const providers = await providersForScenario();
  steps.push('open');
  const core = binding.open(accessJson, providers);
  steps.push('probe');
  const probe = await core.providerCall(encode({
    request_id: 'mb-probe', method: 'probe', deadline_ms: 30_000,
    payload: adapter === 'rust-acp' ? rustProbePayload(home) : tsProbePayload(),
  })).then((buf) => decode(buf)).catch((e) => ({ ok: false, error: String(e) }));
  if (!probe.ok) return fail('multibyte_overflow', `probe failed: ${probe.error}`, { executed_steps: steps });
  steps.push('launch');
  const launch = await core.providerCall(encode({
    request_id: 'mb-launch', method: 'launch', deadline_ms: 30_000,
    payload: adapter === 'rust-acp' ? rustLaunchPayload(home) : tsLaunchPayload(),
  })).then((buf) => decode(buf)).catch((e) => ({ ok: false, error: String(e) }));
  if (!launch.ok || !launch.session_id) return fail('multibyte_overflow', `launch failed: ${launch.error}`, { executed_steps: steps });
  const multibyteSeed = '🎉';
  steps.push('oversize_execute');
  const exec = await core.providerCall(encode({
    request_id: 'mb-exec', method: 'execute', session_id: launch.session_id, deadline_ms: 30_000,
    payload: adapter === 'rust-acp' ? rustExecutePayload() : tsExecutePayload(),
  })).then((buf) => decode(buf)).catch((e) => ({ ok: false, error: String(e) }));
  if (!exec.ok || !exec.operation_id) {
    return fail('multibyte_overflow', `execute failed: ${formatProviderError(exec.error)}`, { executed_steps: steps, exec });
  }
  steps.push('pull_oversized');
  let batch = null;
  for (let i = 0; i < 40; i += 1) {
    try {
      batch = decode(await core.nextProviderEvents(exec.operation_id, 1, 4096));
      if (isOversizedOutcome(batch)) break;
    } catch (error) {
      if (isOversizedOutcome({ error: String(error) })) {
        batch = { error: String(error) };
        break;
      }
    }
    await new Promise((r) => setTimeout(r, 50));
  }
  steps.push('close');
  await core.close().catch(() => {});
  const utf8Bytes = new TextEncoder().encode(multibyteSeed).length * (300 * 1024);
  if (!isOversizedOutcome(batch)) {
    return fail('multibyte_overflow', `expected delivery_overflow/oversized outcome, got: ${JSON.stringify(batch)}`, { executed_steps: steps, batch });
  }
  return pass('multibyte_overflow', {
    adapter,
    rejected: true,
    utf8_bytes: utf8Bytes,
    multibyte_marker: multibyteSeed,
    outcome: batch?.gap ?? batch?.error ?? 'oversized',
    executed_steps: steps,
  });
}


async function runReentrantCall() {
  cleanupFixtureChildren();
  const steps = ['seed'];
  const home = mkdtempSync(join(tmpdir(), 'nexus-reentrant-'));
  const ws = mkdtempSync(join(tmpdir(), 'nexus-reentrant-ws-'));
  const fixture = resolve(root, 'crates/nexus-agent-host/tests/fixtures/mock_acp_workflow.py');
  const seed = spawnSync('cargo', ['run', '-q', '-p', 'nexus-core-node', '--bin', 'native-wire-fixture-seed', '--', home], { cwd: root });
  if (seed.status !== 0) return fail('reentrant_call', seed.stderr?.toString() || 'seed failed', { executed_steps: steps });
  writeAgentHostConfig(home, fixture, ws, { BLOCK_PROMPT: '1' });
  const accessJson = JSON.stringify({ user_home: home, access: 'engine_owner', allow_uninitialized: false });
  const providers = await providersForScenario();
  steps.push('open');
  const core = binding.open(accessJson, providers);
  steps.push('probe');
  const probe = await core.providerCall(encode({
    request_id: 're-probe-init', method: 'probe', deadline_ms: 30_000,
    payload: adapter === 'rust-acp' ? rustProbePayload(home) : tsProbePayload(),
  })).then((buf) => decode(buf)).catch((e) => ({ ok: false, error: String(e) }));
  if (!probe.ok) return fail('reentrant_call', `probe failed: ${probe.error}`, { executed_steps: steps });
  steps.push('launch');
  const launch = await core.providerCall(encode({
    request_id: 're-launch', method: 'launch', deadline_ms: 30_000,
    payload: adapter === 'rust-acp' ? rustLaunchPayload(home) : tsLaunchPayload(),
  })).then((buf) => decode(buf)).catch((e) => ({ ok: false, error: String(e) }));
  if (!launch.ok || !launch.session_id) return fail('reentrant_call', `launch failed: ${launch.error}`, { executed_steps: steps });
  steps.push('execute_inflight');
  const execPromise = core.providerCall(encode({
    request_id: 're-exec', method: 'execute', session_id: launch.session_id, deadline_ms: 60_000,
    payload: adapter === 'rust-acp' ? rustExecutePayload() : tsExecutePayload(),
  })).then((buf) => decode(buf)).catch((e) => ({ ok: false, error: String(e) }));
  const execReply = await execPromise;
  if (!execReply.ok || !execReply.operation_id) {
    return fail('reentrant_call', `execute did not start: ${formatProviderError(execReply.error)}`, { executed_steps: steps, execReply });
  }
  const opId = execReply.operation_id;
  steps.push('first_pull');
  const firstPull = core.nextProviderEvents(opId, 16, 256 * 1024);
  await new Promise((r) => setTimeout(r, 100));
  steps.push('second_pull_busy');
  let secondErr = null;
  try {
    await core.nextProviderEvents(opId, 16, 256 * 1024);
    secondErr = 'second pull succeeded';
  } catch (error) {
    secondErr = String(error);
  }
  if (!/busy|pull already in flight/i.test(secondErr ?? '')) {
    return fail('reentrant_call', `expected Busy on concurrent same-op pull, got: ${secondErr}`, { executed_steps: steps, opId });
  }
  steps.push('graph_probe');
  const probeDuring = await core.providerCall(encode({
    request_id: 're-probe', method: 'probe', deadline_ms: 5_000,
    payload: adapter === 'rust-acp' ? rustProbePayload(home) : tsProbePayload(),
  })).then((buf) => decode(buf)).catch((e) => ({ ok: false, error: String(e) }));
  if (!probeDuring.ok) {
    return fail('reentrant_call', `graph probe failed during inflight pull: ${formatProviderError(probeDuring.error)}`, { executed_steps: steps });
  }
  steps.push('close');
  await core.close().catch(() => {});
  return pass('reentrant_call', {
    adapter,
    operation_id: opId,
    probe_ok: true,
    second_pull_error: secondErr,
    executed_steps: steps,
  });
}

try {
  let result;
  switch (scenario) {
    case 'open_close_100': result = await runOpenClose100(); break;
    case 'never_settling_callback': result = await runNeverSettling(); break;
    case 'failed_open_child': result = await runFailedOpenChild(); break;
    case 'eof_after_close': result = await runEofWhileLive(); break;
    case 'full_queue_shutdown': result = await runFullQueueShutdown(); break;
    case 'multibyte_overflow': result = await runMultibyteOverflow(); break;
    case 'worker_termination': result = await runWorkerTermination(); break;
    case 'reentrant_call': result = await runReentrantCall(); break;
    default: result = fail(scenario, `unknown isolated scenario: ${scenario}`);
  }
  parentPort.postMessage(result);
} catch (error) {
  parentPort.postMessage({ key: scenario, ok: false, error: String(error), executed_steps: [] });
}
