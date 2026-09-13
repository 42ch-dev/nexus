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
  lstatSync,
  mkdirSync,
  openSync,
  readFileSync,
  readdirSync,
  realpathSync,
  renameSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { cpus, release as osRelease, tmpdir, totalmem } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  CANONICAL_SAMPLES,
  KNOWN_CONFOUNDERS,
  REQUIRED_PHASES,
  RUNTIME_SCHEMA,
  digestAppBundle,
  evaluateRuntimeEvidence,
  providerLifecycleComplete,
  sha256File,
  summarise,
  walkFiles,
} from './proof-contract.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const appRoot = join(__dirname, '..');
const ROOT = resolve(appRoot, '..', '..');
const PRODUCT_NAME = 'Nexus RFT Feasibility';
const BUNDLE_ID = 'com.nexus42.rft-electron-proof';
const EXPECTED_TARGET = 'aarch64-apple-darwin';
const EXPECTED_CONTRACT_HASH = 'a36f909f94a4842bcbe96ee4e83e355eb14d01a2758b202af364023ceb08b734';

/** PKG-2 Electron size limits (proof-matrix §2). */
const ELECTRON_ZIP_LIMIT_MIB = 250;
const ELECTRON_INSTALLED_LIMIT_MIB = 600;

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// --- provenance -----------------------------------------------------------

function gitRev(args) {
  const res = spawnSync('git', args, { cwd: ROOT, encoding: 'utf8' });
  return res.status === 0 ? res.stdout : '';
}

/**
 * Immutable source identity for the run: the commit SHA plus a digest over the
 * porcelain status and the diff against it, so a dirty tree is recorded as a
 * stable value rather than an unverifiable "dirty" flag.
 */
function sourceProvenance() {
  const sha = gitRev(['rev-parse', 'HEAD']).trim() || 'unknown';
  const porcelain = gitRev(['status', '--porcelain']);
  const diff = gitRev(['diff', 'HEAD']);
  const treeDigest = createHash('sha256')
    .update(sha)
    .update('\0')
    .update(porcelain)
    .update('\0')
    .update(diff)
    .digest('hex');
  return { source_sha: sha, tree_digest: treeDigest, tree_dirty: porcelain.trim().length > 0 };
}

function findNativeNodeInBundle(appPath) {
  const walked = walkFiles(appPath);
  return walked.files.find((path) => path.endsWith('.node')) ?? null;
}

function readPin(name) {
  try {
    const manifest = JSON.parse(
      readFileSync(join(ROOT, 'apps', 'desktop-electron', 'node_modules', name, 'package.json'), 'utf8'),
    );
    return manifest.version ?? null;
  } catch {
    return null;
  }
}

function buildProvenance(appPath, outDir, command, startedAt) {
  const nativeNode = findNativeNodeInBundle(appPath);
  const digest = digestAppBundle(appPath);
  return {
    ...sourceProvenance(),
    app_path: appPath,
    app_bundle_id: BUNDLE_ID,
    arch: process.arch,
    app_bundle_sha256: digest.sha256,
    app_bundle_file_count: digest.file_count,
    app_bundle_symlink_count: digest.symlink_count,
    native_node_path_relative: nativeNode ? nativeNode.replace(appPath, '') : null,
    native_node_sha256: nativeNode ? sha256File(nativeNode) : null,
    electron_version: readPin('electron'),
    packager_version: readPin('@electron/packager'),
    out_dir: outDir,
    command,
    utc_start: new Date(startedAt).toISOString(),
  };
}

function parseArgs(argv) {
  const out = {
    app: null,
    out: null,
    home: process.env.NEXUS_PROOF_HOME ?? null,
    phases: null,
    diagnostic: false,
    launchMethod: 'direct',
    cold: 10,
    warm: 30,
    soakSeconds: 600,
    cycles: 100,
    portBase: 19300,
    deadlineSeconds: 2100,
  };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--app') out.app = argv[++i];
    else if (arg === '--out') out.out = argv[++i];
    else if (arg === '--home') out.home = argv[++i];
    else if (arg === '--phases') {
      out.phases = argv[++i].split(',').map((s) => s.trim());
      out.diagnostic = true;
    } else if (arg === '--diagnostic') out.diagnostic = true;
    else if (arg === '--launch-method') out.launchMethod = argv[++i];
    else if (arg === '--cold') out.cold = Number(argv[++i]);
    else if (arg === '--warm') out.warm = Number(argv[++i]);
    else if (arg === '--soak-seconds') out.soakSeconds = Number(argv[++i]);
    else if (arg === '--cycles') out.cycles = Number(argv[++i]);
    else if (arg === '--deadline-seconds') out.deadlineSeconds = Number(argv[++i]);
    else if (arg === '--port-base') out.portBase = Number(argv[++i]);
    else if (arg === '--help' || arg === '-h') out.help = true;
  }
  return out;
}

function usage() {
  console.error(
    'usage: node scripts/proof-runtime.mjs --app <App.app> --out <dir> [--home <dir>]\n' +
      '       [--launch-method direct|launchservices] [--diagnostic | --phases a,b]\n' +
      '\n' +
      'Canonical (gating) run: no --phases/--diagnostic. Executes exactly\n' +
      `  ${REQUIRED_PHASES.join(', ')}\n` +
      `  with fixed samples cold=${CANONICAL_SAMPLES.cold} warm=${CANONICAL_SAMPLES.warm} ` +
      `cycles=${CANONICAL_SAMPLES.cycles} soak=${CANONICAL_SAMPLES.soakSeconds}s,\n` +
      '  and is the only mode that may write runtime-lifecycle.json.\n' +
      'Diagnostic runs write runtime-diagnostic.json and can never gate a decision.',
  );
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

/**
 * Raised when the packaged owner is observably unable to serve native work.
 * Callers treat it as a confounder (blocked), never as a measured product fail.
 */
class OwnerUnavailableError extends Error {
  constructor(message) {
    super(message);
    this.name = 'OwnerUnavailableError';
    this.ownerUnavailable = true;
  }
}

/** Readiness diagnostics that mean the owner will never answer this launch. */
const OWNER_UNAVAILABLE_PATTERN = /utility exited|owner_busy|prior owner interrupted|owner is closing|no preload api/i;

class AppUnderProof {
  constructor(appPath, home, port, logPath, launchMethod = 'direct') {
    this.appPath = appPath;
    this.home = home;
    this.port = port;
    this.logPath = logPath;
    this.launchMethod = launchMethod;
    this.child = null;
    this.cdp = null;
    this.pid = null;
    this.launchedVia = null;
    this.exitCode = undefined;
    this.spawnError = null;
  }

  /**
   * Launch the packaged bundle.
   *
   * `direct` executes `Contents/MacOS/<binary>`; `launchservices` asks
   * LaunchServices via `open -n -a`, which is how a user actually starts the
   * app. The distinction matters: only the LaunchServices path reproduces the
   * real launch environment, so a run under `direct` records itself as
   * confounded rather than as a product result.
   */
  async launch() {
    const exe = appExecutable(this.appPath);
    const fd = openSync(this.logPath, 'a');
    const args = [`--remote-debugging-port=${this.port}`];
    // A failed exec raises an 'error' event on the child; without a listener it
    // becomes an uncaught exception and the run writes no evidence at all. Record
    // it instead so the phase reports the failure through its normal path.
    const onSpawnError = (error) => {
      this.spawnError = `${error.code ?? 'spawn_error'}: ${error.message}`;
    };
    if (this.launchMethod === 'launchservices') {
      this.child = spawn('open', ['-n', '-a', this.appPath, '--args', ...args], {
        env: { ...process.env, NEXUS_PROOF_HOME: this.home },
        stdio: ['ignore', fd, fd],
        detached: false,
      });
      this.child.on('error', onSpawnError);
      this.launchedVia = 'launchservices';
      await new Promise((resolvePromise) => this.child.once('exit', resolvePromise));
      this.pid = await this.#discoverMainPid(exe);
    } else {
      this.child = spawn(exe, args, {
        env: { ...process.env, NEXUS_PROOF_HOME: this.home },
        stdio: ['ignore', fd, fd],
        detached: true,
      });
      this.child.on('error', onSpawnError);
      this.child.on('exit', (code) => {
        this.exitCode = code ?? 0;
      });
      this.pid = this.child.pid;
      this.launchedVia = 'direct';
    }
    // A failure to attach can mean the app refused to start (e.g. another
    // instance holds the single-instance lock and this one exited immediately).
    // That is an owner-unavailable condition, not a product measurement.
    let attached;
    try {
      attached = await Cdp.attach(this.port);
    } catch (error) {
      if (this.hasExited()) {
        throw new OwnerUnavailableError(
          `app exited before the renderer was reachable (exit=${this.exitCode ?? 'unknown'}): ${error.message}`,
        );
      }
      throw error;
    }
    this.cdp = attached.cdp;
    if (!this.pid) this.pid = await this.#discoverMainPid(exe);
    return this;
  }

  /** Find the launched main process (not a `--type=...` helper) by executable path. */
  async #discoverMainPid(exe, timeoutMs = 20_000) {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      const main = psTable().find(
        (row) => row.command.startsWith(exe) && !row.command.includes('--type='),
      );
      if (main) return main.pid;
      await sleep(200);
    }
    return null;
  }

  /** True once the main process has died (or failed to spawn) this launch. */
  hasExited() {
    if (this.spawnError) return true;
    if (this.exitCode !== undefined) return true;
    if (this.pid == null) return false;
    return !isAlive(this.pid);
  }

  /** The Electron utility process serving native work, if one is alive. */
  utilityProcess() {
    if (!this.pid) return null;
    return ownedProcesses(this.pid).find((p) => p.command.includes('--type=utility')) ?? null;
  }

  /**
   * Wait for graph-interactive readiness.
   *
   * Aborts as soon as the owner is observably unavailable rather than burning
   * the whole window: a dead main process, or a readiness probe that reports the
   * utility owner died / is fenced, means no measurement will ever arrive. The
   * caller records that as a confounder and a non-green result instead of
   * letting a top-level timeout erase the run (P3-T3 review M2).
   */
  async waitReady(timeoutMs = 60_000) {
    const deadline = Date.now() + timeoutMs;
    let last = null;
    let ownerFailures = 0;
    while (Date.now() < deadline) {
      if (this.hasExited()) {
        throw new OwnerUnavailableError(
          `app process unavailable during readiness (exit=${this.exitCode ?? 'spawn-failed'}${this.spawnError ? ` ${this.spawnError}` : ''})`,
        );
      }
      try {
        last = await this.cdp.evaluateJson(READINESS_EXPR, 60_000);
        if (last?.entities > 0) return last;
        const diagnostic = `${last?.open_error ?? ''} ${last?.graph_error ?? ''}`;
        if (OWNER_UNAVAILABLE_PATTERN.test(diagnostic)) {
          ownerFailures += 1;
          if (ownerFailures >= 3) {
            throw new OwnerUnavailableError(`utility owner unavailable: ${diagnostic.trim()}`);
          }
        } else {
          ownerFailures = 0;
        }
      } catch (error) {
        if (error instanceof OwnerUnavailableError) throw error;
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

/** True once the run has passed its top-level deadline. */
function deadlineExceeded(ctx) {
  return typeof ctx.deadlineAt === 'number' && Date.now() > ctx.deadlineAt;
}

/** Run one launch cohort, aborting the cohort as soon as the owner is unavailable. */
async function runLaunchCohort(ctx, count, label, samples) {
  const { args, outDir, appPath, home } = ctx;
  let port = ctx.portCursor.value;
  let ownerUnavailable = null;
  for (let i = 0; i < count; i += 1) {
    if (deadlineExceeded(ctx)) {
      samples.push({ index: i, ms: null, aborted: 'run_deadline_exceeded' });
      break;
    }
    if (ownerUnavailable) {
      samples.push({ index: i, ms: null, aborted: 'owner_unavailable', error: ownerUnavailable });
      continue;
    }
    await quiesce();
    const app = new AppUnderProof(appPath, home, port++, join(outDir, 'app-launch.log'), args.launchMethod);
    ctx.portCursor.value = port;
    const started = Date.now();
    try {
      await app.launch();
      const ready = await app.waitReady();
      samples.push({ index: i, ms: Date.now() - started, ready });
    } catch (error) {
      if (error instanceof OwnerUnavailableError) {
        ownerUnavailable = String(error.message);
        samples.push({ index: i, ms: null, aborted: 'owner_unavailable', error: ownerUnavailable });
      } else {
        samples.push({ index: i, ms: null, error: String(error), log_tail: app.logTail() });
      }
    }
    const exited = await app.closeCleanly();
    if (!exited) await app.destroy();
  }
  return { label, ownerUnavailable };
}

async function phaseLaunch(ctx) {
  const { args, checks } = ctx;
  const samples = { cold: [], warm: [] };
  ctx.portCursor = { value: args.portBase + 10 };

  const coldCohort = await runLaunchCohort(ctx, args.cold, 'cold', samples.cold);
  const warmCohort = await runLaunchCohort(ctx, args.warm, 'warm', samples.warm);

  const cold = summarise(samples.cold.map((s) => s.ms));
  const warm = summarise(samples.warm.map((s) => s.ms));
  const failures = [...samples.cold, ...samples.warm].filter((s) => s.ms === null).length;
  const ownerUnavailable = coldCohort.ownerUnavailable ?? warmCohort.ownerUnavailable ?? null;
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
    detail:
      `cold(n=${cold.count},p95=${cold.p95}ms,max=${cold.max}ms) warm(n=${warm.count},p95=${warm.p95}ms,max=${warm.max}ms) ` +
      `failures=${failures}${ownerUnavailable ? ' owner_unavailable' : ''}`,
  });
  if (ownerUnavailable) ctx.confounders?.push('utility_owner_unavailable');
  return { samples, summary: { cold, warm }, failures, owner_unavailable: ownerUnavailable };
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
    const row = (graph.result.entities || []).find((e) => e.key_block_id === entity);
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
  const app = new AppUnderProof(appPath, home, args.portBase + 200, join(outDir, 'app-soak.log'), args.launchMethod);
  let soakStats = null;
  try {
    await app.launch();
    await app.waitReady();

    // Two dedicated CAS rows so the soak writes are real compare-and-swap
    // updates rather than blind overwrites.
    const soakEntities = ['kb_50a1c0de', 'kb_50b2c0de'];
    await app.cdp.evaluate(`(async () => {
      const api = window.nexusProof;
      for (const entity_id of ${JSON.stringify(soakEntities)}) {
        const graph = await api.runProofStep('graph', { world_id: 'wld_owned' });
        const row = (graph.result.entities || []).find((e) => e.key_block_id === entity_id);
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
      if (deadlineExceeded(ctx)) {
        resource.aborted = 'run_deadline_exceeded';
        break;
      }
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
    const cyclesApp = new AppUnderProof(appPath, home, args.portBase + 201, join(outDir, 'app-cycles.log'), args.launchMethod);
    await cyclesApp.launch();
    await cyclesApp.waitReady();
    const cycleSamples = [];
    const durations = [];
    for (let i = 1; i <= args.cycles; i += 1) {
      if (deadlineExceeded(ctx)) {
        resource.cycles_aborted = 'run_deadline_exceeded';
        break;
      }
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
    if (error instanceof OwnerUnavailableError || /utility exited|owner unavailable/i.test(String(error))) {
      ctx.confounders?.push('utility_owner_unavailable');
    }
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

/**
 * Drive one complete provider lifecycle and require every observable step:
 * probe, launch, execute, a bounded drain that must observe at least one
 * `MessageDelta` and exactly one terminal (`OpFinished`/`OpFailed`), cancel,
 * and shutdown. A failed pull, a missing terminal event, or a failed shutdown
 * makes the whole operation fail — a dispatched request is not an observed
 * lifecycle (P3-T3 review I7).
 */
async function runProviderOperation(app, home, index) {
  const op = { index, ok: false, steps: {}, drain: { batches: 0, events: 0, deltas: 0, terminals: [] } };
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
    if (!op.steps.launch.ok || !sessionId) return op;
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
    if (!op.steps.execute.ok || !operationId) return op;

    // Bounded full drain: pull until the stream reports no more work, requiring
    // the delta + single terminal pair the lifecycle contract promises.
    let drained = false;
    let pullFailed = null;
    for (let i = 0; i < 40 && !drained; i += 1) {
      const batch = await app.cdp.evaluateJson(
        `window.nexusProof.runProofStep('provider_pull', ${JSON.stringify({
          operation_id: operationId,
          max_events: 16,
          max_bytes: 262144,
        })})`,
        60_000,
      );
      if (!batch?.ok) {
        pullFailed = batch?.error?.message ?? 'pull rejected';
        break;
      }
      op.drain.batches += 1;
      for (const event of batch.result?.events ?? []) {
        op.drain.events += 1;
        if (event?.MessageDelta) op.drain.deltas += 1;
        if (event?.OpFinished || event?.OpFailed) {
          op.drain.terminals.push({
            kind: event.OpFinished ? 'OpFinished' : 'OpFailed',
            operation_id: event.OpFinished?.operation_id ?? event.OpFailed?.operation_id ?? null,
            session_id: event.OpFinished?.session_id ?? event.OpFailed?.session_id ?? null,
            reason: event.OpFinished?.reason ?? event.OpFailed?.error ?? null,
          });
        }
      }
      if (!batch.result?.has_more && op.drain.terminals.length > 0) drained = true;
      else if (!batch.result?.has_more && (batch.result?.events ?? []).length === 0 && i > 2) drained = true;
      else await sleep(25);
    }
    const terminal = op.drain.terminals.at(-1) ?? null;
    op.steps.pull = {
      ok: pullFailed === null && op.drain.terminals.length > 0,
      error: pullFailed,
      batches: op.drain.batches,
      events: op.drain.events,
      deltas: op.drain.deltas,
      terminal_count: op.drain.terminals.length,
      terminal,
      terminal_matches_operation: terminal?.operation_id === operationId,
      terminal_matches_session: terminal?.session_id === sessionId,
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
    // Completeness is defined once, in the shared contract, so the runtime and
    // any fixture verifier cannot disagree about what "done" means.
    op.ok = providerLifecycleComplete(op);
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

/**
 * Read an asar archive's file table.
 *
 * The archive starts with a Pickle header: the JSON directory lives at a
 * 16-byte offset, not immediately after the first size field. Locate it by
 * content and brace-match so a header-layout change cannot silently yield an
 * empty entry list (which would make the "no .node in asar" check vacuous).
 */
function asarEntries(asarPath) {
  const buffer = readFileSync(asarPath);
  const jsonStart = buffer.indexOf(Buffer.from('{"files"'));
  if (jsonStart < 0) throw new Error(`asar header not found in ${asarPath}`);
  let depth = 0;
  let jsonEnd = -1;
  for (let i = jsonStart; i < buffer.length; i += 1) {
    const byte = buffer[i];
    if (byte === 0x7b) depth += 1;
    else if (byte === 0x7d) {
      depth -= 1;
      if (depth === 0) {
        jsonEnd = i;
        break;
      }
    }
  }
  if (jsonEnd < 0) throw new Error(`asar header is not terminated in ${asarPath}`);
  const header = JSON.parse(buffer.subarray(jsonStart, jsonEnd + 1).toString('utf8'));
  const files = [];
  const walk = (node, prefix) => {
    for (const [name, value] of Object.entries(node.files ?? {})) {
      const path = `${prefix}/${name}`;
      if (value.files) walk(value, path);
      else files.push({ path, size: value.size ?? null, unpacked: Boolean(value.unpacked) });
    }
  };
  walk(header, '');
  if (files.length === 0) throw new Error(`asar ${asarPath} lists no files`);
  return files;
}

async function phaseSecurity(ctx) {
  const { args, outDir, appPath, home, checks } = ctx;
  const evidence = { packaged_artifact: {}, renderer: null, asar: null };
  await quiesce();
  const app = new AppUnderProof(appPath, home, args.portBase + 300, join(outDir, 'app-security.log'), args.launchMethod);
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
    evidence.packaged_artifact = (() => {
      const walked = walkFiles(unpackedDir);
      return {
        unpacked_files: walked.files.map((path) => path.replace(unpackedDir, '<app.asar.unpacked>')),
        unpacked_symlinks: walked.symlinks.map((path) => path.replace(unpackedDir, '<app.asar.unpacked>')),
        has_native_node_outside_asar: walked.files.some((path) => path.endsWith('.node')),
      };
    })();

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
    if (error instanceof OwnerUnavailableError || /utility exited|owner unavailable/i.test(String(error))) {
      ctx.confounders?.push('utility_owner_unavailable');
    }
    checks.push({ id: 'SEC-renderer', ok: false, detail: `security phase failed: ${error}` });
    evidence.error = String(error);
    evidence.log_tail = app.logTail();
    await app.destroy();
    return evidence;
  }
}

async function phaseLifecycle(ctx) {
  const { args, outDir, appPath, home, checks } = ctx;
  const evidence = { native: null, provider: null, kill: null, graph_after_kill: null, reopen: null };
  await quiesce();
  const app = new AppUnderProof(appPath, home, args.portBase + 400, join(outDir, 'app-lifecycle.log'), args.launchMethod);
  try {
    await app.launch();
    await app.waitReady();

    // --- compatibility + graph + patch (create / update / stale refusal) ----
    const native = await app.cdp.evaluateJson(
      `(async () => {
         const api = window.nexusProof;
         const compat = await api.runProofStep('compatibility');
         const before = await api.runProofStep('graph', { world_id: 'wld_owned' });
         const entityId = 'kb_a1b2c3d4';
         const existing = (before.result.entities || []).find((e) => e.key_block_id === entityId);
         const created = await api.runProofStep('patch', {
           world_id: 'wld_owned',
           request: { entity_id: entityId, expected_version: existing ? existing.version : 0,
                      patch: { title: 'native-create', block_type: 'character' } },
         });
         const afterCreate = await api.runProofStep('graph', { world_id: 'wld_owned' });
         const rowAfterCreate = (afterCreate.result.entities || []).find((e) => e.key_block_id === entityId);
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
         const rowAfterStale = (afterStale.result.entities || []).find((e) => e.key_block_id === entityId);
         const foreign = await api.runProofStep('graph', { world_id: 'wld_foreign' });
         return {
           compat: compat.result,
           graph_entities: (before.result.entities || []).map((e) => e.key_block_id),
           created_version: created.ok ? created.result.version : null,
           created_ok: created.ok,
           updated_version: updated.ok ? updated.result.version : null,
           updated_ok: updated.ok,
           stale_ok: stale.ok,
           stale_error: stale.error ? stale.error.message : null,
           version_after_stale: rowAfterStale ? rowAfterStale.version : null,
           title_after_stale: rowAfterStale ? rowAfterStale.canonical_name : null,
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
         const row = (graph.result && graph.result.entities || []).find((e) => e.key_block_id === 'kb_a1b2c3d4');
         return { ok: Boolean(graph.ok), version: row ? row.version : null, title: row ? row.canonical_name : null };
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
    // Provenance-bound proof that the packaged `.node` actually loaded and served
    // real work inside the Electron utility process — the contract requires this
    // independently of the pass/fail of the surrounding checks, and the provider
    // side must be a completed lifecycle (delta + one terminal + shutdown), not a
    // dispatched request.
    const launchedProcesses = app.pid ? ownedProcesses(app.pid) : [];
    const providerComplete = providerLifecycleComplete(provider);
    evidence.native_utility_load = {
      ok: Boolean(nativeOk && providerComplete),
      native_loaded_in: 'electron utility process (main is filesystem-probe only)',
      target_triple: native.compat?.target_triple ?? null,
      expected_target_triple: EXPECTED_TARGET,
      contract_tree_sha256: native.compat?.contract_tree_sha256 ?? null,
      contract_hash_matches: native.compat?.contract_tree_sha256 === EXPECTED_CONTRACT_HASH,
      native_api_version: native.compat?.native_api_version ?? null,
      writer_protocol: native.compat?.writer_protocol ?? null,
      real_native_effects: {
        graph_entities_read: native.graph_entities ?? [],
        create_version: native.created_version ?? null,
        update_version: native.updated_version ?? null,
        stale_write_refused: native.stale_ok === false,
        foreign_world_denied: native.foreign_ok === false,
      },
      provider_lifecycle: {
        probe_ok: provider.steps.probe?.ok ?? null,
        launch_ok: provider.steps.launch?.ok ?? null,
        execute_ok: provider.steps.execute?.ok ?? null,
        pull_ok: provider.steps.pull?.ok ?? null,
        message_deltas: provider.steps.pull?.deltas ?? 0,
        terminal_count: provider.steps.pull?.terminal_count ?? 0,
        terminal: provider.steps.pull?.terminal ?? null,
        cancel_ok: provider.steps.cancel?.ok ?? null,
        shutdown_ok: provider.steps.shutdown?.ok ?? null,
        complete: providerComplete,
      },
      utility_process_observed: launchedProcesses.some((p) => p.command.includes('--type=utility')),
      utility_launch_method_evidence: app.launchedVia,
      native_payload_sha256: findNativeNodeInBundle(app.path ?? appPath)
        ? sha256File(findNativeNodeInBundle(app.path ?? appPath))
        : null,
    };
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
    if (error instanceof OwnerUnavailableError || /utility exited|owner unavailable/i.test(String(error))) {
      ctx.confounders?.push('utility_owner_unavailable');
    }
    checks.push({ id: 'LIFECYCLE-native', ok: false, detail: `lifecycle phase failed: ${error}` });
    evidence.error = String(error);
    evidence.log_tail = app.logTail();
    await app.destroy();
    return evidence;
  }
}

function artifactSizes(appPath, outDir) {
  const appSize = dirSize(appPath);
  const zipPath = join(outDir, 'Nexus-RFT-Feasibility-app.zip');
  rmSync(zipPath, { force: true });
  const zip = spawnSync('ditto', ['-c', '-k', '--sequesterRsrc', '--keepParent', appPath, zipPath], {
    encoding: 'utf8',
  });
  const du = spawnSync('du', ['-sk', appPath], { encoding: 'utf8' });
  return {
    app_bundle_bytes: appSize.bytes,
    app_bundle_mib: Number((appSize.bytes / 1048576).toFixed(1)),
    app_bundle_file_count: appSize.file_count,
    app_bundle_symlink_count: appSize.symlink_count,
    app_bundle_disk_kib: du.status === 0 ? Number(du.stdout.trim().split(/\s+/)[0]) : null,
    app_bundle_disk_mib:
      du.status === 0 ? Number((Number(du.stdout.trim().split(/\s+/)[0]) / 1024).toFixed(1)) : null,
    zip_path: zip.status === 0 ? zipPath : null,
    zip_bytes: zip.status === 0 ? statSync(zipPath).size : null,
    zip_mib: zip.status === 0 ? Number((statSync(zipPath).size / 1048576).toFixed(1)) : null,
    zip_error: zip.status === 0 ? null : zip.stderr,
  };
}

/**
 * Installed-bundle footprint: the bytes actually present in the installed
 * tree, counting each physical file once (symlinked duplicates are not
 * followed). This is the measure the PKG-2 "installed bundle" limit refers to.
 */
function dirSize(root) {
  const walked = walkFiles(root);
  return {
    bytes: walked.files.reduce((sum, path) => sum + lstatSync(path).size, 0),
    file_count: walked.files.length,
    symlink_count: walked.symlinks.length,
  };
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
  // Canonicalize so the provenance path matches the gate's realpath comparison
  // even when the evidence root is reached through a symlink (e.g. /tmp).
  const canonicalAppPath = realpathSync(appPath);
  if (!args.home || !existsSync(args.home)) {
    console.error(`--home (or NEXUS_PROOF_HOME) must point at a seeded disposable home; got ${args.home}`);
    process.exit(1);
  }
  if (!['direct', 'launchservices'].includes(args.launchMethod)) {
    console.error(`--launch-method must be direct or launchservices; got ${args.launchMethod}`);
    process.exit(1);
  }

  // --- mode resolution ------------------------------------------------------
  // Only an exact, complete, default-sampled run may write canonical evidence.
  // Anything narrower is a diagnostic: it gets its own file and cannot gate.
  const phases = args.phases ?? [...REQUIRED_PHASES];
  const unknown = phases.filter((phase) => !REQUIRED_PHASES.includes(phase));
  if (unknown.length > 0) {
    console.error(`unknown phase(s): ${unknown.join(', ')}; supported: ${REQUIRED_PHASES.join(', ')}`);
    process.exit(1);
  }
  const canonicalPhases = [...phases].sort().join(',') === [...REQUIRED_PHASES].sort().join(',');
  const canonicalSamples =
    args.cold === CANONICAL_SAMPLES.cold &&
    args.warm === CANONICAL_SAMPLES.warm &&
    args.cycles === CANONICAL_SAMPLES.cycles &&
    args.soakSeconds === CANONICAL_SAMPLES.soakSeconds;
  const gating = !args.diagnostic && canonicalPhases && canonicalSamples;
  const mode = gating ? 'canonical' : 'diagnostic';

  // A previous canonical document must never survive a failed attempt looking
  // current: quarantine it before any work starts, and write the new one
  // atomically at the end.
  const canonicalPath = join(outDir, 'runtime-lifecycle.json');
  let quarantinedPrevious = null;
  if (gating && existsSync(canonicalPath)) {
    quarantinedPrevious = `${canonicalPath}.superseded-${Date.now()}.json`;
    renameSync(canonicalPath, quarantinedPrevious);
  }

  const fixture = ensureAgentHostConfig(args.home);
  const startedAt = Date.now();
  const deadlineAt = startedAt + args.deadlineSeconds * 1000;
  const command = ['node', 'scripts/proof-runtime.mjs', ...process.argv.slice(2)].join(' ');
  const checks = [];
  const confounders = args.launchMethod === 'launchservices' ? [] : ['direct_executable_launch'];
  const evidence = {
    schema: RUNTIME_SCHEMA,
    mode,
    gating,
    app_path: canonicalAppPath,
    bundle_id: BUNDLE_ID,
    home: args.home,
    fixture,
    repository_root: ROOT,
    phases_requested: phases,
    phases_executed: [],
    sample_plan: {
      cold: args.cold,
      warm: args.warm,
      cycles: args.cycles,
      soak_seconds: args.soakSeconds,
    },
    command,
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
    validity: {
      valid: confounders.length === 0,
      launch_method: args.launchMethod,
      confounders: [],
      confounder_notes: {},
    },
    deadline_at: new Date(deadlineAt).toISOString(),
    quarantined_previous_canonical: quarantinedPrevious,
    provenance: buildProvenance(canonicalAppPath, outDir, command, startedAt),
  };
  if (!gating) {
    console.error(
      `[proof-runtime] DIAGNOSTIC run (phases=${phases.join(',')} samples=${JSON.stringify(evidence.sample_plan)} ` +
        `launch=${args.launchMethod}). Non-gating output: it can never be consumed as canonical evidence.`,
    );
  }

  /**
   * Persist the evidence in its current state. Used both for the normal exit and
   * by the top-level deadline, so an expired run still leaves a structured,
   * non-green record instead of nothing.
   */
  /**
   * Fold the accumulated confounders into the document. Confounders are
   * deduplicated: the same condition can be observed by several phases, and the
   * record should state each distinct reason once.
   */
  const applyValidity = () => {
    const unique = [...new Set(confounders)];
    evidence.validity = {
      valid: unique.length === 0,
      launch_method: args.launchMethod,
      confounders: unique,
      confounder_notes: Object.fromEntries(
        unique.map((c) => [c, KNOWN_CONFOUNDERS[c] ?? 'unclassified confounder']),
      ),
    };
  };

  const persist = (finalStatus) => {
    applyValidity();
    evidence.checks = checks;
    evidence.utc_end = new Date().toISOString();
    evidence.elapsed_ms = Date.now() - startedAt;
    evidence.status = finalStatus;
    const fileName = gating ? 'runtime-lifecycle.json' : 'runtime-diagnostic.json';
    const target = join(outDir, fileName);
    const temporary = `${target}.tmp-${process.pid}`;
    writeFileSync(temporary, `${JSON.stringify(evidence, null, 2)}\n`);
    renameSync(temporary, target);
    writeFileSync(`${outDir}/${fileName.replace(/\.json$/, '.raw.json')}`, JSON.stringify(evidence));
    persistSizes();
    return { fileName, target };
  };

  /**
   * Per-architecture Electron size evidence.
   *
   * Measuring the bundle on disk does not depend on whether the app can run, so
   * this document is written independently of the run verdict and of the
   * canonical/diagnostic distinction. That is what makes the PKG-2 size rows
   * derivable per architecture — and therefore reachable for a future GO —
   * without editing the reducer (I11).
   */
  const persistSizes = () => {
    if (!evidence.sizes) return null;
    const limits = { zip_mib: ELECTRON_ZIP_LIMIT_MIB, installed_mib: ELECTRON_INSTALLED_LIMIT_MIB };
    const sizeChecks = [
      {
        id: 'PKG-2-electron-zip',
        ok: evidence.sizes.zip_mib != null && evidence.sizes.zip_mib <= limits.zip_mib,
        measured_mib: evidence.sizes.zip_mib,
        limit_mib: limits.zip_mib,
      },
      {
        id: 'PKG-2-electron-installed',
        ok: evidence.sizes.app_bundle_mib != null && evidence.sizes.app_bundle_mib <= limits.installed_mib,
        measured_mib: evidence.sizes.app_bundle_mib,
        limit_mib: limits.installed_mib,
      },
    ];
    const sizeDoc = {
      schema: 'rft-p3-t3-electron-size/v1',
      status: sizeChecks.every((check) => check.ok) ? 'pass' : 'fail',
      arch: process.arch,
      app_path: canonicalAppPath,
      app_bundle_id: BUNDLE_ID,
      limits,
      sizes: evidence.sizes,
      checks: sizeChecks,
      ...sourceProvenance(),
      command,
      utc_start: evidence.host.utc_start,
      utc_end: new Date().toISOString(),
    };
    const target = join(outDir, 'electron-size.json');
    const temporary = `${target}.tmp-${process.pid}`;
    writeFileSync(temporary, `${JSON.stringify(sizeDoc, null, 2)}\n`);
    renameSync(temporary, target);
    return target;
  };

  const ctx = { args, outDir, appPath, home: args.home, checks, confounders, deadlineAt };

  // Top-level absolute deadline: a run that cannot finish must still record why.
  const deadlineTimer = setTimeout(() => {
    confounders.push('run_deadline_exceeded');
    const { fileName } = persist('fail');
    console.error(
      `[proof-runtime] run deadline of ${args.deadlineSeconds}s exceeded; wrote non-green ${fileName} before exit`,
    );
    process.exit(1);
  }, args.deadlineSeconds * 1000);
  deadlineTimer.unref?.();

  if (phases.includes('launch')) {
    evidence.launch = await phaseLaunch(ctx);
    evidence.phases_executed.push('launch');
    writeFileSync(join(outDir, 'runtime-launch.json'), `${JSON.stringify(evidence.launch, null, 2)}\n`);
  }
  if (phases.includes('resources')) {
    evidence.resources = await phaseResources(ctx);
    evidence.phases_executed.push('resources');
    writeFileSync(join(outDir, 'runtime-resources.json'), `${JSON.stringify(evidence.resources, null, 2)}\n`);
  }
  if (phases.includes('security')) {
    evidence.security = await phaseSecurity(ctx);
    evidence.phases_executed.push('security');
    writeFileSync(join(outDir, 'runtime-security.json'), `${JSON.stringify(evidence.security, null, 2)}\n`);
  }
  if (phases.includes('lifecycle')) {
    evidence.lifecycle = await phaseLifecycle(ctx);
    evidence.phases_executed.push('lifecycle');
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

  let finalStatus;
  if (gating) {
    // A canonical document is `pass` only when the shared contract confirms it
    // is complete (exact phase set, exact check IDs, raw sample observations,
    // provenance) *and* every check passed. Anything else is a fail.
    // Confounders must be in the document *before* the contract evaluates it:
    // a confounded run is a blocked diagnosis, never a shape complaint.
    applyValidity();
    evidence.checks = checks;
    const verdict = evaluateRuntimeEvidence(evidence, {});
    const complete = verdict.state === 'valid-pass' || verdict.state === 'valid-fail';
    evidence.contract_state = verdict.state;
    evidence.contract_reasons = verdict.reasons;
    finalStatus = complete && checks.every((check) => check.ok) ? 'pass' : 'fail';
  } else {
    evidence.note =
      'Non-gating diagnostic output. Written to runtime-diagnostic.json only; it is never a ' +
      'substitute for a canonical runtime-lifecycle.json.';
    finalStatus = 'diagnostic';
  }

  clearTimeout(deadlineTimer);
  const { fileName } = persist(finalStatus);
  console.log(
    JSON.stringify(
      {
        mode,
        status: evidence.status,
        contract_state: evidence.contract_state ?? null,
        confounders: evidence.validity.confounders,
        output: fileName,
        checks: checks.map((c) => `${c.id}:${c.ok ? 'pass' : 'fail'}`),
      },
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
