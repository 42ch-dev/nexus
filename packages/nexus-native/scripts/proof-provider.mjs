#!/usr/bin/env node
import { execFileSync, spawnSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdtempSync, mkdirSync, readFileSync, realpathSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
import { createHash } from 'node:crypto';
import { randomUUID } from 'node:crypto';
import { Worker } from 'node:worker_threads';

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

// Preserve the previous run's evidence before this run can overwrite it —
// including the early failure paths below.
archiveEvidencePreFix6(join(outDir, 'lifecycle.json'));

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
const MAX_PENDING_REQUESTS = 16;
const MAX_ACTIVE_TASKS = 32;
function captureLocalSetUnitEvidence() {
  const out = spawnSync(
    'cargo',
    ['test', '-p', 'nexus-acp-host', '--test', 'localset_shutdown', 'localset_shutdown', '--', '--exact'],
    { cwd: root, encoding: 'utf8' },
  );
  const combined = `${out.stdout}\n${out.stderr}`;
  return {
    ok: out.status === 0,
    exit_code: out.status ?? 1,
    finished_in: combined.match(/finished in ([0-9.]+s)/)?.[1] ?? null,
    command: 'cargo test -p nexus-acp-host --test localset_shutdown localset_shutdown -- --exact',
    tail: combined.trim().split('\n').slice(-6),
  };
}

function gitHeadSha() {
  const out = spawnSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' });
  return out.status === 0 ? out.stdout.trim() : 'unknown';
}

function artifactSha256(path) {
  return createHash('sha256').update(readFileSync(path)).digest('hex');
}

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
  return /stale generation|generation mismatch|invalid principal|tombstone|dead_env|env_dead|provider port unavailable/i.test(s);
}

// EOF/close/queue behaviour is proved per adapter in the isolated workers
// (`proof-isolated-worker.mjs`); those scenarios carry the close envelope, so
// this module no longer keeps a second, weaker implementation of them.

const CLOSE_DEADLINE_MS = 5_000;
const SCENARIO_EVIDENCE_FIELDS = [
  'deadline_ms',
  'elapsed_ms',
  'cleanup_confirmed',
  'pending_task_ids',
  'pending_task_outcomes',
  'surviving_pids',
];

// Scenarios that do not own a close of their own: they run inside the
// happy-path session whose close is measured at the top level, so the close
// envelope gate is applied to their parent instead of to them.
const INHERITS_CLOSE_FROM = {
  terminal_identity: 'happy_lifecycle',
  cooperative_cancel_2s: 'happy_lifecycle',
  late_generation: 'happy_lifecycle',
};

// EOF/close/queue behaviour is proved per adapter in the isolated workers
// (`proof-isolated-worker.mjs`); those scenarios carry the close envelope, so
// this module no longer keeps a second, weaker implementation of them.

/// Reject a scenario whose close evidence is missing, over budget, unconfirmed,
/// or leaves a survivor behind. Returns an error string, or null when the
/// scenario's evidence satisfies the contract.
function validateScenarioEvidence(key, scenario) {
  if (!scenario?.ok) return null;
  if (INHERITS_CLOSE_FROM[key]) {
    if (scenario.inherits_close_from !== INHERITS_CLOSE_FROM[key]) {
      return `${key}: must declare inherits_close_from=${INHERITS_CLOSE_FROM[key]}`;
    }
    return null;
  }
  for (const field of SCENARIO_EVIDENCE_FIELDS) {
    if (scenario[field] === undefined) return `${key}: missing ${field}`;
  }
  if (scenario.elapsed_ms > CLOSE_DEADLINE_MS) {
    return `${key}: close took ${scenario.elapsed_ms}ms > ${CLOSE_DEADLINE_MS}ms budget`;
  }
  if (!scenario.cleanup_confirmed) {
    return `${key}: cleanup_confirmed false`;
  }
  if ((scenario.surviving_pids ?? []).length > 0) {
    return `${key}: surviving_pids ${scenario.surviving_pids.join(',')}`;
  }
  if (key === 'open_close_100') {
    // Q3-S8: the per-cycle growth bound must be present and satisfied in the
    // evidence itself, not only in the worker's own verdict.
    if (typeof scenario.rss_growth_bytes !== 'number') {
      return `${key}: missing per-cycle RSS growth measurement`;
    }
    if (
      typeof scenario.rss_growth_limit_bytes !== 'number' ||
      scenario.rss_growth_bytes > scenario.rss_growth_limit_bytes
    ) {
      return `${key}: RSS grew ${scenario.rss_growth_bytes} bytes past the bound`;
    }
    if (scenario.rss_measured_in !== 'isolated_worker') {
      return `${key}: RSS must be measured inside the isolated worker`;
    }
  }
  if (key === 'worker_termination' || key === 'full_queue_shutdown') {
    // Either the scenario observed an owned child — and then it must carry that
    // child's real OS identity as observed *before* the action — or it must say
    // so explicitly, so a missing guard can never pass silently.
    if (typeof scenario.child_observed !== 'boolean') {
      return `${key}: must declare child_observed`;
    }
    if (scenario.child_observed) {
      const guard = scenario.process_guard;
      if (!guard) return `${key}: missing process_guard for an observed child`;
      if (!(guard.pid > 0)) return `${key}: process_guard has no observed pid`;
      if (guard.birth_ms === null || guard.birth_ms === undefined) {
        return `${key}: process_guard has no OS birth identity`;
      }
      if (guard.pgid === null || guard.pgid === undefined) {
        return `${key}: process_guard has no process-group id`;
      }
      if (guard.alive !== true) {
        return `${key}: process_guard must be observed while the child was alive`;
      }
    } else if (scenario.process_guard != null) {
      return `${key}: no child observed, so no process_guard may be claimed`;
    }
  }
  if (key === 'failed_open_child') {
    if (scenario.bad_provider_id !== 'bad') return `${key}: must target provider id 'bad'`;
    if (scenario.bad_command !== '/nonexistent/nexus-bad-acp') {
      return `${key}: must declare the nonexistent command it attempted`;
    }
    if (!scenario.executed_steps?.includes('probe_bad_child')) {
      return `${key}: never attempted the bad provider probe`;
    }
    if (scenario.probe_error && /not registered/i.test(String(scenario.probe_error))) {
      return `${key}: provider-not-registered is FAIL`;
    }
    if (scenario.launch_error_category !== 'launch_failed') {
      return `${key}: launch must fail as a launch-class error, got ${scenario.launch_error_category}`;
    }
    if (String(scenario.error ?? '').length === 0) {
      return `${key}: launch produced no error evidence`;
    }
  }
  if (key === 'eof_after_close') {
    const err = String(scenario.error ?? '');
    if (!/provider_eof|transport|protocol_error|exited|eof|broken pipe|failed to connect|session creation failed/i.test(err)) {
      return `${key}: expected typed EOF/transport terminal, got ${err}`;
    }
    if (scenario.close_cleanup_confirmed !== true) {
      return `${key}: the measured close must confirm cleanup`;
    }
  }
  return null;
}

function archiveEvidencePreFix6(targetPath) {
  const archive = targetPath.replace(/lifecycle\.json$/, 'lifecycle.pre-fix6.json');
  if (existsSync(targetPath)) copyFileSync(targetPath, archive);
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

function failScenario(key, message, detail = {}) {
  return { key, ok: false, error: message, ...detail };
}


function runIsolatedScenario(scenarioKey, adapter, providers = undefined) {
  return new Promise((resolve, reject) => {
    const worker = new Worker(new URL('./proof-isolated-worker.mjs', import.meta.url), {
      workerData: { root, adapter, scenario: scenarioKey, providers: adapter === 'ts-acp' ? undefined : providers },
    });
    worker.on('message', (msg) => resolve(msg));
    worker.on('error', reject);
    worker.on('exit', (code) => {
      if (code !== 0) reject(new Error(`isolated worker ${scenarioKey} exit ${code}`));
    });
  });
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

async function runAdapterScenarios(core, ctx) {
  const { adapter, home, sessionId, operationId, terminalPayload, providers } = ctx;
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
          inherits_close_from: 'happy_lifecycle',
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
      ? passScenario('cooperative_cancel_2s', {
          cancel_ms: cancelMs,
          terminal_session_id: cancelSession,
          terminal_operation_id: exec.operation_id,
          inherits_close_from: 'happy_lifecycle',
        })
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

  try {
    scenarios.never_settling_callback = await runIsolatedScenario('never_settling_callback', adapter, providers);
    if (scenarios.never_settling_callback.ok && !scenarios.never_settling_callback.executed_steps?.includes('cancel')) {
      scenarios.never_settling_callback = failScenario('never_settling_callback', 'never reached cancel step');
    }
  } catch (error) {
    scenarios.never_settling_callback = failScenario('never_settling_callback', String(error));
  }

  try {
    scenarios.failed_open_child = await runIsolatedScenario('failed_open_child', adapter, providers);
    const failedGate = validateScenarioEvidence('failed_open_child', scenarios.failed_open_child);
    if (failedGate) scenarios.failed_open_child = failScenario('failed_open_child', failedGate);
    if (scenarios.failed_open_child.ok && !scenarios.failed_open_child.executed_steps?.includes('launch_bad_child')) {
      scenarios.failed_open_child = failScenario('failed_open_child', 'never reached bad child launch');
    }
  } catch (error) {
    scenarios.failed_open_child = failScenario('failed_open_child', String(error));
  }

  try {
    scenarios.full_queue_shutdown = await runIsolatedScenario('full_queue_shutdown', adapter, providers);
    if (adapter === 'rust-acp' && scenarios.full_queue_shutdown.ok) {
      if (!scenarios.full_queue_shutdown.rejected_code || /session limit/i.test(String(scenarios.full_queue_shutdown.rejected_code))) {
        scenarios.full_queue_shutdown = failScenario('full_queue_shutdown', `expected LocalSet busy, got ${scenarios.full_queue_shutdown.rejected_code}`);
      }
      if (scenarios.full_queue_shutdown.ok && (scenarios.full_queue_shutdown.active_count ?? 0) < MAX_ACTIVE_TASKS) {
        scenarios.full_queue_shutdown = failScenario('full_queue_shutdown', `active_count too low: ${scenarios.full_queue_shutdown.active_count}`);
      }
      if (scenarios.full_queue_shutdown.ok && (scenarios.full_queue_shutdown.pending_count ?? 0) < MAX_PENDING_REQUESTS) {
        scenarios.full_queue_shutdown = failScenario('full_queue_shutdown', `pending_count too low: ${scenarios.full_queue_shutdown.pending_count}`);
      }
    }
    if (scenarios.full_queue_shutdown.ok && !scenarios.full_queue_shutdown.executed_steps?.includes('close_before_drain')) {
      scenarios.full_queue_shutdown = failScenario('full_queue_shutdown', 'never closed before drain');
    }
  } catch (error) {
    scenarios.full_queue_shutdown = failScenario('full_queue_shutdown', String(error));
  }

  try {
    scenarios.multibyte_overflow = await runIsolatedScenario('multibyte_overflow', adapter, providers);
    if (scenarios.multibyte_overflow.ok && scenarios.multibyte_overflow.rejected === false) {
      scenarios.multibyte_overflow = failScenario('multibyte_overflow', 'adapter did not reject oversized multibyte update');
    }
    if (scenarios.multibyte_overflow.ok && !scenarios.multibyte_overflow.executed_steps?.includes('oversize_execute')) {
      scenarios.multibyte_overflow = failScenario('multibyte_overflow', 'never attempted oversize execute');
    }
  } catch (error) {
    scenarios.multibyte_overflow = failScenario('multibyte_overflow', String(error));
  }

  try {
    scenarios.worker_termination = await runIsolatedScenario('worker_termination', adapter, providers);
    if (scenarios.worker_termination.ok && scenarios.worker_termination.post_kill_execute_ok) {
      scenarios.worker_termination = failScenario('worker_termination', 'in-flight execute succeeded after child kill');
    }
    if (scenarios.worker_termination.ok && !scenarios.worker_termination.executed_steps?.includes('kill_child')) {
      scenarios.worker_termination = failScenario('worker_termination', 'never killed child mid-operation');
    }
  } catch (error) {
    scenarios.worker_termination = failScenario('worker_termination', String(error));
  }

  try {
    scenarios.reentrant_call = await runIsolatedScenario('reentrant_call', adapter, providers);
    if (scenarios.reentrant_call.ok && !scenarios.reentrant_call.executed_steps?.includes('second_pull_busy')) {
      scenarios.reentrant_call = failScenario('reentrant_call', 'never attempted concurrent same-op pull');
    }
    if (scenarios.reentrant_call.ok && !/busy|pull already in flight/i.test(String(scenarios.reentrant_call.second_pull_error ?? ''))) {
      scenarios.reentrant_call = failScenario('reentrant_call', `missing Busy on second pull: ${scenarios.reentrant_call.second_pull_error}`);
    }
  } catch (error) {
    scenarios.reentrant_call = failScenario('reentrant_call', String(error));
  }

  try {
    scenarios.open_close_100 = await runIsolatedScenario('open_close_100', adapter, providers);
    if (scenarios.open_close_100.ok && !scenarios.open_close_100.executed_steps?.includes('close:99')) {
      scenarios.open_close_100 = failScenario('open_close_100', 'did not complete 100 cycles in worker');
    }
  } catch (error) {
    scenarios.open_close_100 = failScenario('open_close_100', String(error));
  }

  try {
    scenarios.eof_after_close = await runIsolatedScenario('eof_after_close', adapter, providers);
    const eofGate = validateScenarioEvidence('eof_after_close', scenarios.eof_after_close);
    if (eofGate) scenarios.eof_after_close = failScenario('eof_after_close', eofGate);
    if (scenarios.eof_after_close.ok && !scenarios.eof_after_close.executed_steps?.includes('launch_eof_fixture')) {
      scenarios.eof_after_close = failScenario('eof_after_close', 'never reached EOF fixture launch');
    }
  } catch (error) {
    scenarios.eof_after_close = failScenario('eof_after_close', String(error));
  }
  const rssEnd = rssSnapshot();
  rssPeak = Math.max(rssPeak, rssEnd.rss);
  // Q3-W6: the isolated workers own the native workload, so their RSS is the
  // measurement that matters. Aggregate what they reported and keep the
  // wrapper's own numbers as a separate, clearly-labelled figure.
  const workerRss = Object.entries(scenarios)
    .filter(([key]) => !key.startsWith('_'))
    .map(([key, scenario]) => ({
      scenario: key,
      rss_start: scenario.rss_start ?? null,
      rss_end: scenario.rss_end ?? null,
      rss_peak: scenario.rss_peak ?? null,
      rss_growth_bytes: scenario.rss_growth_bytes ?? null,
      measured_in: scenario.rss_measured_in ?? null,
    }))
    .filter((entry) => entry.rss_peak !== null || entry.rss_start !== null);
  const workerPeak = workerRss.reduce(
    (peak, entry) => Math.max(peak, entry.rss_peak ?? 0, entry.rss_end ?? 0, entry.rss_start ?? 0),
    0,
  );
  scenarios._rss = {
    wrapper_rss_start: rssStart.rss,
    wrapper_rss_end: rssEnd.rss,
    wrapper_rss_peak: rssPeak,
    // Aggregated from the isolated workers that ran the native environments.
    worker_rss_peak: workerPeak,
    worker_rss_samples: workerRss,
    rss_source: workerPeak > 0 ? 'isolated_workers' : 'wrapper_only',
  };
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

  let shutdownReply;
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
      close_state: closeReport.state,
      cleanup_confirmed: closeReport.cleanup_confirmed,
      deadline_ms: CLOSE_DEADLINE_MS,
      elapsed_ms: closeMs,
      pending_task_ids: (closeReport.pending_operations ?? []).map(String),
      pending_task_outcomes: (closeReport.pending_operations ?? []).map(String),
      surviving_pids: findFixtureChildPids(),
      process_guard: null,
      terminal_session_id: terminalSessionId,
      terminal_operation_id: terminalOpId,
    }),
    ...adapterScenarios,
  };
  if (!scenarios.eof_after_close?.ok) {
    scenarios.eof_after_close = failScenario('eof_after_close', 'live EOF scenario missing or failed');
  }
  scenarios.late_generation = isStaleGenerationError(postCloseErr)
    ? passScenario('late_generation', {
        error: postCloseErr,
        captured_generation: capturedGeneration,
        inherits_close_from: 'happy_lifecycle',
      })
    : failScenario('late_generation', `expected stale generation/tombstone error, got: ${postCloseErr}`);
  for (const key of SCENARIO_KEYS) {
    scenarios[key] ??= failScenario(key, 'missing scenario result');
    const gateErr = validateScenarioEvidence(key, scenarios[key]);
    if (gateErr && scenarios[key].ok) {
      scenarios[key] = failScenario(key, gateErr);
    }
    if (!scenarios[key].ok) {
      console.error('scenario failed', key, scenarios[key]);
      process.exit(1);
    }
  }

  const rss = adapterScenarios._rss ?? {
    wrapper_rss_start: rssSnapshot().rss,
    wrapper_rss_end: rssSnapshot().rss,
    wrapper_rss_peak: rssSnapshot().rss,
    worker_rss_peak: 0,
    worker_rss_samples: [],
    rss_source: 'wrapper_only',
  };
  delete scenarios._rss;
  const survivingPids = findFixtureChildPids();
  if (survivingPids.length > 0) {
    // An owned child that outlives the proof is a leak, never a success: report
    // it in the evidence and fail the run.
    const report = {
      adapter,
      cleanup_confirmed: closeReport.cleanup_confirmed,
      surviving_child_pids: survivingPids,
    };
    writeFileSync(
      join(outDir, 'lifecycle.json'),
      JSON.stringify({ ...report, scenarios, failed: 'surviving_owned_child' }, null, 2),
    );
    console.error('owned child survived the {} lifecycle proof', adapter, report);
    process.exit(1);
  }
  const lifecyclePath = join(outDir, 'lifecycle.json');
  const codeSha = gitHeadSha();
  const evidence = {
    adapter,
    case: caseName,
    code_sha: codeSha,
    artifact_sha256: artifactSha256(nodePath),
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
    rss_start: rss.wrapper_rss_start,
    rss_end: rss.wrapper_rss_end,
    rss_peak: rss.rss_peak ?? rss.wrapper_rss_peak,
    wrapper_rss_start: rss.wrapper_rss_start,
    wrapper_rss_end: rss.wrapper_rss_end,
    wrapper_rss_peak: rss.wrapper_rss_peak,
    worker_rss_peak: rss.worker_rss_peak,
    worker_rss_samples: rss.worker_rss_samples,
    rss_source: rss.rss_source,
    surviving_child_pids: survivingPids,
    scenarios,
    localset_shutdown_unit: captureLocalSetUnitEvidence(),
    sdk,
    ...admittedMeta,
  };
  mkdirSync(outDir, { recursive: true });
  writeFileSync(lifecyclePath, JSON.stringify(evidence, null, 2));
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
