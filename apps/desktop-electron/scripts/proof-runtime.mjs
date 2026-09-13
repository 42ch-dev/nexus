#!/usr/bin/env node
/**
 * P3-T3 runtime proof driver.
 *
 * Launches the packaged Electron proof shell on a real macOS host, drives the
 * restricted proof preload over the Chrome DevTools Protocol, and records raw
 * samples for the proof-matrix rows START-1 / RES-1 / RES-2 / SEC-1 / PKG-2.
 *
 * Everything here executes the packaged artifact. Nothing is inferred from
 * source text, and a phase that cannot run writes its own failure instead of a
 * green summary.
 *
 * Usage:
 *   node scripts/proof-runtime.mjs --app <path/to/App.app> --out <evidence dir>
 *     [--home <seeded disposable home>] [--phases launch,resources,security,lifecycle]
 *     [--cold 10] [--warm 30] [--soak-seconds 600] [--cycles 100]
 */
import { spawn, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  existsSync,
  mkdirSync,
  openSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { cpus, release as osRelease, tmpdir, totalmem } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const appRoot = join(__dirname, '..');
const ROOT = resolve(appRoot, '..', '..');
const PRODUCT_NAME = 'Nexus RFT Feasibility';
const BUNDLE_ID = 'com.nexus42.rft-electron-proof';
const EXPECTED_TARGET = 'aarch64-apple-darwin';
const EXPECTED_CONTRACT_HASH = 'a36f909f94a4842bcbe96ee4e83e355eb14d01a2758b202af364023ceb08b734';

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function parseArgs(argv) {
  const out = {
    app: null,
    out: null,
    home: process.env.NEXUS_PROOF_HOME ?? null,
    phases: ['launch', 'resources', 'security', 'lifecycle'],
    cold: 10,
    warm: 30,
    soakSeconds: 600,
    cycles: 100,
    portBase: 19300,
  };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--app') out.app = argv[++i];
    else if (arg === '--out') out.out = argv[++i];
    else if (arg === '--home') out.home = argv[++i];
    else if (arg === '--phases') out.phases = argv[++i].split(',').map((s) => s.trim());
    else if (arg === '--cold') out.cold = Number(argv[++i]);
    else if (arg === '--warm') out.warm = Number(argv[++i]);
    else if (arg === '--soak-seconds') out.soakSeconds = Number(argv[++i]);
    else if (arg === '--cycles') out.cycles = Number(argv[++i]);
    else if (arg === '--port-base') out.portBase = Number(argv[++i]);
    else if (arg === '--help' || arg === '-h') out.help = true;
  }
  return out;
}

function usage() {
  console.error(
    'usage: node scripts/proof-runtime.mjs --app <App.app> --out <dir> [--home <dir>]\n' +
      '       [--phases launch,resources,security,lifecycle] [--cold N] [--warm N]\n' +
      '       [--soak-seconds N] [--cycles N]',
  );
}

function sha256File(path) {
  return createHash('sha256').update(readFileSync(path)).digest('hex');
}

function canonicalJson(text) {
  return JSON.stringify(JSON.parse(text));
}

function appExecutable(appPath) {
  return join(appPath, 'Contents', 'MacOS', PRODUCT_NAME);
}

function defaultAppPath(arch, out) {
  const packageDir = resolve(out, '..', 'electron-packages', arch);
  return join(packageDir, `${PRODUCT_NAME}-darwin-${arch}`, `${PRODUCT_NAME}.app`);
}

// --- process ownership -----------------------------------------------------

function psTable() {
  const res = spawnSync('ps', ['-axo', 'pid=,ppid=,rss=,pcpu=,command='], { encoding: 'utf8' });
  const rows = [];
  for (const line of res.stdout.split('\n')) {
    const match = /^\s*(\d+)\s+(\d+)\s+(\d+)\s+([\d.]+)\s+(.*)$/.exec(line);
    if (!match) continue;
    rows.push({
      pid: Number(match[1]),
      ppid: Number(match[2]),
      rss_kib: Number(match[3]),
      cpu_percent: Number(match[4]),
      command: match[5],
    });
  }
  return rows;
}

/** Every descendant of `rootPid` that is still alive: main, helpers, utility, provider children. */
function ownedProcesses(rootPid) {
  const rows = psTable();
  const byParent = new Map();
  for (const row of rows) {
    if (!byParent.has(row.ppid)) byParent.set(row.ppid, []);
    byParent.get(row.ppid).push(row);
  }
  const owned = [];
  const queue = [rootPid];
  const seen = new Set([rootPid]);
  while (queue.length) {
    const pid = queue.shift();
    for (const child of byParent.get(pid) ?? []) {
      if (seen.has(child.pid)) continue;
      seen.add(child.pid);
      owned.push(child);
      queue.push(child.pid);
    }
  }
  return owned;
}

function isAlive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

function killTree(rootPid) {
  if (!isAlive(rootPid)) return;
  try {
    process.kill(-rootPid, 'SIGKILL');
  } catch {
    // not a process-group leader; fall through to per-pid kill
  }
  for (const proc of ownedProcesses(rootPid)) {
    try {
      process.kill(proc.pid, 'SIGKILL');
    } catch {
      // already gone
    }
  }
  try {
    process.kill(rootPid, 'SIGKILL');
  } catch {
    // already gone
  }
}

/** Wait for every descendant of `rootPid` to disappear. */
async function waitForExit(rootPid, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (!isAlive(rootPid) && ownedProcesses(rootPid).length === 0) return true;
    await sleep(100);
  }
  return false;
}

function rssSample(rootPid) {
  const owned = ownedProcesses(rootPid);
  const total = owned.reduce((sum, p) => sum + p.rss_kib, 0) * 1024;
  const cpu = owned.reduce((sum, p) => sum + p.cpu_percent, 0);
  return {
    at: new Date().toISOString(),
    rss_bytes: total,
    cpu_percent: cpu,
    processes: owned.map((p) => ({
      pid: p.pid,
      rss_kib: p.rss_kib,
      cpu_percent: p.cpu_percent,
      kind: p.command.includes('--type=utility')
        ? 'utility'
        : p.command.includes('--type=renderer')
          ? 'renderer'
          : p.command.includes('mock_acp_workflow')
            ? 'provider-child'
            : p.command.includes('crashpad')
              ? 'crashpad'
              : p.pid === rootPid
                ? 'main'
                : 'helper',
      command: p.command.slice(0, 160),
    })),
  };
}

// --- CDP ------------------------------------------------------------------

class Cdp {
  constructor(ws) {
    this.ws = ws;
    this.nextId = 1;
    this.pending = new Map();
    this.closed = false;
    ws.addEventListener('message', (event) => this.#onMessage(event.data));
    ws.addEventListener('close', () => {
      this.closed = true;
      for (const { reject } of this.pending.values()) reject(new Error('cdp closed'));
      this.pending.clear();
    });
  }

  #onMessage(data) {
    let message;
    try {
      message = JSON.parse(typeof data === 'string' ? data : String(data));
    } catch {
      return;
    }
    if (message.id === undefined) return;
    const entry = this.pending.get(message.id);
    if (!entry) return;
    this.pending.delete(message.id);
    if (message.error) entry.reject(new Error(`cdp ${message.error.message}`));
    else entry.resolve(message.result);
  }

  send(method, params = {}, timeoutMs = 90_000) {
    if (this.closed) return Promise.reject(new Error('cdp closed'));
    const id = this.nextId++;
    const promise = new Promise((resolvePromise, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`cdp ${method} timed out after ${timeoutMs}ms`));
      }, timeoutMs);
      this.pending.set(id, {
        resolve: (value) => {
          clearTimeout(timer);
          resolvePromise(value);
        },
        reject: (error) => {
          clearTimeout(timer);
          reject(error);
        },
      });
    });
    this.ws.send(JSON.stringify({ id, method, params }));
    return promise;
  }

  async evaluate(expression, timeoutMs = 120_000) {
    const result = await this.send(
      'Runtime.evaluate',
      { expression, awaitPromise: true, returnByValue: true },
      timeoutMs,
    );
    if (result.exceptionDetails) {
      const detail =
        result.exceptionDetails.exception?.description ?? result.exceptionDetails.text ?? 'evaluate failed';
      throw new Error(detail.split('\n')[0]);
    }
    return result.result?.value;
  }

  async evaluateJson(expression, timeoutMs = 120_000) {
    const raw = await this.evaluate(expression, timeoutMs);
    if (typeof raw === 'string') return JSON.parse(raw);
    return raw;
  }

  close() {
    try {
      this.ws.close();
    } catch {
      // ignore
    }
  }

  static async attach(port, { timeoutMs = 40_000 } = {}) {
    const deadline = Date.now() + timeoutMs;
    let target = null;
    while (Date.now() < deadline) {
      try {
        const response = await fetch(`http://127.0.0.1:${port}/json/list`);
        const list = await response.json();
        target = list.find(
          (entry) => entry.type === 'page' && typeof entry.url === 'string' && entry.url.startsWith('nexus-proof://'),
        );
        if (target?.webSocketDebuggerUrl) break;
      } catch {
        // devtools endpoint not up yet
      }
      await sleep(100);
    }
    if (!target?.webSocketDebuggerUrl) {
      throw new Error(`no nexus-proof page target on port ${port}`);
    }
    const ws = new WebSocket(target.webSocketDebuggerUrl);
    await new Promise((resolvePromise, reject) => {
      ws.addEventListener('open', resolvePromise, { once: true });
      ws.addEventListener('error', () => reject(new Error('cdp websocket error')), { once: true });
    });
    const cdp = new Cdp(ws);
    await cdp.send('Runtime.enable');
    return { cdp, target };
  }
}

// --- app launch -----------------------------------------------------------

class AppUnderProof {
  constructor(appPath, home, port, logPath) {
    this.appPath = appPath;
    this.home = home;
    this.port = port;
    this.logPath = logPath;
    this.child = null;
    this.cdp = null;
    this.pid = null;
  }

  async launch() {
    const exe = appExecutable(this.appPath);
    const fd = openSync(this.logPath, 'a');
    this.child = spawn(exe, [`--remote-debugging-port=${this.port}`], {
      env: { ...process.env, NEXUS_PROOF_HOME: this.home },
      stdio: ['ignore', fd, fd],
      detached: true,
    });
    this.pid = this.child.pid;
    const attached = await Cdp.attach(this.port);
    this.cdp = attached.cdp;
    return this;
  }

  async waitReady(timeoutMs = 60_000) {
    const deadline = Date.now() + timeoutMs;
    let last = null;
    while (Date.now() < deadline) {
      try {
        last = await this.cdp.evaluateJson(READINESS_EXPR, 60_000);
        if (last?.entities > 0) return last;
      } catch (error) {
        last = { error: String(error) };
      }
      await sleep(150);
    }
    throw new Error(`graph never became interactive: ${JSON.stringify(last)}`);
  }

  async destroy() {
    if (this.cdp) this.cdp.close();
    if (this.pid) killTree(this.pid);
    if (this.pid) await waitForExit(this.pid, 10_000);
  }

  async closeCleanly() {
    try {
      await this.cdp.evaluate(`window.nexusProof.runProofStep('close')`, 30_000);
    } catch {
      // a dead renderer cannot answer; the process-exit wait below decides
    }
    const exited = await waitForExit(this.pid, 12_000);
    this.cdp?.close();
    return exited;
  }

  logTail(lines = 40) {
    try {
      return readFileSync(this.logPath, 'utf8').trim().split('\n').slice(-lines).join('\n');
    } catch {
      return '';
    }
  }
}

const READINESS_EXPR = `(async () => {
  const api = window.nexusProof;
  if (!api) return { error: 'no preload api' };
  const open = await api.runProofStep('open');
  const graph = await api.runProofStep('graph', { world_id: 'wld_owned' });
  const entities = graph && graph.ok && graph.result ? (graph.result.entities || []).length : -1;
  api.publishHarnessResult('readiness', {
    open_ok: Boolean(open && open.ok),
    graph_ok: Boolean(graph && graph.ok),
    entities,
  });
  return {
    open_ok: Boolean(open && open.ok),
    graph_ok: Boolean(graph && graph.ok),
    entities,
    open_error: open && open.error ? open.error.message : null,
    graph_error: graph && graph.error ? graph.error.message : null,
  };
})()`;

// --- fixtures -------------------------------------------------------------

/**
 * The provider fixture is a real ACP child process. Its recipe is admitted by
 * the Rust host owner from the seeded proof home, exactly as the P2 provider
 * proof does; the driver only writes the fixture configuration into its own
 * disposable home.
 */
function ensureAgentHostConfig(home) {
  const configDir = join(home, 'config');
  mkdirSync(configDir, { recursive: true });
  const configPath = join(configDir, 'agent-host.toml');
  const fixture = join(ROOT, 'crates', 'nexus-agent-host', 'tests', 'fixtures', 'mock_acp_workflow.py');
  if (!existsSync(fixture)) throw new Error(`ACP fixture missing: ${fixture}`);
  const python = spawnSync('bash', ['-lc', 'command -v python3'], { encoding: 'utf8' }).stdout.trim();
  if (!python.startsWith('/')) throw new Error('no absolute python3 on PATH');
  const toml = `[[providers]]
id = "mock-acp"
protocol = "acp"
command = "${python}"
args = ["${fixture}"]
enabled = true

[providers.env]
ACP_FIXTURE_LOG = "${join(home, 'fixture.log')}"
`;
  writeFileSync(configPath, toml);
  return { configPath, fixture, python };
}

function providerPayload(kind, extra = {}) {
  if (kind === 'probe') return { provider_id: 'mock-acp' };
  if (kind === 'launch') return { provider_id: 'mock-acp' };
  if (kind === 'execute') return { kind: 'prompt', content: 'hello' };
  return extra;
}

// --- phases ---------------------------------------------------------------

function summarise(samples) {
  const values = samples.filter((v) => typeof v === 'number' && Number.isFinite(v)).sort((a, b) => a - b);
  if (values.length === 0) return { count: 0, p50: null, p95: null, max: null, min: null };
  const nearestRank = (q) => values[Math.min(values.length - 1, Math.ceil(q * values.length) - 1)];
  return {
    count: values.length,
    min: values[0],
    p50: nearestRank(0.5),
    p95: nearestRank(0.95),
    max: values[values.length - 1],
  };
}

async function phaseLaunch(ctx) {
  const { args, outDir, appPath, home, checks } = ctx;
  const samples = { cold: [], warm: [] };
  let port = args.portBase + 10;

  for (let i = 0; i < args.cold; i += 1) {
    await quiesce();
    const app = new AppUnderProof(appPath, home, port++, join(outDir, 'app-launch.log'));
    const started = Date.now();
    try {
      await app.launch();
      const ready = await app.waitReady();
      samples.cold.push({ index: i, ms: Date.now() - started, ready });
    } catch (error) {
      samples.cold.push({ index: i, ms: null, error: String(error), log_tail: app.logTail() });
    }
    const exited = await app.closeCleanly();
    if (!exited) await app.destroy();
  }

  for (let i = 0; i < args.warm; i += 1) {
    const app = new AppUnderProof(appPath, home, port++, join(outDir, 'app-launch.log'));
    const started = Date.now();
    try {
      await app.launch();
      const ready = await app.waitReady();
      samples.warm.push({ index: i, ms: Date.now() - started, ready });
    } catch (error) {
      samples.warm.push({ index: i, ms: null, error: String(error), log_tail: app.logTail() });
    }
    const exited = await app.closeCleanly();
    if (!exited) await app.destroy();
  }

  const cold = summarise(samples.cold.map((s) => s.ms));
  const warm = summarise(samples.warm.map((s) => s.ms));
  const failures = [...samples.cold, ...samples.warm].filter((s) => s.ms === null).length;
  // START-1: cold p95 <= 5s / max <= 8s; warm p95 <= 3s / max <= 5s.
  const pass =
    failures === 0 &&
    cold.count === args.cold &&
    warm.count === args.warm &&
    cold.p95 <= 5000 &&
    cold.max <= 8000 &&
    warm.p95 <= 3000 &&
    warm.max <= 5000;
  checks.push({
    id: 'START-1',
    ok: pass,
    detail: `cold(n=${cold.count},p95=${cold.p95}ms,max=${cold.max}ms) warm(n=${warm.count},p95=${warm.p95}ms,max=${warm.max}ms) failures=${failures}`,
  });
  return { samples, summary: { cold, warm }, failures };
}

const SOAK_START_EXPR = (readsPerSecond, writesPerSecond, entities) => `(() => {
  const api = window.nexusProof;
  const state = window.__proofSoak = {
    running: true, reads: 0, writes: 0, read_errors: 0, write_errors: [],
    conflicts: 0, started_at: Date.now(),
  };
  const entities = ${JSON.stringify(entities)};
  let slot = 0;
  const tick = async (fn, rate) => {
    while (state.running) {
      const began = Date.now();
      try { await fn(); } catch (error) {
        state.read_errors += 1;
        state.last_error = String(error);
      }
      const budget = 1000 / rate - (Date.now() - began);
      if (budget > 0) await new Promise((r) => setTimeout(r, budget));
    }
  };
  const readLoop = tick(async () => {
    const reply = await api.runProofStep('graph', { world_id: 'wld_owned' });
    if (reply && reply.ok) state.reads += 1; else state.read_errors += 1;
  }, ${readsPerSecond});
  const writeLoop = tick(async () => {
    const entity = entities[slot++ % entities.length];
    const graph = await api.runProofStep('graph', { world_id: 'wld_owned' });
    if (!graph || !graph.ok) { state.write_errors.push('graph'); return; }
    const row = (graph.result.entities || []).find((e) => e.entity_id === entity);
    if (!row) { state.write_errors.push('missing:' + entity); return; }
    const reply = await api.runProofStep('patch', {
      world_id: 'wld_owned',
      request: { entity_id: entity, expected_version: row.version, patch: { title: 'soak-' + row.version } },
    });
    if (reply && reply.ok) state.writes += 1;
    else if (reply && reply.error && /version/i.test(reply.error.message || '')) state.conflicts += 1;
    else state.write_errors.push(reply && reply.error ? reply.error.message : 'unknown');
  }, ${writesPerSecond});
  Promise.all([readLoop, writeLoop]).then(() => { state.finished = true; });
  return { started: true };
})()`;

const SOAK_STOP_EXPR = `(() => { if (window.__proofSoak) window.__proofSoak.running = false; return window.__proofSoak; })()`;
const SOAK_STATS_EXPR = `(() => window.__proofSoak || null)()`;

async function quiesce() {
  await sleep(2000);
}

async function phaseResources(ctx) {
  const { args, outDir, appPath, home, checks } = ctx;
  const resource = { soak: null, cycles: null };
  await quiesce();
  const app = new AppUnderProof(appPath, home, args.portBase + 200, join(outDir, 'app-soak.log'));
  let soakStats = null;
  try {
    await app.launch();
    await app.waitReady();

    // Two dedicated CAS rows so the soak writes are real compare-and-swap
    // updates rather than blind overwrites.
    const soakEntities = ['kb_soak_a', 'kb_soak_b'];
    await app.cdp.evaluate(`(async () => {
      const api = window.nexusProof;
      for (const entity_id of ${JSON.stringify(soakEntities)}) {
        const graph = await api.runProofStep('graph', { world_id: 'wld_owned' });
        const row = (graph.result.entities || []).find((e) => e.entity_id === entity_id);
        await api.runProofStep('patch', {
          world_id: 'wld_owned',
          request: {
            entity_id,
            expected_version: row ? row.version : 0,
            patch: { title: 'soak-seed', block_type: 'character' },
          },
        });
      }
      return true;
    })()`);

    // RES-1 idle: 30 s quiescence before the first sample.
    await sleep(30_000);
    const idleSample = rssSample(app.pid);

    await app.cdp.evaluate(SOAK_START_EXPR(10, 2, soakEntities), 60_000);
    const trace = [];
    const providerOps = [];
    const soakBegan = Date.now();
    const soakMs = args.soakSeconds * 1000;
    let nextProviderAt = 15_000;
    while (Date.now() - soakBegan < soakMs) {
      trace.push(rssSample(app.pid));
      if (Date.now() - soakBegan >= nextProviderAt && providerOps.length < 4) {
        providerOps.push(await runProviderOperation(app, home, providerOps.length));
        nextProviderAt += Math.floor(soakMs / 4);
      }
      await sleep(250);
    }
    soakStats = await app.cdp.evaluateJson(SOAK_STOP_EXPR, 30_000);
    await sleep(2000);

    const totalSamples = trace.map((sample) => sample.rss_bytes);
    const active = summarise(totalSamples);
    const growth = Math.max(...totalSamples.slice(-8)) - idleSample.rss_bytes;
    const survivalAfterClose = await collectSurvivors(app);
    const closeExited = await app.closeCleanly();

    const providerLifecycleOk = providerOps.every((op) => op.ok);
    const soakPass =
      active.count > 0 &&
      idleSample.rss_bytes <= 500 * 1024 * 1024 &&
      active.p95 <= 750 * 1024 * 1024 &&
      soakStats != null &&
      soakStats.read_errors === 0 &&
      soakStats.write_errors.length === 0 &&
      providerLifecycleOk &&
      closeExited;
    checks.push({
      id: 'RES-1',
      ok: soakPass,
      detail:
        `idle=${(idleSample.rss_bytes / 1048576).toFixed(1)}MiB ` +
        `p95_active=${(active.p95 / 1048576).toFixed(1)}MiB max=${(active.max / 1048576).toFixed(1)}MiB ` +
        `reads=${soakStats?.reads ?? 0} writes=${soakStats?.writes ?? 0} conflicts=${soakStats?.conflicts ?? 0} ` +
        `read_errors=${soakStats?.read_errors ?? 'n/a'} provider_ops_ok=${providerLifecycleOk} close_exited=${closeExited}`,
    });
    resource.soak = {
      soak_seconds: args.soakSeconds,
      idle_sample: idleSample,
      trace,
      summary: active,
      growth_vs_idle_bytes: growth,
      workload: soakStats,
      provider_operations: providerOps,
      survivors_after_close: survivalAfterClose,
      close_exited: closeExited,
      note:
        'integer-truncated soak reading: 10 graph reads/s + 2 CAS writes/s continuous; ' +
        '"4 provider operations" run once per quarter of the soak (4 total), matching the ' +
        'matrix line where the per-second rate binds only reads and writes.',
    };

    // RES-2: 100 open/close cycles in one process, plateau comparison.
    const cyclesApp = new AppUnderProof(appPath, home, args.portBase + 201, join(outDir, 'app-cycles.log'));
    await cyclesApp.launch();
    await cyclesApp.waitReady();
    const cycleSamples = [];
    const durations = [];
    for (let i = 1; i <= args.cycles; i += 1) {
      const began = Date.now();
      await cyclesApp.cdp.evaluate(`window.nexusProof.runProofStep('close')`, 30_000);
      await cyclesApp.cdp.evaluate(`window.nexusProof.runProofStep('open')`, 30_000);
      durations.push(Date.now() - began);
      if (i % 10 === 0 || i === 1) cycleSamples.push({ cycle: i, ...rssSample(cyclesApp.pid) });
    }
    await cyclesApp.cdp.evaluate(`window.nexusProof.runProofStep('close')`, 30_000);
    const survivors = await collectSurvivors(cyclesApp);
    await sleep(30_000);
    const plateau = rssSample(cyclesApp.pid);
    const cycleExit = await cyclesApp.closeCleanly();
    const plateauStart = cycleSamples[0]?.rss_bytes ?? 0;
    const retainedGrowth = plateau.rss_bytes - plateauStart;
    const cyclesPass =
      durations.length === args.cycles &&
      retainedGrowth <= 20 * 1024 * 1024 &&
      survivors.length === 0 &&
      cycleExit;
    checks.push({
      id: 'RES-2',
      ok: cyclesPass,
      detail:
        `cycles=${durations.length} retained_growth=${(retainedGrowth / 1048576).toFixed(1)}MiB ` +
        `survivors_after_final_close=${survivors.length} close_exited=${cycleExit} ` +
        `cycle_ms p95=${summarise(durations).p95}`,
    });
    resource.cycles = {
      cycle_count: durations.length,
      durations_ms: durations,
      summary: summarise(durations),
      samples: cycleSamples,
      plateau_after_cooldown: plateau,
      retained_growth_bytes: retainedGrowth,
      survivors_after_final_close: survivors,
      close_exited: cycleExit,
    };
    return resource;
  } catch (error) {
    checks.push({ id: 'RES-1', ok: false, detail: `resources phase failed: ${error}` });
    resource.error = String(error);
    resource.log_tail = app.logTail();
    await app.destroy();
    return resource;
  }
}

/**
 * A failed or killed owner must not leave the provider child or any other
 * descendant behind, and the reader must not be able to keep issuing work.
 */
async function collectSurvivors(app) {
  const owned = ownedProcesses(app.pid);
  return owned
    .filter((p) => p.command.includes('mock_acp_workflow') || p.command.includes('--type=utility'))
    .map((p) => ({ pid: p.pid, kind: p.command.includes('--type=utility') ? 'utility' : 'provider-child' }));
}

async function runProviderOperation(app, home, index) {
  const op = { index, ok: false, steps: {} };
  try {
    const probe = await app.cdp.evaluateJson(
      `window.nexusProof.runProofStep('provider_probe', ${JSON.stringify({
        request_id: `soak-probe-${index}`,
        method: 'probe',
        deadline_ms: 30_000,
        payload: providerPayload('probe'),
      })})`,
      60_000,
    );
    op.steps.probe = { ok: Boolean(probe?.ok), available: probe?.result?.health?.available ?? null };
    const launch = await app.cdp.evaluateJson(
      `window.nexusProof.runProofStep('provider_probe', ${JSON.stringify({
        request_id: `soak-launch-${index}`,
        method: 'launch',
        deadline_ms: 30_000,
        payload: providerPayload('launch'),
      })})`,
      60_000,
    );
    const sessionId = launch?.result?.session_id ?? null;
    op.steps.launch = { ok: Boolean(launch?.ok), session_id: sessionId };
    if (!launch?.ok || !sessionId) return op;
    const execute = await app.cdp.evaluateJson(
      `window.nexusProof.runProofStep('provider_probe', ${JSON.stringify({
        request_id: `soak-execute-${index}`,
        method: 'execute',
        session_id: sessionId,
        deadline_ms: 30_000,
        payload: providerPayload('execute'),
      })})`,
      60_000,
    );
    const operationId = execute?.result?.operation_id ?? null;
    op.steps.execute = { ok: Boolean(execute?.ok), operation_id: operationId };
    if (!operationId) return op;
    const batch = await app.cdp.evaluateJson(
      `window.nexusProof.runProofStep('provider_pull', ${JSON.stringify({
        operation_id: operationId,
        max_events: 16,
        max_bytes: 262144,
      })})`,
      60_000,
    );
    op.steps.pull = {
      ok: Boolean(batch?.ok),
      events: batch?.result?.events?.length ?? 0,
      has_more: batch?.result?.has_more ?? null,
    };
    const cancel = await app.cdp.evaluateJson(
      `window.nexusProof.runProofStep('provider_probe', ${JSON.stringify({
        request_id: `soak-cancel-${index}`,
        method: 'cancel',
        session_id: sessionId,
        operation_id: operationId,
        deadline_ms: 30_000,
        payload: {},
      })})`,
      60_000,
    );
    op.steps.cancel = { ok: Boolean(cancel?.ok), error: cancel?.error?.message ?? null };
    const shutdown = await app.cdp.evaluateJson(
      `window.nexusProof.runProofStep('provider_probe', ${JSON.stringify({
        request_id: `soak-shutdown-${index}`,
        method: 'shutdown',
        session_id: sessionId,
        deadline_ms: 30_000,
        payload: {},
      })})`,
      60_000,
    );
    op.steps.shutdown = { ok: Boolean(shutdown?.ok) };
    op.ok = Boolean(op.steps.probe.ok && op.steps.launch.ok && op.steps.execute.ok && op.steps.cancel.ok);
  } catch (error) {
    op.error = String(error);
  }
  return op;
}

const SECURITY_EXPR = `(async () => {
  const api = window.nexusProof;
  const globals = {};
  for (const name of ['require', 'process', 'module', 'global', 'Buffer', '__dirname', 'ipcRenderer']) {
    globals[name] = typeof globalThis[name];
  }
  const root = document.getElementById('root');
  const resources = performance.getEntriesByType('resource').map((entry) => entry.name);
  const bundleRequests = resources.filter((name) => name.includes('/assets/'));
  const nativeRequests = resources.filter((name) => name.endsWith('.node'));
  const before = location.href;
  const windowOpen = window.open('https://example.com/proof-window-open');
  try {
    location.href = 'https://example.com/proof-navigation-attempt';
  } catch (error) {
    // ignore: the guard may throw in the page world
  }
  await new Promise((resolve) => setTimeout(resolve, 400));
  return {
    origin: location.origin,
    href: before,
    href_after_navigation_attempt: location.href,
    protocol: location.protocol,
    preload_api_keys: api ? Object.keys(api).sort() : null,
    globals,
    root_child_count: root ? root.childElementCount : -1,
    document_title: document.title,
    harness_root_present: Boolean(document.getElementById('nexus-electron-proof-root')),
    bundle_request_count: bundleRequests.length,
    bundle_requests_sample: bundleRequests.slice(0, 5),
    native_requests_in_renderer: nativeRequests,
    window_open_result: windowOpen === null ? 'null' : 'window',
  };
})()`;

function asarEntries(asarPath) {
  const buffer = readFileSync(asarPath);
  const headerSize = buffer.readUInt32LE(4);
  const headerJson = buffer.subarray(8, 8 + headerSize).toString('utf8');
  const header = JSON.parse(headerJson.replace(/\0+$/, ''));
  const files = [];
  const walk = (node, prefix) => {
    for (const [name, value] of Object.entries(node.files ?? {})) {
      const path = `${prefix}/${name}`;
      if (value.files) walk(value, path);
      else files.push({ path, size: value.size ?? null, unpacked: Boolean(value.unpacked) });
    }
  };
  walk(header, '');
  return files;
}

async function phaseSecurity(ctx) {
  const { args, outDir, appPath, home, checks } = ctx;
  const evidence = { packaged_artifact: {}, renderer: null, asar: null };
  await quiesce();
  const app = new AppUnderProof(appPath, home, args.portBase + 300, join(outDir, 'app-security.log'));
  try {
    const resourcesDir = join(appPath, 'Contents', 'Resources');
    const asarPath = join(resourcesDir, 'app.asar');
    const unpackedDir = join(resourcesDir, 'app.asar.unpacked');
    const entries = asarEntries(asarPath);
    evidence.asar = {
      asar_path: asarPath,
      asar_bytes: statSync(asarPath).size,
      entry_count: entries.length,
      native_entries: entries.filter((entry) => entry.path.endsWith('.node')),
      unpacked_entries: entries.filter((entry) => entry.unpacked).map((entry) => entry.path),
      web_dist_index_present: entries.some((entry) => entry.path === '/web-dist/index.html'),
    };
    evidence.packaged_artifact = {
      unpacked_files: walkFiles(unpackedDir).map((path) => path.replace(unpackedDir, '<app.asar.unpacked>')),
      has_native_node_outside_asar: walkFiles(unpackedDir).some((path) => path.endsWith('.node')),
    };

    await app.launch();
    await app.waitReady();
    evidence.renderer = await app.cdp.evaluateJson(SECURITY_EXPR, 60_000);
    const dom = await app.cdp.evaluateJson(
      `(() => {
         const root = document.getElementById('nexus-electron-proof-root');
         return {
           present: Boolean(root),
           dataset_keys: root ? Object.keys(root.dataset) : [],
           text_preview: root ? root.textContent.slice(0, 400) : null,
         };
       })()`,
      30_000,
    );
    evidence.harness_dom = dom;
    const exited = await app.closeCleanly();
    evidence.close_exited = exited;

    const globals = evidence.renderer.globals ?? {};
    const isolationOk = ['require', 'process', 'module', 'ipcRenderer'].every(
      (name) => globals[name] === 'undefined',
    );
    const apiKeys = evidence.renderer.preload_api_keys ?? [];
    const frozenApiOk =
      apiKeys.length === 4 &&
      apiKeys.every((key) =>
        ['getLifecycle', 'onLifecycleChanged', 'publishHarnessResult', 'runProofStep'].includes(key),
      );
    const navigationOk =
      evidence.renderer.href_after_navigation_attempt === evidence.renderer.href &&
      evidence.renderer.window_open_result === 'null';
    const originOk = evidence.renderer.protocol === 'nexus-proof:';
    const webBundleOk = evidence.renderer.bundle_request_count > 0 && evidence.renderer.root_child_count > 0;
    const nativeClosedInRendererOk =
      (evidence.renderer.native_requests_in_renderer ?? []).length === 0 &&
      (evidence.asar.native_entries ?? []).length === 0;
    const harnessOk = dom.present === true && dom.dataset_keys.includes('readiness');

    checks.push({
      id: 'SEC-renderer',
      ok:
        isolationOk &&
        frozenApiOk &&
        navigationOk &&
        originOk &&
        webBundleOk &&
        nativeClosedInRendererOk &&
        harnessOk,
      detail:
        `isolation=${isolationOk} frozen_api=${frozenApiOk} navigation_guard=${navigationOk} origin=${originOk} ` +
        `web_bundle=${webBundleOk} no_native_in_renderer=${nativeClosedInRendererOk} dom_harness=${harnessOk}`,
    });
    return evidence;
  } catch (error) {
    checks.push({ id: 'SEC-renderer', ok: false, detail: `security phase failed: ${error}` });
    evidence.error = String(error);
    evidence.log_tail = app.logTail();
    await app.destroy();
    return evidence;
  }
}

function walkFiles(root) {
  if (!existsSync(root)) return [];
  const files = [];
  const queue = [root];
  while (queue.length) {
    const dir = queue.pop();
    for (const entry of readdirSync(dir)) {
      const full = join(dir, entry);
      const st = statSync(full);
      if (st.isDirectory()) queue.push(full);
      else files.push(full);
    }
  }
  return files;
}

async function phaseLifecycle(ctx) {
  const { args, outDir, appPath, home, checks } = ctx;
  const evidence = { native: null, provider: null, kill: null, graph_after_kill: null, reopen: null };
  await quiesce();
  const app = new AppUnderProof(appPath, home, args.portBase + 400, join(outDir, 'app-lifecycle.log'));
  try {
    await app.launch();
    await app.waitReady();

    // --- compatibility + graph + patch (create / update / stale refusal) ----
    const native = await app.cdp.evaluateJson(
      `(async () => {
         const api = window.nexusProof;
         const compat = await api.runProofStep('compatibility');
         const before = await api.runProofStep('graph', { world_id: 'wld_owned' });
         const entityId = 'kb_proof_t3';
         const existing = (before.result.entities || []).find((e) => e.entity_id === entityId);
         const created = await api.runProofStep('patch', {
           world_id: 'wld_owned',
           request: { entity_id: entityId, expected_version: existing ? existing.version : 0,
                      patch: { title: 'native-create', block_type: 'character' } },
         });
         const afterCreate = await api.runProofStep('graph', { world_id: 'wld_owned' });
         const rowAfterCreate = (afterCreate.result.entities || []).find((e) => e.entity_id === entityId);
         const updated = await api.runProofStep('patch', {
           world_id: 'wld_owned',
           request: { entity_id: entityId, expected_version: rowAfterCreate.version,
                      patch: { title: 'native-update' } },
         });
         const stale = await api.runProofStep('patch', {
           world_id: 'wld_owned',
           request: { entity_id: entityId, expected_version: 0, patch: { title: 'stale-write' } },
         });
         const afterStale = await api.runProofStep('graph', { world_id: 'wld_owned' });
         const rowAfterStale = (afterStale.result.entities || []).find((e) => e.entity_id === entityId);
         const foreign = await api.runProofStep('graph', { world_id: 'wld_foreign' });
         return {
           compat: compat.result,
           graph_entities: (before.result.entities || []).map((e) => e.entity_id),
           created_version: created.ok ? created.result.version : null,
           created_ok: created.ok,
           updated_version: updated.ok ? updated.result.version : null,
           updated_ok: updated.ok,
           stale_ok: stale.ok,
           stale_error: stale.error ? stale.error.message : null,
           version_after_stale: rowAfterStale ? rowAfterStale.version : null,
           title_after_stale: rowAfterStale ? rowAfterStale.title : null,
           foreign_ok: foreign.ok,
           foreign_error: foreign.ok ? null : foreign.error.message,
         };
       })()`,
      120_000,
    );
    evidence.native = native;

    // --- provider probe / launch / execute / pull / cancel / shutdown -------
    const provider = await runProviderOperation(app, home, 0);
    // Full event drain for the operation so the terminal event is observed.
    evidence.provider = provider;

    // --- forced owner death: reader must see Interrupted, then reopen ------
    const victim = ownedProcesses(app.pid).find((p) => p.command.includes('--type=utility'));
    if (!victim) throw new Error('no utility process to kill');
    process.kill(victim.pid, 'SIGKILL');
    await sleep(1500);
    const afterKill = await app.cdp.evaluateJson(`window.nexusProof.getLifecycle()`, 30_000);
    evidence.kill = { victim_pid: victim.pid, lifecycle_after_kill: afterKill };
    const survivors = await collectSurvivors(app);
    const reopen = await app.cdp.evaluateJson(
      `(async () => {
         const api = window.nexusProof;
         const open = await api.runProofStep('open');
         const graph = await api.runProofStep('graph', { world_id: 'wld_owned' });
         return { open_ok: Boolean(open.ok), open_error: open.error ? open.error.message : null,
                  graph_ok: Boolean(graph.ok), entities: (graph.result && graph.result.entities || []).length,
                  lifecycle: await api.getLifecycle() };
       })()`,
      90_000,
    );
    evidence.reopen = reopen;
    evidence.kill.survivors_after_kill = survivors;

    const graphAfterReopen = await app.cdp.evaluateJson(
      `(async () => {
         const api = window.nexusProof;
         const graph = await api.runProofStep('graph', { world_id: 'wld_owned' });
         const row = (graph.result && graph.result.entities || []).find((e) => e.entity_id === 'kb_proof_t3');
         return { ok: Boolean(graph.ok), version: row ? row.version : null, title: row ? row.title : null };
       })()`,
      60_000,
    );
    evidence.graph_after_kill = graphAfterReopen;
    const closeExit = await app.closeCleanly();
    evidence.close_exited = closeExit;

    const nativeOk =
      native.compat?.target_triple === EXPECTED_TARGET &&
      native.compat?.contract_tree_sha256 === EXPECTED_CONTRACT_HASH &&
      native.graph_entities?.includes('kb_mod') &&
      native.created_ok &&
      native.created_version === 1 &&
      native.updated_ok &&
      native.updated_version === native.created_version + 1 &&
      native.stale_ok === false &&
      native.version_after_stale === native.updated_version &&
      native.title_after_stale === 'native-update' &&
      native.foreign_ok === false;
    const killOk =
      afterKill.phase === 'interrupted' &&
      afterKill.owner_alive === false &&
      survivors.length === 0 &&
      reopen.open_ok === true &&
      reopen.graph_ok === true;
    checks.push({
      id: 'LIFECYCLE-native',
      ok: Boolean(nativeOk && provider.ok),
      detail:
        `target=${native.compat?.target_triple} hash_match=${native.compat?.contract_tree_sha256 === EXPECTED_CONTRACT_HASH} ` +
        `create=${native.created_version} update=${native.updated_version} stale_refused=${native.stale_ok === false} ` +
        `foreign_denied=${native.foreign_ok === false} provider_probe=${provider.steps.probe?.ok} ` +
        `provider_cancel=${provider.steps.cancel?.ok} provider_shutdown=${provider.steps.shutdown?.ok}`,
    });
    checks.push({
      id: 'LIFECYCLE-fault',
      ok: killOk,
      detail:
        `after_kill_phase=${afterKill.phase} owner_alive=${afterKill.owner_alive} ` +
        `survivors=${survivors.length} reopen_ok=${reopen.open_ok} graph_after_reopen=${reopen.graph_ok} ` +
        `graph_version=${graphAfterReopen.version} close_exited=${closeExit}`,
    });
    return evidence;
  } catch (error) {
    checks.push({ id: 'LIFECYCLE-native', ok: false, detail: `lifecycle phase failed: ${error}` });
    evidence.error = String(error);
    evidence.log_tail = app.logTail();
    await app.destroy();
    return evidence;
  }
}

function artifactSizes(appPath, outDir) {
  const appBytes = dirSize(appPath);
  const zipPath = join(outDir, 'Nexus-RFT-Feasibility-app.zip');
  rmSync(zipPath, { force: true });
  const zip = spawnSync('ditto', ['-c', '-k', '--sequesterRsrc', '--keepParent', appPath, zipPath], {
    encoding: 'utf8',
  });
  return {
    app_bundle_bytes: appBytes,
    app_bundle_mib: Number((appBytes / 1048576).toFixed(1)),
    zip_path: zip.status === 0 ? zipPath : null,
    zip_bytes: zip.status === 0 ? statSync(zipPath).size : null,
    zip_mib: zip.status === 0 ? Number((statSync(zipPath).size / 1048576).toFixed(1)) : null,
    zip_error: zip.status === 0 ? null : zip.stderr,
  };
}

function dirSize(root) {
  return walkFiles(root).reduce((sum, path) => sum + statSync(path).size, 0);
}

function electronBundleVersions(appPath) {
  const framework = join(
    appPath,
    'Contents',
    'Frameworks',
    'Electron Framework.framework',
    'Versions',
    'A',
    'Resources',
    'Info.plist',
  );
  const plist = existsSync(framework)
    ? spawnSync('plutil', ['-convert', 'json', '-o', '-', framework], { encoding: 'utf8' }).stdout
    : null;
  let parsed = null;
  try {
    parsed = plist ? JSON.parse(plist) : null;
  } catch {
    parsed = null;
  }
  return {
    info_plist: framework,
    bundle_version: parsed?.CFBundleVersion ?? null,
    short_version: parsed?.CFBundleShortVersionString ?? null,
  };
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) {
    usage();
    process.exit(0);
  }
  if (!args.out) {
    usage();
    process.exit(1);
  }
  const outDir = resolve(args.out);
  mkdirSync(outDir, { recursive: true });
  const appPath = resolve(args.app ?? defaultAppPath(process.arch === 'arm64' ? 'arm64' : 'x64', outDir));
  if (!existsSync(appExecutable(appPath))) {
    console.error(`packaged app missing: ${appPath}`);
    process.exit(1);
  }
  if (!args.home || !existsSync(args.home)) {
    console.error(`--home (or NEXUS_PROOF_HOME) must point at a seeded disposable home; got ${args.home}`);
    process.exit(1);
  }

  const fixture = ensureAgentHostConfig(args.home);
  const startedAt = Date.now();
  const checks = [];
  const evidence = {
    schema: 'rft-p3-t3-runtime-proof/v1',
    app_path: appPath,
    bundle_id: BUNDLE_ID,
    home: args.home,
    fixture,
    host: {
      platform: process.platform,
      arch: process.arch,
      os_release: osRelease(),
      sw_vers: spawnSync('sw_vers', [], { encoding: 'utf8' }).stdout.trim(),
      cpu: cpus()[0]?.model ?? null,
      cpu_count: cpus().length,
      total_memory_bytes: totalmem(),
      node: process.version,
      electron_bundle: electronBundleVersions(appPath),
      utc_start: new Date(startedAt).toISOString(),
    },
    phases_requested: args.phases,
  };

  const ctx = { args, outDir, appPath, home: args.home, checks };

  if (args.phases.includes('launch')) {
    evidence.launch = await phaseLaunch(ctx);
    writeFileSync(join(outDir, 'runtime-launch.json'), `${JSON.stringify(evidence.launch, null, 2)}\n`);
  }
  if (args.phases.includes('resources')) {
    evidence.resources = await phaseResources(ctx);
    writeFileSync(join(outDir, 'runtime-resources.json'), `${JSON.stringify(evidence.resources, null, 2)}\n`);
  }
  if (args.phases.includes('security')) {
    evidence.security = await phaseSecurity(ctx);
    writeFileSync(join(outDir, 'runtime-security.json'), `${JSON.stringify(evidence.security, null, 2)}\n`);
  }
  if (args.phases.includes('lifecycle')) {
    evidence.lifecycle = await phaseLifecycle(ctx);
    writeFileSync(join(outDir, 'runtime-lifecycle-phase.json'), `${JSON.stringify(evidence.lifecycle, null, 2)}\n`);
  }

  evidence.sizes = artifactSizes(appPath, outDir);
  // PKG-2: Electron .app zip <= 250 MiB/arch and installed bundle <= 600 MiB/arch.
  checks.push({
    id: 'PKG-2-electron',
    ok:
      evidence.sizes.zip_mib != null &&
      evidence.sizes.zip_mib <= 250 &&
      evidence.sizes.app_bundle_mib <= 600,
    detail: `zip=${evidence.sizes.zip_mib}MiB installed=${evidence.sizes.app_bundle_mib}MiB`,
  });

  evidence.checks = checks;
  evidence.utc_end = new Date().toISOString();
  evidence.elapsed_ms = Date.now() - startedAt;
  evidence.status = checks.length > 0 && checks.every((check) => check.ok) ? 'pass' : 'fail';
  writeFileSync(join(outDir, 'runtime-lifecycle.json'), `${JSON.stringify(evidence, null, 2)}\n`);
  writeFileSync(join(outDir, 'runtime-lifecycle.raw.json'), `${JSON.stringify(evidence)}\n`);
  console.log(
    JSON.stringify(
      { status: evidence.status, checks: checks.map((c) => `${c.id}:${c.ok ? 'pass' : 'fail'}`) },
      null,
      2,
    ),
  );
  process.exit(evidence.status === 'pass' ? 0 : 1);
}

function isDirectInvocation() {
  return process.argv[1] && resolve(process.argv[1]) === resolve(fileURLToPath(import.meta.url));
}

if (isDirectInvocation()) {
  main().catch((error) => {
    console.error(error instanceof Error ? error.stack : String(error));
    process.exit(1);
  });
}
