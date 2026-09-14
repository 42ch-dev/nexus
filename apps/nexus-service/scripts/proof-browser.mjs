#!/usr/bin/env node
/**
 * P4-T3 real-browser proof orchestrator (RFT-M1 browser vertical).
 *
 * Dependency-free: Node built-ins only (child_process / http / fs / crypto /
 * global WebSocket) plus the workspace's already-built service + Vite tooling
 * and an installed Chromium/Chrome driven over the DevTools Protocol. No
 * lockfile/dependency addition, no jsdom, no mocks, and no source-text
 * substitute for runtime behavior.
 *
 * What it proves end to end (plan §P4-T3 + proof matrix DX/DB/FFI/LIFE/STREAM/P4):
 *   - real browser → generated BrowserClient → HTTP/SSE → napi → Rust DB;
 *   - the real TS ACP SDK adapter behind the Rust provider port;
 *   - graph read, create-on-absent, update and stale CAS with a concurrent
 *     direct `nexus42` CLI writer over the same DB;
 *   - provider stream → terminal, cooperative cancel, and an operation reported
 *     `interrupted` after a service restart;
 *   - schema/native hashes unchanged, exactly 0 Cargo/rustup/native builds
 *     during the TS edit loop, plus measured restart/visibility latencies.
 *
 * Preconditions (the parent owns builds; this runner never compiles Cargo
 * inside the edit loop):
 *   - `apps/nexus-service/dist/main.js` is not required (the service runs from
 *     source under the existing `tsx` dev entrypoint);
 *   - the native binding is loadable by `@42ch/nexus-native`;
 *   - `target/debug/nexus42` exists for the concurrent direct CLI writer
 *     (built once as a recorded baseline step if missing);
 *   - a Chromium/Chrome binary from the Playwright cache or the system.
 *
 * Usage:
 *   node apps/nexus-service/scripts/proof-browser.mjs --samples 30 --port 18421 \
 *     --out .mstar/iterations/v1.189/guides/evidence/browser
 *
 * The process exits nonzero and retains evidence on any failed criterion, and
 * always tears down its child processes and temp fixture on success, failure,
 * and signals.
 */
import { spawn, spawnSync, execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, statSync, writeFileSync, rmSync, mkdtempSync, readdirSync } from 'node:fs';
import http from 'node:http';
import { cpus as osCpus, release as osRelease, tmpdir, totalmem as osTotalmem, type as osType } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(__dirname, '..', '..', '..');
const serviceRoot = resolve(repoRoot, 'apps', 'nexus-service');
const webRoot = resolve(repoRoot, 'apps', 'web');
const nativeFixture = resolve(repoRoot, 'crates/nexus-agent-host/tests/fixtures/mock_acp_workflow.py');

const WORLD_ID = 'wld_owned';
const CAS_ENTITY = 'kb_cas';
// Create-on-absent entity IDs must satisfy the native `kb_<hex>` convention
// (nexus-core `world_kb.rs`); a non-hex id is rejected as validation (422).
const CREATE_ENTITY = 'kb_a11ce001';
const CLI_BINARY = resolve(repoRoot, 'target', 'debug', 'nexus42');

const CARGO_FAMILY = /(?:^|\/)(cargo|rustc|rustup|cc1|clang\+\+?|ld\.lld)(?:\s|$)/i;
const NATIVE_BUILD = /(?:^|\/)(cmake|ninja|make|meson)(?:\s|$)/i;

// ── CLI parsing ─────────────────────────────────────────────────────────────

function parseArgs(argv) {
  let samples = 30;
  let port = 18421;
  let out = null;
  for (let i = 0; i < argv.length; i += 1) {
    const token = argv[i];
    if (token === '--samples') samples = Number.parseInt(argv[++i] ?? '30', 10);
    else if (token === '--port') port = Number.parseInt(argv[++i] ?? '18421', 10);
    else if (token === '--out') out = argv[++i] ?? null;
    else if (token === '--help' || token === '-h') return { help: true };
    else throw new Error(`unknown argument: ${token}`);
  }
  if (!Number.isInteger(samples) || samples < 1) throw new Error('--samples must be a positive integer');
  if (!Number.isInteger(port) || port < 1 || port > 65_535) throw new Error('--port must be 1..65535');
  if (!out) throw new Error('--out <dir> is required');
  return { samples, port, out, help: false };
}

// ── Small timing/report helpers (real behavior, not one-line renames) ────────

function nearestRankP95(values) {
  if (values.length === 0) return null;
  const sorted = [...values].sort((a, b) => a - b);
  const rank = Math.ceil(0.95 * sorted.length);
  return sorted[Math.min(rank, sorted.length) - 1];
}

function maxOf(values) {
  return values.length ? Math.max(...values) : null;
}

function sha256File(path) {
  return createHash('sha256').update(readFileSync(path)).digest('hex');
}

function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

function commandLineMatchesCargo(commandLine) {
  return CARGO_FAMILY.test(commandLine) || NATIVE_BUILD.test(commandLine);
}

// ── Child process lifecycle (tracked + reaped on every exit path) ────────────

const trackedChildren = new Set();
let cleanedUp = false;
// Bounded retained output of closed children: a child's captured output is
// exactly what failure evidence needs to attribute the failure. Entries are
// keyed by unique child identity (label + pid + spawn sequence — labels like
// `nexus-service` repeat across restarts), and retention is capped in entry
// count and tail size, so one child's tail can never be mistaken for another's.
const retainedExitedOutput = new Map();
const RETAINED_EXITED_OUTPUT_MAX = 8;
let childSpawnSequence = 0;

function trackChild(child, label) {
  child.__label = label;
  // Unique identity: labels repeat across service restarts, so the evidence
  // key binds the label to the pid and a process-global spawn sequence — two
  // children can never collide on one retained-output entry.
  child.__evidenceKey = `${label}-pid${child.pid ?? 'x'}-${(childSpawnSequence += 1)}`;
  // A child stays in the tracked set until its lifecycle settles: `exit` can
  // fire before piped stdout/stderr have drained, and the evidence drain scans
  // this set for exited-but-not-closed children — removing at `exit` would
  // hide exactly the children the drain exists to wait for.
  trackedChildren.add(child);
  // The lifecycle settles on the FIRST of `close` or `error`: a failed spawn
  // (ENOENT) never emits `exit`, so every waiter (evidence drain, stop,
  // cleanup) must observe a settled child instead of waiting forever. This
  // `error` listener also keeps a spawn failure from crashing the process as
  // an unhandled 'error' event. `close` still captures the bounded trailing
  // output before the child leaves the tracked set.
  child.__closed = new Promise((resolveClose) => {
    const settle = (err) => {
      if (child.__closeSettled) return;
      child.__closeSettled = true;
      if (err) child.__error = err;
      const text = typeof child.output === 'function' ? child.output() : '';
      if (text) {
        retainedExitedOutput.delete(child.__evidenceKey);
        retainedExitedOutput.set(child.__evidenceKey, text.slice(-MAX_EVIDENCE_TAIL_CHARS));
        while (retainedExitedOutput.size > RETAINED_EXITED_OUTPUT_MAX) {
          const oldest = retainedExitedOutput.keys().next().value;
          retainedExitedOutput.delete(oldest);
        }
      }
      trackedChildren.delete(child);
      resolveClose();
    };
    child.once('close', () => settle());
    child.once('error', (err) => settle(err));
  });
  return child;
}

/**
 * Await `close` for every tracked child that already exited but whose stdio
 * has not closed yet, bounded per child. Failure evidence collected after
 * this drain sees the final retained output of freshly exited children.
 */
async function drainExitedChildrenForEvidence({ timeoutMs = 500 } = {}) {
  const pending = [];
  for (const child of trackedChildren) {
    const exited = child.exitCode !== null || child.signalCode !== null;
    if (!exited || child.__closeSettled || !(child.__closed instanceof Promise)) continue;
    pending.push(Promise.race([child.__closed, sleep(timeoutMs)]));
  }
  await Promise.all(pending);
}

function spawnLogged(command, args, { cwd, env, label, collect = true }) {
  // `detached` puts each child in its own process group so a hard kill reaps the
  // whole tree (a SIGKILL'd service must not orphan its ACP fixture grandchild).
  const child = spawn(command, args, {
    cwd,
    env,
    stdio: ['ignore', 'pipe', 'pipe'],
    detached: true,
  });
  trackChild(child, label);
  const lines = [];
  const onChunk = (buf) => {
    if (!collect) return;
    lines.push(buf.toString());
    if (lines.length > 400) lines.shift();
  };
  child.stdout.on('data', onChunk);
  child.stderr.on('data', onChunk);
  child.output = () => lines.join('');
  return child;
}

/**
 * Shared group-kill path (QC3-C3): signal a child's whole process group. The
 * immediate-pid fallback fires ONLY on ESRCH — the group, leader included, is
 * already gone — so a failed group signal (EPERM, …) can never masquerade as
 * a delivered group kill while group descendants may still be alive. Callers
 * that need survivor proof verify via `survivingGroupRows`, not via this
 * function's silence.
 */
function signalProcessGroup(pid, signal) {
  try {
    process.kill(-pid, signal);
  } catch (err) {
    if (err?.code !== 'ESRCH') throw err;
    try {
      process.kill(pid, signal);
    } catch {
      /* leader already gone too */
    }
  }
}

/**
 * Bounded child stop: SIGTERM the process group, escalate to SIGKILL, sweep
 * the group once more, then await `close` — every wait bounded. A child that
 * already exited but has not closed skips the signaling and goes straight to
 * the sweep + bounded close drain, so a group descendant holding the pipes
 * cannot outlive cleanup, and cleanup can never hang on a pending
 * `exit`/`close`.
 */
async function stopChild(child, { graceMs = 4_000, killMs = 2_000, closeMs = 2_000 } = {}) {
  if (!child || child.__closeSettled) return;
  const pid = child.pid;
  const signalGroup = (signal) => {
    try {
      signalProcessGroup(pid, signal);
    } catch {
      // Cleanup path: a group-signal failure must not abort the bounded
      // escalation below, but it is NOT treated as a delivered group signal.
      // Survivor proof for the abrupt-restart path lives in
      // `runInterruptedRestartCycles` (ps sweep), not here.
    }
  };
  const exited = () => child.exitCode !== null || child.signalCode !== null;
  const awaitExit = (ms) =>
    Promise.race([
      new Promise((resolve) => {
        // Checked synchronously beside the listener registration: a child
        // exiting between the two would otherwise be missed and awaited
        // forever (the exit-listener race).
        if (exited()) return resolve(true);
        child.once('exit', () => resolve(true));
        child.once('error', () => resolve(true));
      }),
      sleep(ms).then(() => false),
    ]);
  if (!exited()) {
    signalGroup('SIGTERM');
    if (!(await awaitExit(graceMs))) {
      signalGroup('SIGKILL');
      // Bounded even post-SIGKILL: an uninterruptible child must not hang
      // cleanup past this deadline.
      await awaitExit(killMs);
    }
  }
  if (!child.__closeSettled) {
    // Exited (or spawn-errored) with stdio still open: a group descendant
    // inheriting the pipes keeps `close` pending. Bounded group sweep so the
    // descendant dies, then a bounded close drain so cleanup neither leaks
    // the pipes nor hangs on them.
    signalGroup('SIGKILL');
    await Promise.race([child.__closed ?? Promise.resolve(), sleep(closeMs)]);
  }
}

async function cleanupChildren() {
  for (const child of [...trackedChildren]) {
    await stopChild(child).catch(() => undefined);
  }
}

// ── HTTP helpers ────────────────────────────────────────────────────────────

function httpRequest(url, { method = 'GET', headers = {}, body } = {}) {
  return new Promise((resolvePromise, reject) => {
    const target = new URL(url);
    const req = http.request(
      {
        hostname: target.hostname,
        port: target.port,
        path: `${target.pathname}${target.search}`,
        method,
        headers,
      },
      (res) => {
        const chunks = [];
        res.on('data', (c) => chunks.push(c));
        res.on('end', () =>
          resolvePromise({
            status: res.statusCode ?? 0,
            headers: res.headers,
            body: Buffer.concat(chunks).toString('utf8'),
          }),
        );
      },
    );
    req.on('error', reject);
    if (body !== undefined) req.write(body);
    req.end();
  });
}

async function jsonRequest(url, options = {}) {
  const headers = { Accept: 'application/json', ...(options.headers ?? {}) };
  let body = options.body;
  if (body !== undefined && typeof body !== 'string') {
    headers['Content-Type'] = 'application/json';
    body = JSON.stringify(body);
  }
  const res = await httpRequest(url, { ...options, headers, body });
  let payload = null;
  try {
    payload = res.body ? JSON.parse(res.body) : null;
  } catch {
    payload = null;
  }
  return { ...res, payload };
}

async function waitForHttpOk(url, { timeoutMs = 120_000, intervalMs = 250 } = {}) {
  const deadline = Date.now() + timeoutMs;
  let last = null;
  while (Date.now() < deadline) {
    try {
      const res = await httpRequest(url);
      last = { status: res.status };
      if (res.status >= 200 && res.status < 300) return res;
    } catch (err) {
      last = { error: err instanceof Error ? err.message : String(err) };
    }
    await sleep(intervalMs);
  }
  throw new Error(`Timed out waiting for HTTP 2xx at ${url}; last=${JSON.stringify(last)}`);
}

// ── Fixture + provider config ────────────────────────────────────────────────

/**
 * Seed a disposable home with the real native fixture (Creator, owned + foreign
 * World, key blocks, two pending candidates). Uses the existing Rust seed bin —
 * a recorded *baseline* native build step, never part of the TS edit loop.
 */
function seedFixtureHome(home) {
  const seed = spawnSync(
    'cargo',
    ['run', '-q', '-p', 'nexus-core-node', '--bin', 'native-wire-fixture-seed', '--', home],
    { cwd: repoRoot, encoding: 'utf8' },
  );
  if (seed.status !== 0) {
    throw new Error(`fixture seed failed (status ${seed.status}): ${seed.stderr ?? ''}`);
  }
}

/** Resolve a real python3 absolute path the ACP fixture can be launched with. */
function resolvePython() {
  return execFileSync('which', ['python3'], { encoding: 'utf8' }).trim();
}

/**
 * Write the agent-host provider config that points `mock-acp` at the real TS-ACP
 * protocol fixture. `extraEnv` toggles fixture modes (e.g. BLOCK_PROMPT=1).
 */
function writeProviderConfig(home, logPath, extraEnv = {}) {
  const agentHostDir = join(home, '.nexus42', 'agent-host');
  mkdirSync(agentHostDir, { recursive: true });
  const python = resolvePython();
  const env = { ACP_FIXTURE_LOG: logPath, ...extraEnv };
  const envLines = Object.entries(env)
    .map(([k, v]) => `${k} = ${JSON.stringify(v)}`)
    .join('\n');
  const config = [
    '[[providers]]',
    'id = "mock-acp"',
    'protocol = "acp"',
    `command = ${JSON.stringify(python)}`,
    `args = [${JSON.stringify(nativeFixture)}]`,
    'enabled = true',
    '',
    '[providers.env]',
    envLines,
    '',
  ].join('\n');
  writeFileSync(join(agentHostDir, 'config.toml'), config);
}

// ── Service + Vite lifecycle ────────────────────────────────────────────────

/**
 * Return a copy of `env` with the proof UI's real loopback origin appended to
 * `NEXUS_DAEMON_ALLOWED_ORIGINS`. The service's `resolveAllowedOrigins` only
 * *appends* env values, so an existing user configuration is preserved. This is
 * the actual dev proof origin (the Vite server), not a security bypass: the
 * origin is a loopback host and the allowlist is the service's existing seam.
 */
function withProofOrigin(env, vitePort) {
  const origin = `http://127.0.0.1:${vitePort}`;
  const existing = (env.NEXUS_DAEMON_ALLOWED_ORIGINS ?? '').trim();
  const merged = existing
    ? existing.split(',').map((s) => s.trim()).includes(origin)
      ? existing
      : `${existing},${origin}`
    : origin;
  return { ...env, NEXUS_DAEMON_ALLOWED_ORIGINS: merged, __RFT_PROOF_ORIGIN: origin };
}

/**
 * Start the real standalone service from source under the existing `tsx` dev
 * entrypoint (no compile step → a TS source edit becomes visible on restart
 * without a build). Returns a handle with a stable `restart()`.
 */
async function startService({ home, port, env }) {
  const url = `http://127.0.0.1:${port}`;
  const child = spawnLogged(
    process.execPath,
    ['--import', 'tsx', resolve(serviceRoot, 'src/main.ts'), '--home', home, '--host', '127.0.0.1', '--port', String(port)],
    { cwd: serviceRoot, env, label: 'nexus-service' },
  );
  const startedAt = Date.now();
  await waitForHttpOk(`${url}/v1/daemon/runtime/health`, { timeoutMs: 60_000 });
  // Health alone is not readiness: a stale/other process could answer liveness
  // while our own service is still initializing. Require the runtime to report
  // an initialized, provider-enabled profile (the mode this proof uses) before
  // returning, so later requests never race the DB/engine open.
  const deadline = Date.now() + 60_000;
  let status = null;
  for (;;) {
    const res = await jsonRequest(`${url}/v1/daemon/runtime/status`);
    status = res.payload;
    if (status?.workspace_initialized === true && status?.runtime_mode === 'provider_enabled') break;
    if (Date.now() >= deadline) {
      throw new Error(
        `service did not reach provider_enabled readiness: ${JSON.stringify(status)}`,
      );
    }
    await sleep(150);
  }
  const readyMs = Date.now() - startedAt;
  return { child, url, port, home, env, readyMs, status };
}

async function stopService(handle) {
  if (!handle) return;
  await stopChild(handle.child);
}

/**
 * Restart the service on the same home/port, measuring spawn→health-ready.
 */
async function restartService(handle, { providerEnv } = {}) {
  const home = handle.home;
  const logPath = handle.logPath;
  await stopService(handle);
  await waitForPortFree(handle.port);
  if (providerEnv) writeProviderConfig(home, logPath, providerEnv);
  const next = await startService({ home, port: handle.port, env: handle.env });
  next.logPath = logPath;
  return next;
}

async function startVite({ daemonUrl, port, env }) {
  const viteBin = resolve(webRoot, 'node_modules', '.bin', 'vite');
  if (!existsSync(viteBin)) {
    throw new Error(`Vite binary not found at ${viteBin}; run the web install first`);
  }
  const child = spawnLogged(
    viteBin,
    ['--port', String(port), '--strictPort'],
    {
      cwd: webRoot,
      env: { ...env, VITE_DAEMON_URL: daemonUrl, VITE_RFT_NATIVE_PROOF: '1' },
      label: 'vite',
    },
  );
  const url = `http://127.0.0.1:${port}`;
  await waitForHttpOk(url, { timeoutMs: 60_000 });
  return { child, url, port };
}

// ── Chromium / CDP driver ───────────────────────────────────────────────────

function resolveChromium() {
  // Explicit override wins.
  if (process.env.RFT_CHROMIUM && existsSync(process.env.RFT_CHROMIUM)) {
    return process.env.RFT_CHROMIUM;
  }

  const home = process.env.HOME ?? '';
  const macLayouts = ['chrome-mac-arm64', 'chrome-mac'];

  // 1. Puppeteer cache — PREFERRED. The Playwright-bundled Chrome 149 on this
  //    macOS build hangs on the proof URL (direct `--dump-dom` never commits);
  //    the Puppeteer Chrome 147 loads it in ~3.5s. Enumerate
  //    `~/.cache/puppeteer/chrome/<revision>-<version>/<layout>/...` dynamically
  //    (no version hardcode), newest revision first.
  const puppeteerCache = join(home, '.cache/puppeteer/chrome');
  const puppeteerCandidates = [];
  if (existsSync(puppeteerCache)) {
    let entries = [];
    try {
      entries = readdirSync(puppeteerCache, { withFileTypes: true });
    } catch {
      entries = [];
    }
    for (const dir of entries
      .filter((e) => e.isDirectory())
      .map((e) => e.name)
      .sort()
      .reverse()) {
      for (const layout of macLayouts) {
        puppeteerCandidates.push(
          join(
            puppeteerCache,
            dir,
            layout,
            'Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing',
          ),
        );
        puppeteerCandidates.push(
          join(puppeteerCache, dir, layout, 'Chromium.app/Contents/MacOS/Chromium'),
        );
      }
      puppeteerCandidates.push(join(puppeteerCache, dir, 'chrome-linux64', 'chrome'));
    }
  }

  // 2. Playwright cache. Enumerate `chromium-*` revisions (no version hardcode);
  //    both `chrome-mac`/`chrome-mac-arm64` layouts and the `Chromium.app` /
  //    `Google Chrome for Testing.app` binary names.
  const playwrightCache = join(home, 'Library/Caches/ms-playwright');
  const chromiumCandidates = [];
  if (existsSync(playwrightCache)) {
    let entries = [];
    try {
      entries = readdirSync(playwrightCache, { withFileTypes: true });
    } catch {
      entries = [];
    }
    for (const dir of entries
      .filter((e) => e.isDirectory() && e.name.startsWith('chromium-'))
      .map((e) => e.name)
      .sort()
      .reverse()) {
      for (const layout of macLayouts) {
        chromiumCandidates.push(
          join(playwrightCache, dir, layout, 'Chromium.app/Contents/MacOS/Chromium'),
        );
        // Newer Playwright revisions ship "Google Chrome for Testing.app".
        chromiumCandidates.push(
          join(
            playwrightCache,
            dir,
            layout,
            'Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing',
          ),
        );
      }
      // Linux/Windows headless shell + plain chromium layouts occasionally appear
      // in the cache as well.
      chromiumCandidates.push(join(playwrightCache, dir, 'chrome-linux', 'chrome'));
    }
  }

  const candidates = [
    ...puppeteerCandidates,
    ...chromiumCandidates,
    '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
    '/Applications/Chromium.app/Contents/MacOS/Chromium',
    '/usr/bin/google-chrome',
    '/usr/bin/chromium',
    '/usr/bin/chromium-browser',
  ];
  return candidates.find((p) => p && existsSync(p)) ?? null;
}

/** Minimal CDP client over the page target's WebSocket. */
class CdpClient {
  constructor(wsUrl) {
    this.wsUrl = wsUrl;
    this.nextId = 1;
    this.pending = new Map();
  }

  async connect({ timeoutMs = 15_000 } = {}) {
    const ws = new WebSocket(this.wsUrl);
    this.ws = ws;
    await new Promise((resolvePromise, reject) => {
      const timer = setTimeout(() => reject(new Error('CDP WebSocket connect timeout')), timeoutMs);
      ws.addEventListener('open', () => {
        clearTimeout(timer);
        resolvePromise();
      });
      ws.addEventListener('error', (event) => {
        clearTimeout(timer);
        reject(new Error(`CDP WebSocket error: ${event?.message ?? 'unknown'}`));
      });
    });
    // Only request/response frames are consumed; the driver waits on page state
    // (polling) rather than CDP events, so unsolicited notifications are ignored.
    ws.addEventListener('message', (event) => {
      const message = JSON.parse(String(event.data));
      if (message.id && this.pending.has(message.id)) {
        const { resolve: res, reject: rej } = this.pending.get(message.id);
        this.pending.delete(message.id);
        if (message.error) rej(new Error(`${message.error.message} (${message.error.code})`));
        else res(message.result);
      }
    });
    return this;
  }

  send(method, params = {}, { timeoutMs = 30_000 } = {}) {
    const id = this.nextId++;
    return new Promise((resolvePromise, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`CDP ${method} timeout`));
      }, timeoutMs);
      this.pending.set(id, {
        resolve: (value) => {
          clearTimeout(timer);
          resolvePromise(value);
        },
        reject: (err) => {
          clearTimeout(timer);
          reject(err);
        },
      });
      this.ws.send(JSON.stringify({ id, method, params }));
    });
  }

  close() {
    try {
      this.ws?.close();
    } catch {
      /* ignore */
    }
  }
}

async function waitForJson(url, { timeoutMs = 30_000, intervalMs = 200 } = {}) {
  const deadline = Date.now() + timeoutMs;
  let last = null;
  while (Date.now() < deadline) {
    try {
      const res = await httpRequest(url);
      if (res.status >= 200 && res.status < 300) return JSON.parse(res.body);
      last = { status: res.status };
    } catch (err) {
      last = { error: err instanceof Error ? err.message : String(err) };
    }
    await sleep(intervalMs);
  }
  throw new Error(`Timed out waiting for JSON at ${url}; last=${JSON.stringify(last)}`);
}

async function launchChromium({ debugPort, userDataDir }) {
  const binary = resolveChromium();
  if (!binary) {
    throw new Error(
      'No Chromium/Chrome binary found (searched RFT_CHROMIUM, the Playwright cache, and the system).',
    );
  }
  const child = spawnLogged(
    binary,
    [
      '--headless=new',
      `--remote-debugging-port=${debugPort}`,
      `--user-data-dir=${userDataDir}`,
      '--no-first-run',
      '--no-default-browser-check',
      '--disable-gpu',
      '--window-size=1440,960',
      'about:blank',
    ],
    // Collect (bounded) stderr so a launch/navigation failure is attributable.
    { env: process.env, label: 'chromium', collect: true },
  );
  const version = await waitForJson(`http://127.0.0.1:${debugPort}/json/version`, { timeoutMs: 30_000 });
  return { child, binary, version, debugPort };
}

async function openPageTarget(debugPort, pageUrl) {
  // Exactly ONE navigation, owned by `/json/new?<url>`: verified empirically
  // against this Chrome 149 build (`/json/new?<encoded url>` returns a target
  // whose `url` is that URL). `Page.navigate` hangs on this CDP client, so no
  // explicit navigate command is issued; the driver connects/enables and polls
  // the readiness handle, tolerating a page that loaded before enable.
  const res = await httpRequest(
    `http://127.0.0.1:${debugPort}/json/new?${encodeURIComponent(pageUrl)}`,
    { method: 'PUT' },
  );
  if (res.status < 200 || res.status >= 300) {
    throw new Error(`failed to open CDP target: HTTP ${res.status}`);
  }
  const info = JSON.parse(res.body);
  return info;
}

/** Evaluate an expression in the page; `awaitPromise` resolves real promises. */
async function evaluate(client, expression, { awaitPromise = true, timeoutMs = 30_000 } = {}) {
  const result = await client.send(
    'Runtime.evaluate',
    { expression, awaitPromise, returnByValue: true },
    { timeoutMs },
  );
  if (result.exceptionDetails) {
    throw new Error(`page evaluation failed: ${JSON.stringify(result.exceptionDetails)}`);
  }
  return result.result?.value;
}

async function waitForPageCondition(client, expression, { timeoutMs = 15_000, intervalMs = 150 } = {}) {
  const deadline = Date.now() + timeoutMs;
  let last;
  while (Date.now() < deadline) {
    try {
      last = await evaluate(client, expression);
      if (last) return last;
    } catch (err) {
      last = err instanceof Error ? err.message : String(err);
    }
    await sleep(intervalMs);
  }
  throw new Error(`Timed out waiting for page condition: ${expression}; last=${JSON.stringify(last)}`);
}

/**
 * Bounded page diagnostics for an absent/failed proof handle: location, title,
 * a body excerpt, and any captured Runtime exceptions/console errors. Keeps a
 * timeout attributable instead of opaque.
 */
async function collectPageDiagnostics(client) {
  const diag = {};
  try {
    diag.location = await evaluate(client, 'location.href');
    diag.title = await evaluate(client, 'document.title');
    diag.bodyExcerpt = await evaluate(
      client,
      "(document.body && document.body.innerText || '').slice(0, 600)",
    );
    diag.hasRootChild = await evaluate(
      client,
      "Boolean(document.getElementById('root') && document.getElementById('root').childElementCount > 0)",
    );
    diag.proofHandlePresent = await evaluate(client, 'Boolean(window.__RFT_NATIVE_PROOF__)');
  } catch (err) {
    diag.error = err instanceof Error ? err.message : String(err);
  }
  return diag;
}

async function captureScreenshot(client, outPath) {
  const shot = await client.send('Page.captureScreenshot', { format: 'png' });
  writeFileSync(outPath, Buffer.from(shot.data, 'base64'));
  return outPath;
}

// ── Evidence writing ────────────────────────────────────────────────────────

function writeEvidence(outDir, filename, payload) {
  mkdirSync(outDir, { recursive: true });
  const path = join(outDir, filename);
  writeFileSync(path, `${JSON.stringify(payload, null, 2)}\n`);
  return path;
}

const MAX_EVIDENCE_TAIL_CHARS = 4_000;

/** Redact fixture-home paths and API-key header values from child output. */
function redactChildOutput(text, fixtureHome) {
  let out = text;
  if (fixtureHome) out = out.split(fixtureHome).join('<fixture-home>');
  out = out.replace(/(X-API-Key\s*[:=]\s*)\S+/gi, '$1<redacted>');
  out = out.replace(/(NEXUS42_DAEMON_API_KEY\s*=\s*)\S+/gi, '$1<redacted>');
  return out;
}
/**
 * Bounded, redacted tail of every tracked child's captured output, keyed by
 * unique child identity (label + pid + spawn sequence). Lets a
 * launch/navigation/service failure be attributed in the failure evidence
 * without dumping unbounded logs; exited children contribute their retained
 * output so a failed launch is still attributable to the exact child.
 */
function collectChildOutput(fixtureHome) {
  const rows = {};
  for (const child of trackedChildren) {
    if (typeof child.output !== 'function') continue;
    const text = child.output();
    if (!text) continue;
    rows[child.__evidenceKey ?? child.__label ?? `pid-${child.pid}`] = redactChildOutput(
      text.slice(-MAX_EVIDENCE_TAIL_CHARS),
      fixtureHome,
    );
  }
  // Closed children left the tracked set at `close`; their retained (already
  // tail-bounded) output is merged so the failure evidence keeps the very
  // child output that explains a launch/service failure. Keys are unique per
  // child, so a silent restart never masks an earlier child's tail.
  for (const [evidenceKey, text] of retainedExitedOutput) {
    if (rows[evidenceKey]) continue;
    rows[evidenceKey] = redactChildOutput(text, fixtureHome);
  }
  return rows;
}

// ── Direct CLI writer ───────────────────────────────────────────────────────

/**
 * Transient admission contention between the standalone service's engine owner
 * and a direct CLI writer surfaces as `workspace writer busy … migration.lock`.
 * Retry ONLY that condition, within a monotonic deadline, rerunning the exact
 * same args. Conflicts (stale CAS), validation, and every other error return
 * immediately — CAS semantics are unchanged.
 */
const WRITER_BUSY_PATTERN = /workspace writer busy|migration\.lock/;
const CLI_WRITER_BUSY_DEADLINE_MS = 20_000;
const CLI_WRITER_BUSY_BACKOFF_MS = 100;

async function runCliPatch(home, { entityId, expectedVersion, title }) {
  const args = [
    'creator', 'world', 'kb', 'entity', 'patch',
    '--world-id', WORLD_ID,
    '--entity-id', entityId,
    '--expected-version', String(expectedVersion),
    '--title', title,
    '--json',
  ];
  const deadline = Date.now() + CLI_WRITER_BUSY_DEADLINE_MS;
  let attempts = 0;
  let writerBusyMs = 0;
  for (;;) {
    attempts += 1;
    const result = spawnSync(CLI_BINARY, args, {
      cwd: repoRoot,
      env: { ...process.env, HOME: home },
      encoding: 'utf8',
    });
    const stderr = result.stderr ?? '';
    const busy = result.status !== 0 && WRITER_BUSY_PATTERN.test(stderr);
    if (!busy || Date.now() >= deadline) {
      return {
        status: result.status,
        stdout: result.stdout ?? '',
        stderr,
        attempts,
        writerBusyMs,
      };
    }
    writerBusyMs += CLI_WRITER_BUSY_BACKOFF_MS;
    await sleep(CLI_WRITER_BUSY_BACKOFF_MS);
  }
}

// ── Phase runners ───────────────────────────────────────────────────────────

async function runBrowserInteractionProof(ctx) {
  const { client, screenshotsDir } = ctx;
  const world = WORLD_ID;

  // Navigation was already performed by the CDP target creation
  // (`/json/new?<pageUrl>`) — the single navigation. Poll the page readiness
  // handle, which tolerates load-before-enable. The bound is deliberately large
  // (180 s): the FIRST load on a cold Chrome/Vite dev module graph may require
  // transforming the whole route graph before the proof page's module executes
  // and publishes `window.__RFT_NATIVE_PROOF__`. This is bounded cold-start
  // readiness, NOT a DX edit metric (the DX-2 latency criterion is measured
  // separately, on a warm page, after readiness). A transient per-evaluation
  // error (the page busy during transform) is retried inside
  // `waitForPageCondition`. On timeout, attach bounded page diagnostics.
  try {
    await waitForPageCondition(
      client,
      'Boolean(window.__RFT_NATIVE_PROOF__ && window.__RFT_NATIVE_PROOF__.ready)',
      { timeoutMs: 180_000 },
    );
  } catch (err) {
    const diagnostics = await collectPageDiagnostics(client);
    throw new Error(
      `${err instanceof Error ? err.message : String(err)}; page diagnostics: ${JSON.stringify(diagnostics)}`,
    );
  }
  await waitForPageCondition(
    client,
    `Boolean(document.querySelector('[data-testid="rft-native-proof-page"]'))`,
  );

  const graphBefore = await evaluate(
    client,
    `window.__RFT_NATIVE_PROOF__.readGraph().then((g) => ({ entities: (g.entities||[]).length, casVersion: (g.entities||[]).filter(e => e.key_block_id === ${JSON.stringify(CAS_ENTITY)}).map(e => e.version)[0] ?? null }))`,
  );
  // Require the EXISTING canvas to actually render entity content in the DOM
  // before screenshotting — React Flow mounts nodes asynchronously and applies
  // fitView on layout, and the headless browser may default to the accessible
  // list view, so an API promise is not visual readiness. Accept either a
  // React-Flow node or an entity-table row that is on-screen and carries a
  // seeded entity name (an offscreen/zero-size element is not meaningful
  // evidence).
  try {
    await waitForPageCondition(
      client,
      `(() => {
        const onScreen = (el) => {
          const r = el.getBoundingClientRect();
          return r.width > 1 && r.height > 1 && r.bottom > 0 && r.right > 0 &&
            r.top < window.innerHeight && r.left < window.innerWidth;
        };
        const nodes = Array.from(document.querySelectorAll('.react-flow__node'));
        const rows = Array.from(document.querySelectorAll('table tbody tr'));
        const candidates = [...nodes, ...rows].filter(onScreen);
        return candidates.some((el) => (el.innerText || '').includes(${JSON.stringify('Cas')}));
      })()`,
      { timeoutMs: 25_000 },
    );
  } catch (err) {
    const diagnostics = await evaluate(
      client,
      `(() => {
        const rect = (el) => { const r = el.getBoundingClientRect(); return { w: Math.round(r.width), h: Math.round(r.height), left: Math.round(r.left), top: Math.round(r.top), right: Math.round(r.right), bottom: Math.round(r.bottom) }; };
        const styleOf = (el) => { const s = getComputedStyle(el); return { display: s.display, visibility: s.visibility, opacity: s.opacity, transform: s.transform }; };
        const nodes = Array.from(document.querySelectorAll('.react-flow__node')).map((n) => ({ id: n.getAttribute('data-id'), text: (n.innerText || '').slice(0, 60), rect: rect(n), style: styleOf(n) }));
        const rows = Array.from(document.querySelectorAll('table tbody tr')).slice(0, 12).map((r) => ({ text: (r.innerText || '').slice(0, 60), rect: rect(r) }));
        const viewport = document.querySelector('.react-flow__viewport');
        const pane = document.querySelector('.react-flow__pane');
        return {
          nodeCount: nodes.length,
          nodes,
          rowCount: rows.length,
          rows,
          viewport: viewport ? { transform: getComputedStyle(viewport).transform, rect: rect(viewport) } : null,
          pane: pane ? { rect: rect(pane) } : null,
          reducedMotion: window.matchMedia('(prefers-reduced-motion: reduce)').matches,
          bodyExcerpt: (document.body.innerText || '').slice(0, 400),
        };
      })()`,
    );
    throw new Error(
      `canvas render wait failed: ${err instanceof Error ? err.message : String(err)}; diagnostics=${JSON.stringify(diagnostics)}`,
    );
  }
  const renderedNodes = await evaluate(
    client,
    `(() => {
      const nodes = Array.from(document.querySelectorAll('.react-flow__node'));
      const rows = Array.from(document.querySelectorAll('table tbody tr'));
      const pick = (el) => { const r = el.getBoundingClientRect(); return { text: (el.innerText||'').slice(0,40), w: Math.round(r.width), h: Math.round(r.height), left: Math.round(r.left), top: Math.round(r.top) }; };
      return { view: nodes.length > 0 ? 'graph' : 'list', nodes: nodes.map(pick), rows: rows.slice(0, 8).map(pick) };
    })()`,
  );
  const graphShot = await captureScreenshot(client, join(screenshotsDir, '01-canvas-graph.png'));

  // Candidates render (the existing canvas always fetches them).
  const candidates = await evaluate(
    client,
    'window.__RFT_NATIVE_PROOF__.readCandidates().then((c) => (c.items||[]).map(i => i.candidate_id))',
  );

  // Create-on-absent through the real BrowserClient in page context, then
  // observe the existing canvas (which refetches via the graph query).
  const created = await evaluate(
    client,
    `window.__RFT_NATIVE_PROOF__.createEntityOnAbsent({ entity_id: ${JSON.stringify(CREATE_ENTITY)}, expected_version: 0, patch: { title: 'Proof Entity', block_type: 'character' } }).then((r) => ({ ok: true, version: r.version, id: r.entity.key_block_id })).catch((e) => ({ ok: false, status: e.status ?? 0, code: e.code ?? null, message: String((e && e.message) || e) }))`,
  );
  // Assert immediately (before polling) so a validation failure is attributable:
  // the created id and version must match, else throw with the serialized body.
  if (!created?.ok || created.id !== CREATE_ENTITY || typeof created.version !== 'number') {
    throw new Error(`create-on-absent did not succeed: ${JSON.stringify(created)}`);
  }
  await waitForPageCondition(
    client,
    `window.__RFT_NATIVE_PROOF__.readGraph().then((g) => (g.entities||[]).some(e => e.key_block_id === ${JSON.stringify(CREATE_ENTITY)}))`,
    { timeoutMs: 15_000 },
  );
  // Require the NEW entity's node to EXIST in the canvas DOM first (any
  // position) — the watcher/cache path, not visibility. Then, because adding a
  // node grows the graph and React Flow preserves the previous viewport
  // (expected production behavior), use the EXISTING fit-view control to
  // reframe, and only then require the created entity on-screen before
  // screenshotting. No production viewport reset and no new UI are added.
  try {
    await waitForPageCondition(
      client,
      `(() => {
        const els = [...document.querySelectorAll('.react-flow__node'), ...document.querySelectorAll('table tbody tr')];
        return els.some((el) => (el.innerText || '').includes('Proof Entity'));
      })()`,
      { timeoutMs: 20_000 },
    );
  } catch (err) {
    const diagnostics = await evaluate(
      client,
      `(async () => {
        const rect = (el) => { const r = el.getBoundingClientRect(); return { w: Math.round(r.width), h: Math.round(r.height), left: Math.round(r.left), top: Math.round(r.top), right: Math.round(r.right), bottom: Math.round(r.bottom) }; };
        const styleOf = (el) => { const s = getComputedStyle(el); return { display: s.display, visibility: s.visibility, opacity: s.opacity, transform: s.transform }; };
        const nodes = Array.from(document.querySelectorAll('.react-flow__node')).map((n) => ({ id: n.getAttribute('data-id'), text: (n.innerText || '').slice(0, 60), rect: rect(n), style: styleOf(n) }));
        const rows = Array.from(document.querySelectorAll('table tbody tr')).slice(0, 12).map((r) => ({ text: (r.innerText || '').slice(0, 60), rect: rect(r) }));
        const viewport = document.querySelector('.react-flow__viewport');
        let graphCreated = null;
        let changes = null;
        try {
          const g = await window.__RFT_NATIVE_PROOF__.readGraph();
          graphCreated = (g.entities || []).find((e) => e.key_block_id === ${JSON.stringify(CREATE_ENTITY)}) ?? null;
        } catch (ge) { graphCreated = { error: String((ge && ge.message) || ge) }; }
        try { changes = await window.__RFT_NATIVE_PROOF__.readCoreChanges({ after_sequence: '0' }); } catch (ce) { changes = { error: String((ce && ce.message) || ce) }; }
        return {
          nodeCount: nodes.length,
          nodes,
          rowCount: rows.length,
          rows,
          viewport: viewport ? { transform: getComputedStyle(viewport).transform, rect: rect(viewport) } : null,
          reducedMotion: window.matchMedia('(prefers-reduced-motion: reduce)').matches,
          graphCreated,
          coreChanges: changes,
          bodyExcerpt: (document.body.innerText || '').slice(0, 400),
        };
      })()`,
    );
    throw new Error(
      `created entity absent from canvas DOM: ${err instanceof Error ? err.message : String(err)}; diagnostics=${JSON.stringify(diagnostics)}`,
    );
  }
  // Reframe through the EXISTING React Flow control (production UI), then
  // require the created entity on-screen.
  const fitViewControl = await evaluate(
    client,
    `(() => {
      const btn = document.querySelector('.react-flow__controls-fitview');
      if (!btn) return { found: false };
      const r = btn.getBoundingClientRect();
      const s = getComputedStyle(btn);
      return { found: true, visible: r.width > 0 && r.height > 0 && s.visibility !== 'hidden' && s.display !== 'none' };
    })()`,
  );
  if (!fitViewControl?.found || !fitViewControl.visible) {
    throw new Error(`React Flow fit-view control unavailable: ${JSON.stringify(fitViewControl)}`);
  }
  await evaluate(
    client,
    `(() => { const btn = document.querySelector('.react-flow__controls-fitview'); btn.click(); return true; })()`,
  );
  const createVisible = await waitForPageCondition(
    client,
    `(() => {
      const onScreen = (el) => {
        const r = el.getBoundingClientRect();
        return r.width > 1 && r.height > 1 && r.bottom > 0 && r.right > 0 &&
          r.top < window.innerHeight && r.left < window.innerWidth;
      };
      const els = [...document.querySelectorAll('.react-flow__node'), ...document.querySelectorAll('table tbody tr')];
      return els.filter(onScreen).some((el) => (el.innerText || '').includes('Proof Entity'));
    })()`,
    { timeoutMs: 20_000 },
  ).then(() => true).catch(() => false);
  const fitViewOutcome = { control: fitViewControl, visibleAfterFit: createVisible };
  const createShot = await captureScreenshot(client, join(screenshotsDir, '02-create-on-absent.png'));

  // Update via the browser at the observed current version.
  const casVersion = graphBefore.casVersion;
  const updateViaBrowser = await evaluate(
    client,
    `window.__RFT_NATIVE_PROOF__.client.worldKbPatchEntity(${JSON.stringify(world)}, { entity_id: ${JSON.stringify(CAS_ENTITY)}, expected_version: ${casVersion}, patch: { title: 'BROWSER-UPDATED' } }).then((r) => ({ ok: true, version: r.version })).catch((e) => ({ ok: false, status: e.status ?? 0, code: e.code ?? String((e && e.message) || e) }))`,
  );
  const readCasVersionExpr = `window.__RFT_NATIVE_PROOF__.readGraph().then((g) => (g.entities||[]).filter(e => e.key_block_id === ${JSON.stringify(CAS_ENTITY)}).map(e => e.version)[0] ?? null)`;
  const postUpdateVersion = await evaluate(client, readCasVersionExpr);

  // DB-1 locked protocol (proof matrix §1/§2): 100 deterministic competing-write
  // pairs. Each pair is a real two-writer barrier — the next expected version
  // is observed from canonical state, then the browser (BrowserClient in the
  // page) and the direct CLI writer race at that SAME expected version.
  // Exactly one wins; the loser must observe the typed conflict (HTTP 409 /
  // CLI exit 76) and the committed version must advance by exactly one per
  // pair (no lost commit). The criterion verdict is derived in
  // `deriveDb1Criterion` from the COMPLETE sample only.
  const racePairs = [];
  {
    let expectedVersion = postUpdateVersion;
    for (let index = 0; index < DB1_REQUIRED_PAIRS; index += 1) {
      const raceBrowser = evaluate(
        client,
        `window.__RFT_NATIVE_PROOF__.client.worldKbPatchEntity(${JSON.stringify(world)}, { entity_id: ${JSON.stringify(CAS_ENTITY)}, expected_version: ${expectedVersion}, patch: { title: 'RACE-BROWSER-${index}' } }).then((r) => ({ winner: 'browser', status: 200, version: r.version })).catch((e) => ({ winner: 'none', status: e.status ?? 0, code: e.code ?? null }))`,
      );
      const cliPromise = runCliPatch(ctx.home, {
        entityId: CAS_ENTITY,
        expectedVersion,
        title: `RACE-CLI-${index}`,
      });
      const pairStartedAt = performance.now();
      const browserRaceResult = await raceBrowser;
      const cli = await cliPromise;
      const pairMs = performance.now() - pairStartedAt;
      const browserWon = browserRaceResult.status === 200;
      const cliWon = cli.status === 0;
      if (browserWon === cliWon) {
        // Neither or both writers won: record the malformed round verbatim and
        // stop — the derivation fails the criterion, never fabricates a winner.
        racePairs.push({
          index,
          expectedVersion,
          resultingVersion: null,
          winner: browserWon ? 'both' : 'none',
          browserStatus: browserRaceResult.status,
          cliStatus: cli.status,
          cliAttempts: cli.attempts,
          pairMs,
        });
        break;
      }
      const resultingVersion = await evaluate(client, readCasVersionExpr);
      racePairs.push({
        index,
        expectedVersion,
        resultingVersion,
        winner: browserWon ? 'browser' : 'cli',
        browserStatus: browserRaceResult.status,
        cliStatus: cli.status,
        cliAttempts: cli.attempts,
        pairMs,
      });
      if (resultingVersion !== expectedVersion + 1) {
        // Lost commit or double bump: stop; the malformed sample fails the row.
        break;
      }
      expectedVersion = resultingVersion;
    }
  }

  // Prove the direct CLI writer becomes visible through the canvas watermark:
  // the CLI (already run above) wins the CAS; then a second CLI-only write lands
  // while the browser is idle, and the canvas — which watches the durable
  // `core_changes` outbox — must reflect the CLI's title in the rendered DOM
  // without any browser-side mutation. This observes canonical state, not a
  // promise resolution.
  const cliTitle = 'CLI-WATERMARK-VISIBLE';
  const canonicalBeforeCli = await evaluate(
    client,
    `window.__RFT_NATIVE_PROOF__.readGraph().then((g) => (g.entities||[]).filter(e => e.key_block_id === ${JSON.stringify(CAS_ENTITY)}).map(e => e.canonical_name)[0] ?? null)`,
  );
  const cliVersion = await evaluate(client, readCasVersionExpr);
  const cliWrite = await runCliPatch(ctx.home, {
    entityId: CAS_ENTITY,
    expectedVersion: cliVersion,
    title: cliTitle,
  });
  // 1. The CLI write must succeed — otherwise nothing downstream is meaningful.
  if (cliWrite.status !== 0) {
    throw new Error(
      `CLI watermark write failed: status=${cliWrite.status} cliVersion=${cliVersion} stdout=${cliWrite.stdout.trim()} stderr=${cliWrite.stderr.trim()}`,
    );
  }
  // 2. First prove CANONICAL visibility: poll `readGraph()` until the entity's
  //    canonical_name equals the CLI title. This isolates a data-propagation
  //    failure (service/core/DB) from a render failure before the DOM check.
  let graphCanonical = null;
  try {
    graphCanonical = await waitForPageCondition(
      client,
      `window.__RFT_NATIVE_PROOF__.readGraph().then((g) => (g.entities||[]).filter(e => e.key_block_id === ${JSON.stringify(CAS_ENTITY)}).map(e => e.canonical_name)[0] ?? null).then((n) => n === ${JSON.stringify(cliTitle)} ? n : false)`,
      { timeoutMs: 20_000 },
    );
  } catch (err) {
    throw new Error(
      `CLI watermark canonical visibility failed: ${err instanceof Error ? err.message : String(err)}; cliWrite=${JSON.stringify({ status: cliWrite.status, stdout: cliWrite.stdout.trim().slice(0, 400) })}`,
    );
  }
  // 3. Then require RENDERED DOM visibility (never weakened). On timeout, report
  //    the CLI result, the latest graph entity/version/title, and the latest
  //    core_changes page so the failure is attributable.
  let canvasText = null;
  try {
    canvasText = await waitForPageCondition(
      client,
      `(document.body.innerText || '').includes(${JSON.stringify(cliTitle)}) ? ${JSON.stringify(cliTitle)} : false`,
      { timeoutMs: 20_000 },
    );
  } catch (err) {
    const latest = await evaluate(
      client,
      `(async () => {
        const g = await window.__RFT_NATIVE_PROOF__.readGraph();
        const e = (g.entities||[]).find((x) => x.key_block_id === ${JSON.stringify(CAS_ENTITY)});
        let changes = null;
        try { changes = await window.__RFT_NATIVE_PROOF__.readCoreChanges({ after_sequence: '0' }); } catch (ce) { changes = { error: String((ce && ce.message) || ce) }; }
        return { graph: { canonical_name: e?.canonical_name ?? null, version: e?.version ?? null }, changes };
      })()`,
    );
    throw new Error(
      `CLI watermark DOM visibility failed: ${err instanceof Error ? err.message : String(err)}; cliWrite=${JSON.stringify({ status: cliWrite.status, stdout: cliWrite.stdout.trim().slice(0, 400) })}; latest=${JSON.stringify(latest)}`,
    );
  }
  const watermarkOutcome = {
    cliWrite: {
      status: cliWrite.status,
      stdout: cliWrite.stdout.trim().slice(0, 400),
      attempts: cliWrite.attempts,
      writerBusyMs: cliWrite.writerBusyMs,
    },
    cliTitle,
    canonicalBeforeCli,
    graphCanonical,
    canvasText,
    visible: canvasText === cliTitle,
  };

  return {
    graphBefore,
    renderedNodes,
    graphShot,
    candidates,
    created,
    createShot,
    updateViaBrowser,
    racePairs,
    watermarkOutcome,
    fitViewOutcome,
  };
}

async function runProviderProof(ctx) {
  const { client } = ctx;
  const session = await evaluate(
    client,
    'window.__RFT_NATIVE_PROOF__.createAgentHostSession({ provider_id: "mock-acp" })',
  );
  const sessionId = session.session_id;
  const operation = await evaluate(
    client,
    `window.__RFT_NATIVE_PROOF__.executeAgentHostOperation(${JSON.stringify(sessionId)}, { kind: "prompt", content: "hello" })`,
  );
  const events = await evaluate(
    client,
    `window.__RFT_NATIVE_PROOF__.drainAgentHostEvents(${JSON.stringify(sessionId)}, { timeoutMs: 30000 })`,
  );
  const terminal = events.find((e) => e.OpFinished ?? e.OpFailed ?? e.SessionStopped) ?? null;
  const message = events
    .map((e) => e.MessageDelta?.text ?? e.ThoughtDelta?.text ?? null)
    .filter(Boolean)
    .join('');
  const inspect = await evaluate(
    client,
    `window.__RFT_NATIVE_PROOF__.getAgentHostOperation(${JSON.stringify(operation.operation_id)})`,
  );
  return { session, operation, terminal, message, inspect, eventCount: events.length };
}

async function runCancelProof(ctx, serviceHandle) {
  const { client } = ctx;
  // LIFE-2 locked protocol (proof matrix §2): 10 cooperative ACP cancel
  // cycles against the real TS ACP SDK adapter and the blocking protocol
  // fixture. Each cycle measures acknowledgment→terminal from the driver
  // send of the cancel (monotonic `performance.now()`), with the terminal
  // observed through a real GET — a cancel ack with a stale `running` GET is
  // a native defect this criterion must expose. The verdict is derived in
  // `deriveCancelCriterion` from the COMPLETE cycle sample only.
  const cycles = [];
  for (let index = 0; index < LIFE_REQUIRED_CYCLES; index += 1) {
    const session = await evaluate(
      client,
      'window.__RFT_NATIVE_PROOF__.createAgentHostSession({ provider_id: "mock-acp" })',
    );
    const operation = await evaluate(
      client,
      `window.__RFT_NATIVE_PROOF__.executeAgentHostOperation(${JSON.stringify(session.session_id)}, { kind: "prompt", content: "block" })`,
    );
    await sleep(750);
    const cancelStartedAt = performance.now();
    const cancel = await evaluate(
      client,
      `window.__RFT_NATIVE_PROOF__.cancelAgentHostOperation(${JSON.stringify(operation.operation_id)})`,
    );
    const deadline = Date.now() + 20_000;
    let status = null;
    for (;;) {
      const inspect = await jsonRequest(
        `${serviceHandle.url}/v1/daemon/agent-host/operations/${operation.operation_id}`,
      );
      status = inspect.payload?.status ?? null;
      if (status && status !== 'started' && status !== 'running') break;
      if (Date.now() >= deadline) break;
      await sleep(100);
    }
    cycles.push({
      index,
      sessionId: session.session_id,
      operationId: operation.operation_id,
      cancel,
      cancelAcknowledged: cancel?.status === 'cancelled',
      observedStatus: status,
      settled: status === 'cancelled',
      cancelAckToTerminalMs: performance.now() - cancelStartedAt,
    });
  }
  return { cycles };
}

/**
 * LIFE-3 locked protocol (proof matrix §2): abrupt-restart cycles. Each cycle
 * SIGKILLs the service's WHOLE process group through the shared
 * `signalProcessGroup` path (the immediate-pid fallback fires only on ESRCH),
 * verifies via `ps` that NO group descendant survived (QC3-C3: an unconditional
 * pid fallback used to hide survivors), then restarts and requires the
 * previously active non-resumable operation to be reported `interrupted`.
 * Every wait is bounded; any surviving descendant is recorded in the cycle row
 * and fails the criterion via `deriveRestartCriterion`.
 */
async function runInterruptedRestartCycles(ctx, serviceHandle, { cycles = LIFE_REQUIRED_CYCLES } = {}) {
  const rows = [];
  let handle = serviceHandle;
  for (let index = 0; index < cycles; index += 1) {
    const session = await evaluate(
      ctx.client,
      'window.__RFT_NATIVE_PROOF__.createAgentHostSession({ provider_id: "mock-acp" })',
    );
    const operation = await evaluate(
      ctx.client,
      `window.__RFT_NATIVE_PROOF__.executeAgentHostOperation(${JSON.stringify(session.session_id)}, { kind: "prompt", content: "block" })`,
    );
    await sleep(750);
    // Snapshot the tracked tree BEFORE the kill: once the group leader dies,
    // orphans are re-parented and a fresh ps walk could no longer find them.
    const groupBefore = collectDescendants([handle.child.pid], psRows()).filter(
      (row) => row.pid !== handle.child.pid,
    );
    const killRequestedAt = performance.now();
    let groupKillError = null;
    try {
      signalProcessGroup(handle.child.pid, 'SIGKILL');
    } catch (err) {
      groupKillError = err instanceof Error ? `${err.name}: ${err.message}` : String(err);
    }
    // `__closed` is registered at spawn time, so it cannot miss an early exit;
    // the bound keeps this abrupt-restart proof finite even if inherited stdio
    // delays `close`. Port release below remains the authoritative stop check.
    await Promise.race([handle.child.__closed ?? Promise.resolve(), sleep(5_000)]);
    const closedSettled = handle.child.__closeSettled === true;
    // Bounded survivor sweep: one extra group SIGKILL, one bounded settle
    // window, then the verdict rows. A survivor here is evidence, never a
    // silent pass.
    let groupSurvivors = survivingGroupRows(groupBefore, handle.child.pid);
    if (groupSurvivors.length > 0) {
      try {
        signalProcessGroup(handle.child.pid, 'SIGKILL');
      } catch (err) {
        groupKillError = groupKillError ?? (err instanceof Error ? `${err.name}: ${err.message}` : String(err));
      }
      await Promise.race([handle.child.__closed ?? Promise.resolve(), sleep(1_000)]);
      groupSurvivors = survivingGroupRows(groupBefore, handle.child.pid);
    }
    await waitForPortFree(handle.port);
    handle = await restartService(handle, { providerEnv: { BLOCK_PROMPT: '1' } });
    const observed = await observeInterrupted(handle, operation.operation_id);
    rows.push({
      index,
      sessionId: session.session_id,
      operationId: operation.operation_id,
      groupKillError,
      closedSettled,
      abruptCloseSettledMs: closedSettled ? performance.now() - killRequestedAt : null,
      groupDescendantsBefore: groupBefore.length,
      groupSurvivors,
      restartReadyMs: handle.readyMs,
      httpStatus: observed.httpStatus,
      observedStatus: observed.status,
    });
  }
  return { rows, service: handle };
}

async function waitForPortFree(port, { timeoutMs = 15_000 } = {}) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      await httpRequest(`http://127.0.0.1:${port}/v1/daemon/runtime/health`);
    } catch {
      return;
    }
    await sleep(100);
  }
  throw new Error(`port ${port} did not free within ${timeoutMs}ms`);
}

/**
 * After restart, read the operation via native hostQuery: a previously active
 * non-resumable operation must be reported `interrupted`.
 */
async function observeInterrupted(serviceHandle, operationId) {
  const inspect = await jsonRequest(
    `${serviceHandle.url}/v1/daemon/agent-host/operations/${operationId}`,
  );
  return { status: inspect.payload?.status ?? null, httpStatus: inspect.status };
}

// ── Edit loop (DX-1/DX-2) ────────────────────────────────────────────────────

/** Snapshot process-tree Cargo/native traces for a root pid set. */
function psRows() {
  const out = execFileSync('ps', ['-eo', 'pid=,ppid=,command='], { encoding: 'utf8' });
  return out
    .split('\n')
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line) => {
      const match = line.match(/^(\d+)\s+(\d+)\s+(.*)$/);
      return match ? { pid: Number(match[1]), ppid: Number(match[2]), command: match[3] } : null;
    })
    .filter(Boolean);
}

function collectDescendants(rootPids, rows) {
  const byParent = new Map();
  for (const row of rows) {
    const list = byParent.get(row.ppid) ?? [];
    list.push(row);
    byParent.set(row.ppid, list);
  }
  const seen = new Set();
  const queue = [...rootPids];
  while (queue.length) {
    const pid = queue.shift();
    if (seen.has(pid)) continue;
    seen.add(pid);
    for (const child of byParent.get(pid) ?? []) queue.push(child.pid);
  }
  return rows.filter((row) => seen.has(row.pid));
}

function countCargoTraces(rootPids) {
  const rows = collectDescendants(rootPids, psRows());
  return rows.filter((row) => commandLineMatchesCargo(row.command)).length;
}

/**
 * Members of the snapshotted process tree (excluding the killed leader) whose
 * exact pid AND command line still exist. The command line must match too: a
 * bare pid match could be pid reuse inside the observation window.
 */
function survivingGroupRows(groupBefore, leaderPid) {
  const rows = psRows();
  return groupBefore.filter(
    (row) =>
      row.pid !== leaderPid && rows.some((r) => r.pid === row.pid && r.command === row.command),
  );
}

/**
 * Trace Cargo/native child processes while `work()` runs. `resolveRootPids` is
 * a pid array or a zero-arg function re-resolved on every sample. A sampling
 * failure or a zero-sample window FAILS the trace: a `cargoTraceCount === 0`
 * claim requires observed samples, never a blind spot.
 */

/**
 * Current traced root set for a service-edit window: the proof runner plus the
 * LIVE service child. Resolved per sample, because `restartServiceFromEdit`
 * replaces the service process mid-window and the replacement must stay inside
 * the traced root set.
 */
function liveServiceRootPids(ctx) {
  const roots = [process.pid];
  const child = ctx.service?.child;
  if (child?.pid && child.exitCode === null && child.signalCode === null) {
    roots.push(child.pid);
  }
  return roots;
}

async function traceDuringEdit(resolveRootPids, work, { intervalMs = 50 } = {}) {
  const samples = [];
  let samplingError = null;
  let running = true;
  const sampler = (async () => {
    while (running) {
      try {
        // Re-resolved on EVERY sample: a restart that replaces the service
        // process mid-window keeps the replacement inside the traced roots.
        const roots = typeof resolveRootPids === 'function' ? resolveRootPids() : resolveRootPids;
        samples.push(countCargoTraces(roots));
      } catch (err) {
        samplingError = err instanceof Error ? err : new Error(String(err));
        break;
      }
      await sleep(intervalMs);
    }
  })();
  try {
    const result = await work();
    if (samplingError) {
      throw new Error(`cargo trace sampling failed during edit: ${samplingError.message}`);
    }
    if (samples.length === 0) {
      throw new Error('cargo trace sampling collected no samples; a zero-Cargo claim is unproven');
    }
    return { result, cargoSamples: samples };
  } finally {
    running = false;
    await sampler.catch(() => undefined);
  }
}

/** Byte-preserving tracked edit that restores the file even on failure. */
class TrackedEdit {
  constructor(absPath) {
    this.absPath = absPath;
    this.original = null;
    this.existed = false;
  }
  capture() {
    this.existed = existsSync(this.absPath);
    this.original = this.existed ? readFileSync(this.absPath) : null;
    return this;
  }
  write(content) {
    writeFileSync(this.absPath, content, 'utf8');
  }
  restore() {
    if (this.existed && this.original) writeFileSync(this.absPath, this.original);
    else if (!this.existed && existsSync(this.absPath)) rmSync(this.absPath, { force: true });
  }
  isRestored() {
    if (this.existed && this.original) return readFileSync(this.absPath).equals(this.original);
    return !existsSync(this.absPath);
  }
}

/**
 * Browser/shared-UI edit sample: rewrite the dev-only marker module and wait
 * for the running Vite page to observe the new value (HMR, no full reload
 * required). Measures edit→visible and counts Cargo traces throughout.
 */
async function runWebEditSample(ctx, sampleIndex) {
  const { client } = ctx;
  const markerPath = resolve(webRoot, 'src/pages/rft-native-proof-marker.ts');
  const edit = new TrackedEdit(markerPath).capture();
  const value = `sample-${sampleIndex}`;
  const source = `/**\n * P4-T3 development-only proof marker (RFT-M1 browser vertical).\n *\n * Runner-mutated for the DX-2 edit sample; restored byte-identically.\n */\nexport const RFT_NATIVE_PROOF_MARKER = ${JSON.stringify(value)};\n`;
  try {
    const rootPids = [ctx.vite.child.pid];
    const started = Date.now();
    const { cargoSamples } = await traceDuringEdit(rootPids, async () => {
      edit.write(source);
      await waitForPageCondition(
        client,
        `(document.querySelector('[data-testid="rft-native-proof-marker"]')?.textContent ?? '').includes(${JSON.stringify(value)})`,
        { timeoutMs: 5_000 },
      );
    });
    return {
      surface: 'browser-shared-ui',
      file: markerPath,
      visibleMs: Date.now() - started,
      cargoTraceCount: cargoSamples.reduce((a, b) => Math.max(a, b), 0),
    };
  } finally {
    edit.restore();
    if (!edit.isRestored()) throw new Error(`marker file not restored: ${markerPath}`);
    // A successful restore must be observable: BOTH the baseline marker text in
    // the DOM AND the proof handle back at ready. A failed restore is NOT
    // swallowed — it fails the run, because the next sample would otherwise run
    // against a stale/frozen handle.
    await waitForPageCondition(
      client,
      `(document.querySelector('[data-testid="rft-native-proof-marker"]')?.textContent ?? '').includes('baseline')`,
      { timeoutMs: 10_000 },
    );
    await waitForPageCondition(
      client,
      'Boolean(window.__RFT_NATIVE_PROOF__ && window.__RFT_NATIVE_PROOF__.ready)',
      { timeoutMs: 30_000 },
    );
  }
}

/**
 * TS route-transformation edit sample: rewrite the service's request-transform
 * source, restart the service under `tsx` (no compile), wait for readiness, and
 * observe the edited behavior over real HTTP. Counts Cargo traces throughout.
 */
async function runRouteEditSample(ctx) {
  const targetPath = resolve(serviceRoot, 'src/world-kb.ts');
  const edit = new TrackedEdit(targetPath).capture();
  const originalSource = edit.original.toString('utf8');
  // A real, reversible request-transformation change: lower the candidates
  // pagination default from 50 to 1. The locked fixture seeds exactly TWO
  // pending candidates, so the edited behavior is OBSERVABLE — after restart
  // `GET .../kb/candidates` (no `limit`) must return `pagination.limit === 1`
  // AND a single item, versus the restored baseline of limit 50 / two items.
  // Asserting the response shape (not a source marker) proves the running
  // service actually recompiled and served the transformed code.
  const editedSource = originalSource.replace(
    '{ defaultLimit = 50, max = 250 }: { defaultLimit?: number; max?: number } = {}',
    '{ defaultLimit = 1, max = 250 }: { defaultLimit?: number; max?: number } = {}',
  );
  if (editedSource === originalSource) {
    throw new Error('route edit anchor not found in world-kb.ts');
  }
  try {
    edit.write(editedSource);
    let restartMs = null;
    const { cargoSamples } = await traceDuringEdit(() => liveServiceRootPids(ctx), async () => {
      const restarted = await restartServiceFromEdit(ctx);
      restartMs = restarted.readyMs;
    });
    await waitForHttpOk(`${ctx.serviceUrl}/v1/daemon/runtime/health`, { timeoutMs: 30_000 });
    const candidates = await jsonRequest(
      `${ctx.serviceUrl}/v1/daemon/worlds/${WORLD_ID}/kb/candidates`,
    );
    const observedLimit = candidates.payload?.pagination?.limit ?? null;
    const observedCount = candidates.payload?.items?.length ?? null;
    return {
      surface: 'ts-route-transformation',
      file: targetPath,
      restartMs,
      servedStatus: candidates.status,
      observedLimit,
      observedCount,
      expectedLimit: 1,
      expectedCount: 1,
      editObserved: observedLimit === 1 && observedCount === 1,
      cargoTraceCount: cargoSamples.reduce((a, b) => Math.max(a, b), 0),
    };
  } finally {
    edit.restore();
    if (!edit.isRestored()) throw new Error(`route file not restored: ${targetPath}`);
    await restartServiceFromEdit(ctx);
    // Confirm the restored baseline behavior is back (limit 50, two items).
    const restored = await jsonRequest(
      `${ctx.serviceUrl}/v1/daemon/worlds/${WORLD_ID}/kb/candidates`,
    );
    const restoredLimit = restored.payload?.pagination?.limit;
    const restoredCount = restored.payload?.items?.length;
    if (restoredLimit !== 50 || restoredCount !== 2) {
      throw new Error(
        `route edit did not restore baseline behavior: limit=${restoredLimit} count=${restoredCount}`,
      );
    }
  }
}

/**
 * Restart the service from the current on-disk source (tsx) and record the
 * spawn→ready latency. Used by the TS edit samples.
 */
async function restartServiceFromEdit(ctx) {
  await stopService(ctx.service);
  await waitForPortFree(ctx.port);
  const next = await startService({ home: ctx.home, port: ctx.port, env: ctx.serviceEnv });
  next.logPath = ctx.logPath;
  ctx.service = next;
  ctx.serviceUrl = next.url;
  return next;
}

// ── Adapter edit restore gate ───────────────────────────────────────────────

/**
 * The real provider session an adapter sample runs in the page: create a
 * session, prompt once, drain all events, and project the streamed message
 * text plus terminal settlement. Shared verbatim by the edited-adapter
 * evaluation and the restored-baseline assertion so both observe the SAME
 * provider vertical.
 */
const ADAPTER_SESSION_SCRIPT = `(async () => {
  const h = window.__RFT_NATIVE_PROOF__;
  const s = await h.createAgentHostSession({ provider_id: 'mock-acp' });
  const op = await h.executeAgentHostOperation(s.session_id, { kind: 'prompt', content: 'hello' });
  const events = await h.drainAgentHostEvents(s.session_id, { timeoutMs: 30000 });
  const text = events.map(e => (e.MessageDelta && e.MessageDelta.text) || (e.ThoughtDelta && e.ThoughtDelta.text) || '').join('');
  const terminal = events.some(e => e.OpFinished || e.OpFailed);
  return { text, terminal, op: op.operation_id };
})()`;

/**
 * Enforced restore rebuild of the adapter package: a nonzero exit or a spawn
 * error throws with a bounded output tail. The adapter edit sample must never
 * continue past a failed restore — the stale edited `dist` would keep serving
 * the edited behavior while the proof reports success.
 */
function requireRestoredAdapterBuild() {
  const restore = spawnSync('pnpm', ['-F', '@42ch/nexus-provider-acp', 'run', 'build'], {
    cwd: repoRoot,
    encoding: 'utf8',
    timeout: 60_000,
    killSignal: 'SIGKILL',
  });
  if (restore.error || restore.status !== 0) {
    const tail = `${restore.stdout ?? ''}${restore.stderr ?? ''}`.slice(-MAX_EVIDENCE_TAIL_CHARS);
    throw new Error(
      `adapter restore build failed (exit ${restore.status}${restore.error ? `: ${restore.error.message}` : ''}): ${tail}`,
    );
  }
}

/**
 * Consumer-observable restored baseline: after the enforced restore rebuild
 * and service restart, a fresh provider session must reach a terminal event
 * WITHOUT the edit marker. Proves the restored runnable output actually
 * serves — a stale or failed restore cannot masquerade as a successful proof.
 */
async function assertRestoredAdapterBaseline(ctx, marker) {
  await waitForHttpOk(`${ctx.serviceUrl}/v1/daemon/runtime/health`, { timeoutMs: 30_000 });
  await waitForPageCondition(
    ctx.client,
    'Boolean(window.__RFT_NATIVE_PROOF__ && window.__RFT_NATIVE_PROOF__.ready)',
    { timeoutMs: 60_000 },
  );
  const stream = await evaluate(ctx.client, ADAPTER_SESSION_SCRIPT, { timeoutMs: 45_000 });
  if (!stream?.terminal) {
    throw new Error(
      `restored adapter baseline did not reach a terminal event: ${JSON.stringify(stream ?? null).slice(0, 400)}`,
    );
  }
  if (typeof stream.text === 'string' && stream.text.includes(marker)) {
    throw new Error(
      `restored adapter still emits the edited marker ${marker}: ${stream.text.slice(0, 200)}`,
    );
  }
}

/**
 * TS SDK adapter edit sample: rewrite the real TS ACP adapter source marker,
 * rebuild the adapter package with its existing tooling, restart the service,
 * and re-run a real provider session so the changed adapter is exercised.
 * Counts Cargo traces throughout (must be 0).
 */
async function runAdapterEditSample(ctx, sampleIndex) {
  const targetPath = resolve(repoRoot, 'packages/nexus-provider-acp/src/acp.ts');
  const edit = new TrackedEdit(targetPath).capture();
  const originalSource = edit.original.toString('utf8');
  // A real, reversible transformation in the TS ACP adapter's event mapping:
  // prefix every agent-message-chunk text with a marker. The edited behavior is
  // then asserted through the REAL provider vertical — a fresh browser-driven
  // provider session must stream a MessageDelta whose text carries the marker
  // (the baseline emits the raw fixture text).
  const marker = `RFTADAPTER${sampleIndex}:`;
  const editedSource = originalSource.replace(
    "    const text = record.content.text ?? '';\n    return { MessageDelta: { session_id: sessionId, op_id: opId, text } };",
    `    const text = ${JSON.stringify(marker)} + (record.content.text ?? '');\n    return { MessageDelta: { session_id: sessionId, op_id: opId, text } };`,
  );
  if (editedSource === originalSource) {
    throw new Error('adapter edit anchor not found in acp.ts');
  }
  try {
    edit.write(editedSource);
    let restartMs = null;
    const { cargoSamples } = await traceDuringEdit(() => liveServiceRootPids(ctx), async () => {
      // Rebuild the adapter package from the edited source with its own build
      // script (tsup is a TS build, never a Cargo/native one). The build runs
      // as a tracked async child: a synchronous build would block this event
      // loop and blind the sampler for the whole build window.
      const build = spawnLogged(
        'pnpm',
        ['-F', '@42ch/nexus-provider-acp', 'run', 'build'],
        { cwd: repoRoot, label: `adapter-build-${sampleIndex}` },
      );
      // A failed spawn never emits `exit`; racing the settled lifecycle
      // (which settles on `error`) keeps a build-launch failure bounded and
      // reportable instead of hanging the sample forever.
      let buildTimer;
      const buildTimeout = new Promise((resolveTimeout) => {
        buildTimer = setTimeout(() => resolveTimeout('timeout'), 60_000);
      });
      let exitCode;
      try {
        exitCode = await Promise.race([
          new Promise((resolveExit) => {
            if (build.exitCode !== null || build.signalCode !== null) {
              resolveExit(build.exitCode);
              return;
            }
            build.once('exit', resolveExit);
          }),
          build.__closed.then(() => build.exitCode),
          buildTimeout,
        ]);
      } finally {
        clearTimeout(buildTimer);
      }
      if (exitCode === 'timeout') {
        await stopChild(build, { graceMs: 0, killMs: 2_000, closeMs: 2_000 });
        throw new Error(
          `adapter package build timed out after 60000ms: ${build.output().slice(-MAX_EVIDENCE_TAIL_CHARS)}`,
        );
      }
      if (exitCode !== 0) {
        throw new Error(
          `adapter package build failed (exit ${exitCode}${build.__error ? `: ${build.__error.message}` : ''}): ${build.output().slice(-MAX_EVIDENCE_TAIL_CHARS)}`,
        );
      }
      const restarted = await restartServiceFromEdit(ctx);
      restartMs = restarted.readyMs;
    });
    await waitForHttpOk(`${ctx.serviceUrl}/v1/daemon/runtime/health`, { timeoutMs: 30_000 });
    // The service restart can leave the page briefly without a live handle;
    // wait explicitly before the adapter evaluation so the failure is a real
    // adapter defect, not a missing handle.
    await waitForPageCondition(
      ctx.client,
      'Boolean(window.__RFT_NATIVE_PROOF__ && window.__RFT_NATIVE_PROOF__.ready)',
      { timeoutMs: 60_000 },
    );
    // Re-run a real provider session in the page and assert the transformed
    // adapter output is what actually streamed. The CDP send timeout must
    // exceed the inner bounded drain (30 s) so a slow-but-bounded drain can
    // resolve/report rather than being cut off by the transport.
    let stream;
    try {
      stream = await evaluate(ctx.client, ADAPTER_SESSION_SCRIPT, { timeoutMs: 45_000 });
    } catch (err) {
      const pageState = await evaluate(
        ctx.client,
        `({ handle: Boolean(window.__RFT_NATIVE_PROOF__), ready: Boolean(window.__RFT_NATIVE_PROOF__ && window.__RFT_NATIVE_PROOF__.ready), location: location.href })`,
        { awaitPromise: false },
      ).catch((e) => ({ error: String(e?.message ?? e) }));
      throw new Error(
        `adapter provider evaluation failed: ${err instanceof Error ? err.message : String(err)}; pageState=${JSON.stringify(pageState)}; serviceOutput=${JSON.stringify(redactChildOutput((ctx.service?.child?.output?.() ?? '').slice(-MAX_EVIDENCE_TAIL_CHARS), ctx.home))}`,
      );
    }
    return {
      surface: 'ts-sdk-adapter',
      file: targetPath,
      restartMs,
      adapterMarker: marker,
      streamedText: stream.text.slice(0, 200),
      editObserved: typeof stream.text === 'string' && stream.text.includes(marker),
      terminal: stream.terminal,
      cargoTraceCount: cargoSamples.reduce((a, b) => Math.max(a, b), 0),
    };
  } finally {
    edit.restore();
    if (!edit.isRestored()) throw new Error(`adapter file not restored: ${targetPath}`);
    // Restore gates, in order: the rebuild must SUCCEED before anything is
    // restarted (a failed rebuild leaves the edited dist serving), then the
    // restored adapter must observably serve the BASELINE (no edit marker)
    // before this sample can report success. A restore failure therefore
    // fails the proof before any PASS can be recorded.
    requireRestoredAdapterBuild();
    await restartServiceFromEdit(ctx);
    await assertRestoredAdapterBaseline(ctx, marker);
  }
}

// ── Acceptance derivations (pure; extracted verbatim by focused tests) ──────

// Locked sampling protocol (proof matrix §1/§2): DB-1 requires 100
// deterministic competing-write pairs; LIFE-2/LIFE-3 require 10 cycles each.
// A fixed acceptance row may claim `pass` ONLY from the complete locked count
// of typed outcomes — a smaller sample is recorded as incomplete, never
// silently green.
const DB1_REQUIRED_PAIRS = 100;
const LIFE_REQUIRED_CYCLES = 10;
const LIFE2_CANCEL_ACK_P95_MAX_MS = 2_000;
const LIFE2_CANCEL_ACK_MAX_MS = 3_000;
const LIFE3_RESTART_READY_MAX_MS = 5_000;

/**
 * One DB-1 sample: exactly one expected-version winner with the TYPED loser
 * conflict (browser HTTP 409 / CLI exit 76), and the committed version must
 * have advanced by exactly one (no lost commit).
 */
function isValidDb1Pair(pair) {
  if (pair === null || typeof pair !== 'object') return false;
  const typedOutcome =
    pair.winner === 'browser'
      ? pair.browserStatus === 200 && pair.cliStatus === 76
      : pair.winner === 'cli' && pair.cliStatus === 0 && pair.browserStatus === 409;
  return typedOutcome && pair.resultingVersion === pair.expectedVersion + 1;
}

/**
 * DB-1 verdict from the full competing-write sample. `pass` requires the
 * COMPLETE locked count where every pair is typed and version-continuous.
 */
function deriveDb1Criterion(pairs, requiredPairs = DB1_REQUIRED_PAIRS) {
  const rows = Array.isArray(pairs) ? pairs : [];
  const pairMsSamples = rows.map((p) => p?.pairMs).filter((v) => Number.isFinite(v));
  return {
    requiredPairs,
    sampleCount: rows.length,
    typedPairCount: rows.filter(isValidDb1Pair).length,
    pairMs: {
      p95: nearestRankP95(pairMsSamples),
      max: maxOf(pairMsSamples),
      samples: pairMsSamples,
    },
    pairs: rows,
    pass: rows.length === requiredPairs && rows.every(isValidDb1Pair),
  };
}

/**
 * LIFE-2 verdict from the full cooperative-cancel sample: every cycle must be
 * acknowledged AND settle to `cancelled` within the matrix latency envelope
 * (ack→terminal nearest-rank p95 ≤2 s, max ≤3 s).
 */
function deriveCancelCriterion(cycles, requiredCycles = LIFE_REQUIRED_CYCLES) {
  const rows = Array.isArray(cycles) ? cycles : [];
  const ackMsSamples = rows.map((c) => c?.cancelAckToTerminalMs).filter((v) => Number.isFinite(v));
  const typedCycleCount = rows.filter(
    (c) =>
      c !== null &&
      typeof c === 'object' &&
      c.cancelAcknowledged === true &&
      c.settled === true &&
      c.observedStatus === 'cancelled',
  ).length;
  const ackP95 = nearestRankP95(ackMsSamples);
  const ackMax = maxOf(ackMsSamples);
  return {
    requiredCycles,
    sampleCount: rows.length,
    typedCycleCount,
    ackToTerminalMs: { p95: ackP95, max: ackMax, samples: ackMsSamples },
    cycles: rows,
    pass:
      rows.length === requiredCycles &&
      typedCycleCount === requiredCycles &&
      ackP95 !== null &&
      ackP95 <= LIFE2_CANCEL_ACK_P95_MAX_MS &&
      ackMax !== null &&
      ackMax <= LIFE2_CANCEL_ACK_MAX_MS,
  };
}

/**
 * LIFE-3 verdict from the full abrupt-restart sample: every cycle must report
 * the prior operation `interrupted` over HTTP 200, settle the killed child's
 * close, leave NO group descendant alive, and reach restart readiness within
 * the matrix bound (≤5 s).
 */
function deriveRestartCriterion(cycles, requiredCycles = LIFE_REQUIRED_CYCLES) {
  const rows = Array.isArray(cycles) ? cycles : [];
  const readyMsSamples = rows.map((c) => c?.restartReadyMs).filter((v) => Number.isFinite(v));
  const typedCycleCount = rows.filter(
    (c) =>
      c !== null &&
      typeof c === 'object' &&
      c.httpStatus === 200 &&
      c.observedStatus === 'interrupted' &&
      c.closedSettled === true &&
      Array.isArray(c.groupSurvivors) &&
      c.groupSurvivors.length === 0 &&
      Number.isFinite(c.restartReadyMs) &&
      c.restartReadyMs <= LIFE3_RESTART_READY_MAX_MS,
  ).length;
  return {
    requiredCycles,
    sampleCount: rows.length,
    typedCycleCount,
    restartReadyMs: {
      p95: nearestRankP95(readyMsSamples),
      max: maxOf(readyMsSamples),
      samples: readyMsSamples,
    },
    cycles: rows,
    pass: rows.length === requiredCycles && typedCycleCount === requiredCycles,
  };
}

/**
 * Common evidence protocol (proof matrix §1): a retained result must be
 * attributable to its exact source revision, execution environment, and
 * fixture dataset. Every required fact must be OBSERVED — anything
 * unavailable stays `null`, lands in `missing`, and blocks acceptance. No
 * value here is ever defaulted or guessed.
 */
function deriveProvenanceCompleteness(provenance) {
  const observed = (v) => v !== null && v !== undefined && v !== '';
  const isSha256 = (v) => typeof v === 'string' && /^[0-9a-f]{64}$/.test(v);
  const missing = [];
  if (
    !observed(provenance?.source?.sha) ||
    !/^[0-9a-f]{40}$|^[0-9a-f]{64}$/.test(provenance.source.sha)
  ) {
    missing.push('source.sha');
  }
  if (
    typeof provenance?.source?.clean !== 'boolean' ||
    !Number.isFinite(provenance?.source?.dirtyFileCount)
  ) {
    missing.push('source.treeState');
  }
  for (const key of ['os', 'arch', 'libc', 'cpuModel']) {
    if (!observed(provenance?.platform?.[key])) missing.push(`platform.${key}`);
  }
  if (!Number.isFinite(provenance?.platform?.cpuCount)) missing.push('platform.cpuCount');
  if (!Number.isFinite(provenance?.platform?.totalMemBytes)) missing.push('platform.totalMemBytes');
  for (const key of ['node', 'chromium', 'rustc', 'linker']) {
    if (!observed(provenance?.runtime?.[key])) missing.push(`runtime.${key}`);
  }
  if (!isSha256(provenance?.contractHash)) missing.push('contractHash');
  if (!observed(provenance?.dataset?.seedTool)) missing.push('dataset.seedTool');
  if (!isSha256(provenance?.dataset?.dbSha256)) missing.push('dataset.dbSha256');
  if (!Number.isFinite(provenance?.dataset?.dbBytes)) missing.push('dataset.dbBytes');
  if (!Number.isInteger(provenance?.dataset?.entityCount)) missing.push('dataset.entityCount');
  if (!Number.isInteger(provenance?.dataset?.candidatesObserved)) {
    missing.push('dataset.candidatesObserved');
  }
  return { complete: missing.length === 0, missing };
}

// ── Environment provenance (common evidence protocol) ───────────────────────

/** First non-empty stdout line of a version probe; null when unavailable. */
function firstVersionLine(command, args) {
  const res = spawnSync(command, args, { cwd: repoRoot, encoding: 'utf8' });
  if (res.error || res.status !== 0) return null;
  return (res.stdout ?? '').split('\n').map((s) => s.trim()).find(Boolean) ?? null;
}

/**
 * Observed provenance facts for this exact run: committed source SHA + tree
 * cleanliness from git, OS/arch/libc and CPU/RAM from the running process,
 * toolchain/runtime versions from the real binaries this runner depends on,
 * the contract revision hash from the contract document this checkout (or the
 * harness control root) actually carries, and the seeded fixture dataset
 * (identity = sha256 + byte size of the freshly seeded workspace `state.db`).
 * Dataset counts observed later from the live graph/candidate reads are
 * filled by `main()`. Everything unavailable stays `null`.
 */
function collectProvenance(fixtureHome) {
  const gitOut = (args) => {
    const res = spawnSync('git', args, { cwd: repoRoot, encoding: 'utf8' });
    return res.status === 0 ? (res.stdout ?? '') : null;
  };
  const sha = gitOut(['rev-parse', 'HEAD'])?.trim() ?? null;
  const porcelain = gitOut(['status', '--porcelain', '--untracked-files=normal']);
  const dirtyFiles =
    porcelain === null ? null : porcelain.split('\n').map((l) => l.trim()).filter(Boolean);
  // libc: glibc version where Node reports one (Linux); libSystem on darwin.
  let libc = null;
  try {
    const glibc = process.report?.getReport?.()?.header?.glibcVersionRuntime ?? null;
    libc = glibc ? `glibc ${glibc}` : process.platform === 'darwin' ? 'libSystem (darwin)' : null;
  } catch {
    libc = null;
  }
  // Contract hash: hash the contract revision this checkout actually carries.
  // A feature worktree does not mirror `.mstar/iterations/`, so the harness
  // control root (the parent of `.worktrees/`) is the fallback source; the
  // hashed path is always recorded next to the hash.
  const controlRoot = resolve(repoRoot, '..', '..');
  const contractRelative = join('.mstar', 'iterations', 'v1.189', 'specs', 'architecture-contracts.md');
  let contractHash = null;
  let contractHashSource = null;
  for (const base of [repoRoot, controlRoot]) {
    const candidate = join(base, contractRelative);
    if (existsSync(candidate)) {
      contractHash = sha256File(candidate);
      contractHashSource = candidate;
      break;
    }
  }
  const fixtureDbRelative =
    '.nexus42/creators/test_creator/workspaces/default/state.db';
  const fixtureDbPath = join(fixtureHome, ...fixtureDbRelative.split('/'));
  const cpus = osCpus();
  return {
    source: {
      sha,
      clean: dirtyFiles === null ? null : dirtyFiles.length === 0,
      dirtyFileCount: dirtyFiles === null ? null : dirtyFiles.length,
    },
    platform: {
      os: `${osType()} ${osRelease()}`,
      arch: process.arch,
      libc,
      cpuModel: cpus[0]?.model?.trim() ?? null,
      cpuCount: cpus.length,
      totalMemBytes: osTotalmem(),
    },
    runtime: {
      node: process.version,
      chromium: null, // filled by main() after the browser launches
      rustc: firstVersionLine('rustc', ['--version']),
      linker: firstVersionLine('cc', ['--version']),
    },
    contractHash,
    contractHashSource,
    dataset: {
      seedTool: 'cargo run -q -p nexus-core-node --bin native-wire-fixture-seed -- <fixture-home>',
      dbPath: fixtureDbRelative,
      dbSha256: existsSync(fixtureDbPath) ? sha256File(fixtureDbPath) : null,
      dbBytes: existsSync(fixtureDbPath) ? statSync(fixtureDbPath).size : null,
      entityCount: null, // observed from the first graph read
      candidatesObserved: null, // observed from the canvas candidate read
    },
  };
}

// ── Main ────────────────────────────────────────────────────────────────────

/**
 * Recorded baseline build of the direct CLI writer. ALWAYS run before any timed
 * or edit phase (not just when the binary is missing): a stale `nexus42` can
 * take an exclusive migration path and surface as writer-busy against the
 * service. This Cargo build is a baseline step, explicitly excluded from the DX
 * edit loop (DX-1 counts only Cargo/native execs traced DURING an edit).
 */
function baselineBuildNexus42() {
  const startedAt = Date.now();
  const build = spawnSync('cargo', ['build', '-p', 'nexus42', '--bin', 'nexus42'], {
    cwd: repoRoot,
    encoding: 'utf8',
  });
  const durationMs = Date.now() - startedAt;
  if (build.status !== 0) {
    throw new Error(`baseline nexus42 build failed: ${build.stderr ?? ''}`);
  }
  if (!existsSync(CLI_BINARY)) {
    throw new Error(`baseline nexus42 build produced no binary at ${CLI_BINARY}`);
  }
  return { status: build.status, durationMs, binary: CLI_BINARY, sha256: sha256File(CLI_BINARY) };
}

function ensurePrerequisites() {
  if (!existsSync(nativeFixture)) throw new Error(`ACP fixture missing: ${nativeFixture}`);
  const baseline = baselineBuildNexus42();
  if (!resolveChromium()) throw new Error('no Chromium/Chrome binary available');
  return { baseline };
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) {
    console.log(
      'Usage: node apps/nexus-service/scripts/proof-browser.mjs --samples 30 --port 18421 --out <dir>',
    );
    return;
  }
  const outDir = resolve(repoRoot, args.out);
  const screenshotsDir = join(outDir, 'screenshots');
  mkdirSync(screenshotsDir, { recursive: true });
  // Determinism: clear any prior run's evidence inside <out> so a stale PASS or
  // FAIL file/screenshot cannot coexist with this run's outcome. Only paths
  // inside the resolved --out are touched.
  for (const stale of ['browser-vertical-pass.json', 'browser-vertical-fail.json', 'browser-vertical.json']) {
    rmSync(join(outDir, stale), { force: true });
  }
  if (existsSync(screenshotsDir)) {
    for (const entry of readdirSync(screenshotsDir)) {
      rmSync(join(screenshotsDir, entry), { force: true, recursive: true });
    }
  }

  const startedAt = new Date().toISOString();
  const fixtureHome = mkdtempSync(join(tmpdir(), 'nexus-rft-browser-'));
  const fixtureLog = join(fixtureHome, 'fixture.log');
  const chromiumProfile = mkdtempSync(join(tmpdir(), 'nexus-rft-chrome-'));
  const vitePort = args.port + 1;
  const cdpPort = args.port + 2;

  const schemaHashBefore = schemaTreeHash();
  const nativeHashBefore = nativeBindingHash();

  const evidence = {
    runKind: 'browser-vertical',
    command: process.argv.join(' '),
    startedAt,
    timestamps: { utcStart: startedAt },
    world: WORLD_ID,
    samplesRequested: args.samples,
    schemaHashBefore,
    nativeHashBefore,
    phases: {},
    criteria: {},
    errors: [],
  };

  let service = null;
  let vite = null;
  let cdp = null;

  const shutdown = async (signal) => {
    if (cleanedUp) return;
    console.error(`[proof-browser] ${signal} received — cleaning up`);
    cdp?.close();
    await cleanupChildren();
    rmSync(fixtureHome, { recursive: true, force: true });
    rmSync(chromiumProfile, { recursive: true, force: true });
    process.exit(signal === 'SIGINT' ? 130 : 143);
  };
  process.on('SIGINT', () => void shutdown('SIGINT'));
  process.on('SIGTERM', () => void shutdown('SIGTERM'));

  try {
    const { baseline } = ensurePrerequisites();
    evidence.baselineNexus42Build = baseline;

    seedFixtureHome(fixtureHome);
    writeProviderConfig(fixtureHome, fixtureLog);

    // Common evidence protocol: capture provenance right after the
    // deterministic fixture dataset exists (dataset identity is the freshly
    // seeded workspace `state.db`; counts are filled from live reads below).
    evidence.provenance = collectProvenance(fixtureHome);

    // The Vite dev proof server is the browser's real loopback origin; allow it
    // on the service (append-only, user config preserved) so the page's
    // BrowserClient requests are not rejected with 403.
    const serviceEnv = withProofOrigin(process.env, vitePort);
    evidence.proofOrigin = serviceEnv.__RFT_PROOF_ORIGIN;

    service = await startService({ home: fixtureHome, port: args.port, env: serviceEnv });
    service.logPath = fixtureLog;
    const serviceUrl = service.url;

    vite = await startVite({ daemonUrl: serviceUrl, port: vitePort, env: process.env });

    const chromium = await launchChromium({ debugPort: cdpPort, userDataDir: chromiumProfile });
    const pageUrl = `${vite.url}/rft-native-proof?world=${WORLD_ID}`;
    const pageTarget = await openPageTarget(cdpPort, pageUrl);
    cdp = await new CdpClient(pageTarget.webSocketDebuggerUrl).connect();
    await cdp.send('Page.enable');
    await cdp.send('Runtime.enable');
    const navigation = {
      method: '/json/new',
      requestedUrl: pageUrl,
      targetUrl: pageTarget.url ?? null,
    };
    evidence.chromium = {
      binary: chromium.binary,
      version: chromium.version?.['Browser'] ?? chromium.version ?? null,
      userDataDir: chromiumProfile,
    };
    evidence.provenance.runtime.chromium = evidence.chromium.version;

    const ctx = {
      client: cdp,
      pageUrl,
      outDir,
      screenshotsDir,
      home: fixtureHome,
      port: args.port,
      serviceEnv,
      logPath: fixtureLog,
      serviceUrl,
      service,
      vite,
      navigation,
    };

    // 1. Browser interaction: graph/candidates/create-on-absent/stale CAS.
    evidence.phases.interaction = await runBrowserInteractionProof(ctx);
    evidence.navigation = ctx.navigation ?? null;
    // Dataset accounting from the observed first reads — never guessed.
    evidence.provenance.dataset.entityCount =
      evidence.phases.interaction.graphBefore?.entities ?? null;
    evidence.provenance.dataset.candidatesObserved = Array.isArray(
      evidence.phases.interaction.candidates,
    )
      ? evidence.phases.interaction.candidates.length
      : null;

    // 2. Provider stream + terminal through the real TS ACP adapter.
    evidence.phases.providerStream = await runProviderProof(ctx);

    // 3. Cooperative cancel (blocking fixture).
    service = await restartService(service, { providerEnv: { BLOCK_PROMPT: '1' } });
    ctx.service = service;
    ctx.serviceUrl = service.url;
    await waitForPageCondition(
      ctx.client,
      'Boolean(window.__RFT_NATIVE_PROOF__)',
      { timeoutMs: 15_000 },
    );
    evidence.phases.cancel = await runCancelProof(ctx, service);

    // 4. LIFE-3 locked protocol: abrupt restarts leave active non-resumable
    // ops Interrupted, and NO group descendant survives the SIGKILL unnoticed.
    const interruptedRun = await runInterruptedRestartCycles(ctx, service);
    service = interruptedRun.service;
    ctx.service = service;
    ctx.serviceUrl = service.url;
    // Restore the NORMAL fixture config: `providerEnv: {}` (not `{}`) so the
    // BLOCK_PROMPT=1 set for the cancel/restart cycles is rewritten away —
    // otherwise the adapter sample's prompt blocks for 30 s.
    const restored = await restartService(service, { providerEnv: {} });
    service = restored;
    ctx.service = service;
    ctx.serviceUrl = service.url;
    evidence.phases.interrupted = { cycles: interruptedRun.rows };
    // Lightweight assertion that the provider env was restored: the on-disk
    // config the service was restarted with must no longer carry BLOCK_PROMPT.
    const providerConfig = readFileSync(
      join(fixtureHome, '.nexus42', 'agent-host', 'config.toml'),
      'utf8',
    );
    evidence.criteria['LIFE-provider-env-restored'] = {
      configHasBlockPrompt: providerConfig.includes('BLOCK_PROMPT'),
      pass: !providerConfig.includes('BLOCK_PROMPT'),
    };

    // 5. TS edit loop (DX-1/DX-2): 0 Cargo traces, measured latencies.
    const editSamples = [];
    for (let i = 0; i < args.samples; i += 1) {
      editSamples.push(await runWebEditSample(ctx, i));
    }
    const webVisible = editSamples.map((s) => s.visibleMs).filter((v) => Number.isFinite(v));
    evidence.criteria['DX-2-web'] = {
      p95: nearestRankP95(webVisible),
      max: maxOf(webVisible),
      samples: webVisible,
      cargoTraceCount: maxOf(editSamples.map((s) => s.cargoTraceCount)) ?? 0,
      pass: (nearestRankP95(webVisible) ?? Infinity) <= 1_000 && (maxOf(editSamples.map((s) => s.cargoTraceCount)) ?? 1) === 0,
    };

    evidence.phases.routeEdit = await runRouteEditSample(ctx);
    evidence.phases.adapterEdit = await runAdapterEditSample(ctx, 2);

    // 6. Final hash invariance.
    const schemaHashAfter = schemaTreeHash();
    const nativeHashAfter = nativeBindingHash();
    evidence.schemaHashAfter = schemaHashAfter;
    evidence.nativeHashAfter = nativeHashAfter;

    evidence.criteria['DX-1-edit-loop'] = {
      cargoTraceCount: Math.max(
        evidence.criteria['DX-2-web'].cargoTraceCount,
        evidence.phases.routeEdit.cargoTraceCount,
        evidence.phases.adapterEdit.cargoTraceCount,
      ),
    };
    evidence.criteria['DX-1-edit-loop'].pass = evidence.criteria['DX-1-edit-loop'].cargoTraceCount === 0;
    evidence.criteria['DX-2-ts-restart'] = {
      samples: [evidence.phases.routeEdit.restartMs, evidence.phases.adapterEdit.restartMs].filter(
        (v) => Number.isFinite(v),
      ),
    };
    evidence.criteria['DX-2-ts-restart'].p95 = nearestRankP95(evidence.criteria['DX-2-ts-restart'].samples);
    evidence.criteria['DX-2-ts-restart'].pass = (evidence.criteria['DX-2-ts-restart'].p95 ?? Infinity) <= 2_000;
    // The route/adapter edit samples must observe the EDITED behavior (not just
    // a successful restart): the running service serves the transformed route,
    // and the real provider vertical streams the transformed adapter output.
    evidence.criteria['DX-2-route-transform'] = {
      observedLimit: evidence.phases.routeEdit.observedLimit,
      observedCount: evidence.phases.routeEdit.observedCount,
      expectedLimit: evidence.phases.routeEdit.expectedLimit,
      expectedCount: evidence.phases.routeEdit.expectedCount,
      pass: evidence.phases.routeEdit.editObserved === true,
    };
    evidence.criteria['DX-2-adapter-transform'] = {
      streamedText: evidence.phases.adapterEdit.streamedText,
      terminal: evidence.phases.adapterEdit.terminal,
      pass: evidence.phases.adapterEdit.editObserved === true && evidence.phases.adapterEdit.terminal === true,
    };
    // The proof canvas must render actual entity content in the DOM (not just
    // resolve an API promise) before its screenshots are meaningful.
    evidence.criteria['P4-canvas-rendered'] = {
      view: evidence.phases.interaction.renderedNodes.view,
      nodeCount: evidence.phases.interaction.renderedNodes.nodes.length,
      rowCount: evidence.phases.interaction.renderedNodes.rows.length,
      fitViewControl: evidence.phases.interaction.fitViewOutcome.control,
      createdVisibleAfterFitView: evidence.phases.interaction.fitViewOutcome.visibleAfterFit,
      pass:
        (evidence.phases.interaction.renderedNodes.nodes.some((n) => n.w > 1 && n.h > 1) ??
          evidence.phases.interaction.renderedNodes.rows.some((n) => n.w > 1 && n.h > 1)) &&
        evidence.phases.interaction.fitViewOutcome.visibleAfterFit === true,
    };
    // The direct CLI writer must become visible through the canvas watermark —
    // a rendered DOM observation, not a promise resolution.
    evidence.criteria['DB-2-cli-watermark-visible'] = {
      cliWriteStatus: evidence.phases.interaction.watermarkOutcome.cliWrite.status,
      graphCanonical: evidence.phases.interaction.watermarkOutcome.graphCanonical,
      visible: evidence.phases.interaction.watermarkOutcome.visible,
      pass:
        evidence.phases.interaction.watermarkOutcome.cliWrite.status === 0 &&
        evidence.phases.interaction.watermarkOutcome.graphCanonical ===
          evidence.phases.interaction.watermarkOutcome.cliTitle &&
        evidence.phases.interaction.watermarkOutcome.visible === true,
    };
    evidence.criteria['DB-1-stale-cas'] = deriveDb1Criterion(
      evidence.phases.interaction.racePairs,
    );
    evidence.criteria['LIFE-cooperative-cancel'] = deriveCancelCriterion(
      evidence.phases.cancel.cycles,
    );
    evidence.criteria['STREAM-terminal'] = {
      terminal: evidence.phases.providerStream.terminal,
      message: evidence.phases.providerStream.message,
      pass:
        Boolean(evidence.phases.providerStream.terminal) &&
        evidence.phases.providerStream.inspect?.status !== undefined,
    };
    evidence.criteria['LIFE-restart-interrupted'] = deriveRestartCriterion(
      evidence.phases.interrupted.cycles,
    );
    evidence.criteria['FFI-hashes-unchanged'] = {
      schemaUnchanged: schemaHashBefore === schemaHashAfter,
      nativeUnchanged: nativeHashBefore === nativeHashAfter,
    };
    evidence.criteria['FFI-hashes-unchanged'].pass =
      evidence.criteria['FFI-hashes-unchanged'].schemaUnchanged &&
      evidence.criteria['FFI-hashes-unchanged'].nativeUnchanged;
    // Common evidence protocol gate: a retained result must be attributable to
    // its exact source revision, environment, and fixture dataset. Missing
    // provenance blocks acceptance — the run cannot claim pass.
    {
      const provenanceVerdict = deriveProvenanceCompleteness(evidence.provenance);
      evidence.criteria['provenance-complete'] = {
        complete: provenanceVerdict.complete,
        missing: provenanceVerdict.missing,
        pass: provenanceVerdict.complete,
      };
    }

    evidence.pass = Object.values(evidence.criteria).every((c) => c.pass === true);
    evidence.timestamps.utcEnd = new Date().toISOString();
    const evidencePath = writeEvidence(
      outDir,
      `browser-vertical-${evidence.pass ? 'pass' : 'fail'}.json`,
      evidence,
    );
    console.log(`proof-browser: ${evidence.pass ? 'PASS' : 'FAIL'} → ${evidencePath}`);
    if (!evidence.pass) process.exitCode = 1;
  } catch (err) {
    evidence.errors.push(err instanceof Error ? `${err.message}\n${err.stack ?? ''}` : String(err));
    evidence.timestamps.utcEnd = new Date().toISOString();
    // Attribute launch/navigation/service failures: bounded, redacted tail of
    // each child's captured output keyed by unique child identity. Children
    // that exited just before the failure are drained to `close` first (short
    // bound), so the final drained bytes land in the retained evidence.
    // Redaction strips anything that looks like a path under the fixture home
    // or an API key header value.
    await drainExitedChildrenForEvidence();
    evidence.childOutput = collectChildOutput(fixtureHome);
    writeEvidence(outDir, `browser-vertical-fail.json`, evidence);
    throw err;
  } finally {
    cdp?.close();
    await cleanupChildren();
    rmSync(fixtureHome, { recursive: true, force: true });
    rmSync(chromiumProfile, { recursive: true, force: true });
    cleanedUp = true;
  }
}

/** SHA-256 of the loadable native binding artifact (path + bytes). */
function nativeBindingHash() {
  const candidates = [
    resolve(repoRoot, 'packages/nexus-native-darwin-arm64/native/nexus_core_node.node'),
    resolve(repoRoot, 'packages/nexus-native/native/nexus_core_node.node'),
  ];
  const path = candidates.find((p) => existsSync(p));
  return path ? sha256File(path) : null;
}

/** Canonical hash over the schema source tree (paths + bytes). */
function schemaTreeHash() {
  const schemasDir = resolve(repoRoot, 'schemas');
  const files = [];
  const walk = (dir) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const full = join(dir, entry.name);
      if (entry.isDirectory()) walk(full);
      else if (entry.isFile()) files.push(full);
    }
  };
  walk(schemasDir);
  const hash = createHash('sha256');
  for (const file of files.sort()) {
    hash.update(file.slice(schemasDir.length));
    hash.update(readFileSync(file));
  }
  return hash.digest('hex');
}

main().catch((err) => {
  console.error(err instanceof Error ? err.stack ?? err.message : err);
  process.exit(1);
});
