#!/usr/bin/env node
/**
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
 *     --out <evidence-dir>/browser
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
const CREATE_ENTITY = 'kb_a11ce001';
const CLI_BINARY = resolve(repoRoot, 'target', 'debug', 'nexus42');

const CARGO_FAMILY = /(?:^|\/)(cargo|rustc|rustup|cc1|clang\+\+?|ld\.lld)(?:\s|$)/i;
const NATIVE_BUILD = /(?:^|\/)(cmake|ninja|make|meson)(?:\s|$)/i;


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
const retainedExitedOutput = new Map();
const RETAINED_EXITED_OUTPUT_MAX = 8;
let childSpawnSequence = 0;

function trackChild(child, label) {
  child.__label = label;
  child.__evidenceKey = `${label}-pid${child.pid ?? 'x'}-${(childSpawnSequence += 1)}`;
  trackedChildren.add(child);
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

function signalProcessGroup(pid, signal) {
  try {
    process.kill(-pid, signal);
  } catch (err) {
    if (err?.code !== 'ESRCH') throw err;
    try {
      process.kill(pid, signal);
    } catch {
    }
  }
}

async function stopChild(child, { graceMs = 4_000, killMs = 2_000, closeMs = 2_000 } = {}) {
  if (!child || child.__closeSettled) return;
  const pid = child.pid;
  const signalGroup = (signal) => {
    try {
      signalProcessGroup(pid, signal);
    } catch {
    }
  };
  const exited = () => child.exitCode !== null || child.signalCode !== null;
  const awaitExit = (ms) =>
    Promise.race([
      new Promise((resolve) => {
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
      await awaitExit(killMs);
    }
  }
  if (!child.__closeSettled) {
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

async function startService({ home, port, env }) {
  const url = `http://127.0.0.1:${port}`;
  const child = spawnLogged(
    process.execPath,
    ['--import', 'tsx', resolve(serviceRoot, 'src/main.ts'), '--home', home, '--host', '127.0.0.1', '--port', String(port)],
    { cwd: serviceRoot, env, label: 'nexus-service' },
  );
  const startedAt = Date.now();
  await waitForHttpOk(`${url}/v1/daemon/runtime/health`, { timeoutMs: 60_000 });
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


function resolveChromium() {
  if (process.env.RFT_CHROMIUM && existsSync(process.env.RFT_CHROMIUM)) {
    return process.env.RFT_CHROMIUM;
  }

  const home = process.env.HOME ?? '';
  const macLayouts = ['chrome-mac-arm64', 'chrome-mac'];

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
      .sort((a, b) => (a < b ? -1 : a > b ? 1 : 0))
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
      .sort((a, b) => (a < b ? -1 : a > b ? 1 : 0))
      .reverse()) {
      for (const layout of macLayouts) {
        chromiumCandidates.push(
          join(playwrightCache, dir, layout, 'Chromium.app/Contents/MacOS/Chromium'),
        );
        chromiumCandidates.push(
          join(
            playwrightCache,
            dir,
            layout,
            'Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing',
          ),
        );
      }
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
    { env: process.env, label: 'chromium', collect: true },
  );
  const version = await waitForJson(`http://127.0.0.1:${debugPort}/json/version`, { timeoutMs: 30_000 });
  return { child, binary, version, debugPort };
}

async function openPageTarget(debugPort, pageUrl) {
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

async function callPage(client, fn, args = [], { awaitPromise = true, timeoutMs = 30_000 } = {}) {
  const probe = await client.send('Runtime.evaluate', { expression: 'window', returnByValue: false });
  const objectId = probe.result?.objectId;
  if (!objectId) throw new Error('page evaluation failed: page global object unavailable');
  const result = await client.send(
    'Runtime.callFunctionOn',
    {
      functionDeclaration: fn,
      objectId,
      arguments: args.map((value) => ({ value })),
      returnByValue: true,
      awaitPromise,
    },
    { timeoutMs },
  );
  if (result.exceptionDetails) {
    throw new Error(`page evaluation failed: ${JSON.stringify(result.exceptionDetails)}`);
  }
  return result.result?.value;
}

async function pollUntilTruthy(attempt, describe, { timeoutMs = 15_000, intervalMs = 150 } = {}) {
  const deadline = Date.now() + timeoutMs;
  let last;
  while (Date.now() < deadline) {
    try {
      last = await attempt();
      if (last) return last;
    } catch (err) {
      last = err instanceof Error ? err.message : String(err);
    }
    await sleep(intervalMs);
  }
  throw new Error(`Timed out waiting for page condition: ${describe}; last=${JSON.stringify(last)}`);
}

async function waitForPageCondition(client, expression, opts = {}) {
  return pollUntilTruthy(() => evaluate(client, expression), expression, opts);
}

/** Poll a structured page call until truthy; same semantics as waitForPageCondition. */
async function waitForPageFunction(client, fn, args = [], opts = {}) {
  return pollUntilTruthy(() => callPage(client, fn, args), fn, opts);
}

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
  for (const [evidenceKey, text] of retainedExitedOutput) {
    if (rows[evidenceKey]) continue;
    rows[evidenceKey] = redactChildOutput(text, fixtureHome);
  }
  return rows;
}

// ── Direct CLI writer ───────────────────────────────────────────────────────

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


async function runBrowserInteractionProof(ctx) {
  const { client, screenshotsDir } = ctx;
  const world = WORLD_ID;

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

  const graphBefore = await callPage(
    client,
    `async (entityId) => window.__RFT_NATIVE_PROOF__.readGraph().then((g) => ({ entities: (g.entities||[]).length, casVersion: (g.entities||[]).filter(e => e.key_block_id === entityId).map(e => e.version)[0] ?? null }))`,
    [CAS_ENTITY],
  );
  try {
    await waitForPageFunction(
      client,
      `async (needle) => {
        const onScreen = (el) => {
          const r = el.getBoundingClientRect();
          return r.width > 1 && r.height > 1 && r.bottom > 0 && r.right > 0 &&
            r.top < window.innerHeight && r.left < window.innerWidth;
        };
        const nodes = Array.from(document.querySelectorAll('.react-flow__node'));
        const rows = Array.from(document.querySelectorAll('table tbody tr'));
        const candidates = [...nodes, ...rows].filter(onScreen);
        return candidates.some((el) => (el.innerText || '').includes(needle));
      }`,
      ['Cas'],
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

  const candidates = await evaluate(
    client,
    'window.__RFT_NATIVE_PROOF__.readCandidates().then((c) => (c.items||[]).map(i => i.candidate_id))',
  );

  const created = await callPage(
    client,
    `async (entityId) => window.__RFT_NATIVE_PROOF__.createEntityOnAbsent({ entity_id: entityId, expected_version: 0, patch: { title: 'Proof Entity', block_type: 'character' } }).then((r) => ({ ok: true, version: r.version, id: r.entity.key_block_id })).catch((e) => ({ ok: false, status: e.status ?? 0, code: e.code ?? null, message: String((e && e.message) || e) }))`,
    [CREATE_ENTITY],
  );
  if (!created?.ok || created.id !== CREATE_ENTITY || typeof created.version !== 'number') {
    throw new Error(`create-on-absent did not succeed: ${JSON.stringify(created)}`);
  }
  await waitForPageFunction(
    client,
    `async (entityId) => window.__RFT_NATIVE_PROOF__.readGraph().then((g) => (g.entities||[]).some(e => e.key_block_id === entityId))`,
    [CREATE_ENTITY],
    { timeoutMs: 15_000 },
  );
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
    const diagnostics = await callPage(
      client,
      `async (entityId) => {
        const rect = (el) => { const r = el.getBoundingClientRect(); return { w: Math.round(r.width), h: Math.round(r.height), left: Math.round(r.left), top: Math.round(r.top), right: Math.round(r.right), bottom: Math.round(r.bottom) }; };
        const styleOf = (el) => { const s = getComputedStyle(el); return { display: s.display, visibility: s.visibility, opacity: s.opacity, transform: s.transform }; };
        const nodes = Array.from(document.querySelectorAll('.react-flow__node')).map((n) => ({ id: n.getAttribute('data-id'), text: (n.innerText || '').slice(0, 60), rect: rect(n), style: styleOf(n) }));
        const rows = Array.from(document.querySelectorAll('table tbody tr')).slice(0, 12).map((r) => ({ text: (r.innerText || '').slice(0, 60), rect: rect(r) }));
        const viewport = document.querySelector('.react-flow__viewport');
        let graphCreated = null;
        let changes = null;
        try {
          const g = await window.__RFT_NATIVE_PROOF__.readGraph();
          graphCreated = (g.entities || []).find((e) => e.key_block_id === entityId) ?? null;
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
      }`,
      [CREATE_ENTITY],
    );
    throw new Error(
      `created entity absent from canvas DOM: ${err instanceof Error ? err.message : String(err)}; diagnostics=${JSON.stringify(diagnostics)}`,
    );
  }
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

  const casVersion = graphBefore.casVersion;
  const updateViaBrowser = await callPage(
    client,
    `async (worldId, entityId, expectedVersion) => window.__RFT_NATIVE_PROOF__.client.worldKbPatchEntity(worldId, { entity_id: entityId, expected_version: expectedVersion, patch: { title: 'BROWSER-UPDATED' } }).then((r) => ({ ok: true, version: r.version })).catch((e) => ({ ok: false, status: e.status ?? 0, code: e.code ?? String((e && e.message) || e) }))`,
    [world, CAS_ENTITY, casVersion],
  );
  const readCasVersionFn = `async (entityId) => window.__RFT_NATIVE_PROOF__.readGraph().then((g) => (g.entities||[]).filter(e => e.key_block_id === entityId).map(e => e.version)[0] ?? null)`;
  const postUpdateVersion = await callPage(client, readCasVersionFn, [CAS_ENTITY]);

  const racePairs = [];
  {
    let expectedVersion = postUpdateVersion;
    for (let index = 0; index < DB1_REQUIRED_PAIRS; index += 1) {
      const raceBrowser = callPage(
        client,
        `async (worldId, entityId, expectedVersion, title) => window.__RFT_NATIVE_PROOF__.client.worldKbPatchEntity(worldId, { entity_id: entityId, expected_version: expectedVersion, patch: { title } }).then((r) => ({ winner: 'browser', status: 200, version: r.version })).catch((e) => ({ winner: 'none', status: e.status ?? 0, code: e.code ?? null }))`,
        [world, CAS_ENTITY, expectedVersion, `RACE-BROWSER-${index}`],
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
      const resultingVersion = await callPage(client, readCasVersionFn, [CAS_ENTITY]);
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
        break;
      }
      expectedVersion = resultingVersion;
    }
  }

  const cliTitle = 'CLI-WATERMARK-VISIBLE';
  const canonicalBeforeCli = await callPage(
    client,
    `async (entityId) => window.__RFT_NATIVE_PROOF__.readGraph().then((g) => (g.entities||[]).filter(e => e.key_block_id === entityId).map(e => e.canonical_name)[0] ?? null)`,
    [CAS_ENTITY],
  );
  const cliVersion = await callPage(client, readCasVersionFn, [CAS_ENTITY]);
  const cliWrite = await runCliPatch(ctx.home, {
    entityId: CAS_ENTITY,
    expectedVersion: cliVersion,
    title: cliTitle,
  });
  if (cliWrite.status !== 0) {
    throw new Error(
      `CLI watermark write failed: status=${cliWrite.status} cliVersion=${cliVersion} stdout=${cliWrite.stdout.trim()} stderr=${cliWrite.stderr.trim()}`,
    );
  }
  let graphCanonical = null;
  try {
    graphCanonical = await waitForPageFunction(
      client,
      `async (entityId, title) => window.__RFT_NATIVE_PROOF__.readGraph().then((g) => (g.entities||[]).filter(e => e.key_block_id === entityId).map(e => e.canonical_name)[0] ?? null).then((n) => n === title ? n : false)`,
      [CAS_ENTITY, cliTitle],
      { timeoutMs: 20_000 },
    );
  } catch (err) {
    throw new Error(
      `CLI watermark canonical visibility failed: ${err instanceof Error ? err.message : String(err)}; cliWrite=${JSON.stringify({ status: cliWrite.status, stdout: cliWrite.stdout.trim().slice(0, 400) })}`,
    );
  }
  let canvasText = null;
  try {
    canvasText = await waitForPageFunction(
      client,
      `(title) => (document.body.innerText || '').includes(title) ? title : false`,
      [cliTitle],
      { timeoutMs: 20_000 },
    );
  } catch (err) {
    const latest = await callPage(
      client,
      `async (entityId) => {
        const g = await window.__RFT_NATIVE_PROOF__.readGraph();
        const e = (g.entities||[]).find((x) => x.key_block_id === entityId);
        let changes = null;
        try { changes = await window.__RFT_NATIVE_PROOF__.readCoreChanges({ after_sequence: '0' }); } catch (ce) { changes = { error: String((ce && ce.message) || ce) }; }
        return { graph: { canonical_name: e?.canonical_name ?? null, version: e?.version ?? null }, changes };
      }`,
      [CAS_ENTITY],
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
  const operation = await callPage(
    client,
    `async (sessionId) => window.__RFT_NATIVE_PROOF__.executeAgentHostOperation(sessionId, { kind: "prompt", content: "hello" })`,
    [sessionId],
  );
  const events = await callPage(
    client,
    `async (sessionId) => window.__RFT_NATIVE_PROOF__.drainAgentHostEvents(sessionId, { timeoutMs: 30000 })`,
    [sessionId],
  );
  const terminal = events.find((e) => e.OpFinished ?? e.OpFailed ?? e.SessionStopped) ?? null;
  const message = events
    .map((e) => e.MessageDelta?.text ?? e.ThoughtDelta?.text ?? null)
    .filter(Boolean)
    .join('');
  const inspect = await callPage(
    client,
    `async (operationId) => window.__RFT_NATIVE_PROOF__.getAgentHostOperation(operationId)`,
    [operation.operation_id],
  );
  return { session, operation, terminal, message, inspect, eventCount: events.length };
}

async function runCancelProof(ctx, serviceHandle) {
  const { client } = ctx;
  const cycles = [];
  for (let index = 0; index < LIFE_REQUIRED_CYCLES; index += 1) {
    const session = await evaluate(
      client,
      'window.__RFT_NATIVE_PROOF__.createAgentHostSession({ provider_id: "mock-acp" })',
    );
    const operation = await callPage(
      client,
      `async (sessionId) => window.__RFT_NATIVE_PROOF__.executeAgentHostOperation(sessionId, { kind: "prompt", content: "block" })`,
      [session.session_id],
    );
    await sleep(750);
    const cancelStartedAt = performance.now();
    const cancel = await callPage(
      client,
      `async (operationId) => window.__RFT_NATIVE_PROOF__.cancelAgentHostOperation(operationId)`,
      [operation.operation_id],
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

async function runInterruptedRestartCycles(ctx, serviceHandle, { cycles = LIFE_REQUIRED_CYCLES } = {}) {
  const rows = [];
  let handle = serviceHandle;
  for (let index = 0; index < cycles; index += 1) {
    const session = await evaluate(
      ctx.client,
      'window.__RFT_NATIVE_PROOF__.createAgentHostSession({ provider_id: "mock-acp" })',
    );
    const operation = await callPage(
      ctx.client,
      `async (sessionId) => window.__RFT_NATIVE_PROOF__.executeAgentHostOperation(sessionId, { kind: "prompt", content: "block" })`,
      [session.session_id],
    );
    await sleep(750);
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
    await Promise.race([handle.child.__closed ?? Promise.resolve(), sleep(5_000)]);
    const closedSettled = handle.child.__closeSettled === true;
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

async function observeInterrupted(serviceHandle, operationId) {
  const inspect = await jsonRequest(
    `${serviceHandle.url}/v1/daemon/agent-host/operations/${operationId}`,
  );
  return { status: inspect.payload?.status ?? null, httpStatus: inspect.status };
}


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

function survivingGroupRows(groupBefore, leaderPid) {
  const rows = psRows();
  return groupBefore.filter(
    (row) =>
      row.pid !== leaderPid && rows.some((r) => r.pid === row.pid && r.command === row.command),
  );
}

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

// ── Reversible edit outcome precedence ──────────────────────────────────────

/**
 * - the restore gate ALWAYS runs, on success and on failure;
 * - a gate failure is never swallowed and never reports the sample as a
 *   success;
 * - a gate failure after a SUCCESSFUL operation is the reported failure;
 * - a gate failure after a FAILED operation is reported NEXT TO the primary
 *   error, never instead of it: the thrown `AggregateError` carries the primary
 *   error as `errors[0]` and as `cause`.
 *
 * Used by the three reversible edit samples (web marker, TS route, TS adapter).
 * Deliberately not a cleanup framework: one operation, one gate.
 */
async function runWithRestoreGate(work, restore) {
  let result;
  let primaryError;
  let operationFailed = false;
  try {
    result = await work();
  } catch (err) {
    operationFailed = true;
    primaryError = err;
  }
  let restoreError;
  let restoreFailed = false;
  try {
    await restore();
  } catch (err) {
    restoreFailed = true;
    restoreError = err;
  }
  if (!operationFailed) {
    if (restoreFailed) throw restoreError;
    return result;
  }
  if (!restoreFailed) throw primaryError;
  const primaryText = primaryError instanceof Error ? primaryError.message : String(primaryError);
  const restoreText = restoreError instanceof Error ? restoreError.message : String(restoreError);
  throw new AggregateError(
    [primaryError, restoreError],
    `edit operation failed and its restore gate also failed: primary=${primaryText}; restore=${restoreText}`,
    { cause: primaryError },
  );
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

async function runWebEditSample(ctx, sampleIndex) {
  const { client } = ctx;
  const markerPath = resolve(webRoot, 'src/pages/rft-native-proof-marker.ts');
  const edit = new TrackedEdit(markerPath).capture();
  const value = `sample-${sampleIndex}`;
  const source = `/**\n * P4-T3 development-only proof marker (RFT-M1 browser vertical).\n *\n * Runner-mutated for the DX-2 edit sample; restored byte-identically.\n */\nexport const RFT_NATIVE_PROOF_MARKER = ${JSON.stringify(value)};\n`;
  return runWithRestoreGate(
    async () => {
      const rootPids = [ctx.vite.child.pid];
      const started = Date.now();
      const { cargoSamples } = await traceDuringEdit(rootPids, async () => {
        edit.write(source);
        await waitForPageFunction(
          client,
          `(value) => (document.querySelector('[data-testid="rft-native-proof-marker"]')?.textContent ?? '').includes(value)`,
          [value],
          { timeoutMs: 5_000 },
        );
      });
      return {
        surface: 'browser-shared-ui',
        file: markerPath,
        visibleMs: Date.now() - started,
        cargoTraceCount: cargoSamples.reduce((a, b) => Math.max(a, b), 0),
      };
    },
    async () => {
      edit.restore();
      if (!edit.isRestored()) throw new Error(`marker file not restored: ${markerPath}`);
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
    },
  );
}

async function runRouteEditSample(ctx) {
  const targetPath = resolve(serviceRoot, 'src/world-kb.ts');
  const edit = new TrackedEdit(targetPath).capture();
  const originalSource = edit.original.toString('utf8');
  const editedSource = originalSource.replace(
    '{ defaultLimit = 50, max = 250 }: { defaultLimit?: number; max?: number } = {}',
    '{ defaultLimit = 1, max = 250 }: { defaultLimit?: number; max?: number } = {}',
  );
  if (editedSource === originalSource) {
    throw new Error('route edit anchor not found in world-kb.ts');
  }
  return runWithRestoreGate(
    async () => {
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
    },
    async () => {
      edit.restore();
      if (!edit.isRestored()) throw new Error(`route file not restored: ${targetPath}`);
      await restartServiceFromEdit(ctx);
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
    },
  );
}

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

const ADAPTER_SESSION_SCRIPT = `(async () => {
  const h = window.__RFT_NATIVE_PROOF__;
  const s = await h.createAgentHostSession({ provider_id: 'mock-acp' });
  const op = await h.executeAgentHostOperation(s.session_id, { kind: 'prompt', content: 'hello' });
  const events = await h.drainAgentHostEvents(s.session_id, { timeoutMs: 30000 });
  const text = events.map(e => (e.MessageDelta && e.MessageDelta.text) || (e.ThoughtDelta && e.ThoughtDelta.text) || '').join('');
  const terminal = events.some(e => e.OpFinished || e.OpFailed);
  return { text, terminal, op: op.operation_id };
})()`;

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
  const marker = `RFTADAPTER${sampleIndex}:`;
  const editedSource = originalSource.replace(
    "    const text = record.content.text ?? '';\n    return { MessageDelta: { session_id: sessionId, op_id: opId, text } };",
    `    const text = ${JSON.stringify(marker)} + (record.content.text ?? '');\n    return { MessageDelta: { session_id: sessionId, op_id: opId, text } };`,
  );
  if (editedSource === originalSource) {
    throw new Error('adapter edit anchor not found in acp.ts');
  }
  return runWithRestoreGate(
    async () => {
      edit.write(editedSource);
      let restartMs = null;
      const { cargoSamples } = await traceDuringEdit(() => liveServiceRootPids(ctx), async () => {
        const build = spawnLogged(
          'pnpm',
          ['-F', '@42ch/nexus-provider-acp', 'run', 'build'],
          { cwd: repoRoot, label: `adapter-build-${sampleIndex}` },
        );
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
      await waitForPageCondition(
        ctx.client,
        'Boolean(window.__RFT_NATIVE_PROOF__ && window.__RFT_NATIVE_PROOF__.ready)',
        { timeoutMs: 60_000 },
      );
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
    },
    async () => {
      edit.restore();
      if (!edit.isRestored()) throw new Error(`adapter file not restored: ${targetPath}`);
      requireRestoredAdapterBuild();
      await restartServiceFromEdit(ctx);
      await assertRestoredAdapterBaseline(ctx, marker);
    },
  );
}

// ── Acceptance derivations (pure; extracted verbatim by focused tests) ──────

const DB1_REQUIRED_PAIRS = 100;
const LIFE_REQUIRED_CYCLES = 10;
const LIFE2_CANCEL_ACK_P95_MAX_MS = 2_000;
const LIFE2_CANCEL_ACK_MAX_MS = 3_000;
const LIFE3_RESTART_READY_MAX_MS = 5_000;

function isValidDb1Pair(pair) {
  if (pair === null || typeof pair !== 'object') return false;
  const typedOutcome =
    pair.winner === 'browser'
      ? pair.browserStatus === 200 && pair.cliStatus === 76
      : pair.winner === 'cli' && pair.cliStatus === 0 && pair.browserStatus === 409;
  return typedOutcome && pair.resultingVersion === pair.expectedVersion + 1;
}

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

function collectProvenance(fixtureHome) {
  const gitOut = (args) => {
    const res = spawnSync('git', args, { cwd: repoRoot, encoding: 'utf8' });
    return res.status === 0 ? (res.stdout ?? '') : null;
  };
  const sha = gitOut(['rev-parse', 'HEAD'])?.trim() ?? null;
  const porcelain = gitOut(['status', '--porcelain', '--untracked-files=normal']);
  const dirtyFiles =
    porcelain === null ? null : porcelain.split('\n').map((l) => l.trim()).filter(Boolean);
  let libc = null;
  try {
    const glibc = process.report?.getReport?.()?.header?.glibcVersionRuntime ?? null;
    libc = glibc ? `glibc ${glibc}` : process.platform === 'darwin' ? 'libSystem (darwin)' : null;
  } catch {
    libc = null;
  }
  // A feature worktree does not mirror the harness process corpus, so the
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
    
    return;
  }
  const outDir = resolve(repoRoot, args.out);
  const screenshotsDir = join(outDir, 'screenshots');
  mkdirSync(screenshotsDir, { recursive: true });
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

    evidence.provenance = collectProvenance(fixtureHome);

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

    evidence.phases.interaction = await runBrowserInteractionProof(ctx);
    evidence.navigation = ctx.navigation ?? null;
    evidence.provenance.dataset.entityCount =
      evidence.phases.interaction.graphBefore?.entities ?? null;
    evidence.provenance.dataset.candidatesObserved = Array.isArray(
      evidence.phases.interaction.candidates,
    )
      ? evidence.phases.interaction.candidates.length
      : null;

    evidence.phases.providerStream = await runProviderProof(ctx);

    service = await restartService(service, { providerEnv: { BLOCK_PROMPT: '1' } });
    ctx.service = service;
    ctx.serviceUrl = service.url;
    await waitForPageCondition(
      ctx.client,
      'Boolean(window.__RFT_NATIVE_PROOF__)',
      { timeoutMs: 15_000 },
    );
    evidence.phases.cancel = await runCancelProof(ctx, service);

    const interruptedRun = await runInterruptedRestartCycles(ctx, service);
    service = interruptedRun.service;
    ctx.service = service;
    ctx.serviceUrl = service.url;
    const restored = await restartService(service, { providerEnv: {} });
    service = restored;
    ctx.service = service;
    ctx.serviceUrl = service.url;
    evidence.phases.interrupted = { cycles: interruptedRun.rows };
    const providerConfig = readFileSync(
      join(fixtureHome, '.nexus42', 'agent-host', 'config.toml'),
      'utf8',
    );
    evidence.criteria['LIFE-provider-env-restored'] = {
      configHasBlockPrompt: providerConfig.includes('BLOCK_PROMPT'),
      pass: !providerConfig.includes('BLOCK_PROMPT'),
    };

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
    const _evidencePath = writeEvidence(
      outDir,
      `browser-vertical-${evidence.pass ? 'pass' : 'fail'}.json`,
      evidence,
    );
    
    if (!evidence.pass) process.exitCode = 1;
  } catch (err) {
    evidence.errors.push(err instanceof Error ? `${err.message}\n${err.stack ?? ''}` : String(err));
    evidence.timestamps.utcEnd = new Date().toISOString();
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
  for (const file of files.sort((a, b) => (a < b ? -1 : a > b ? 1 : 0))) {
    hash.update(file.slice(schemasDir.length));
    hash.update(readFileSync(file));
  }
  return hash.digest('hex');
}

main().catch((err) => {
  console.error(err instanceof Error ? err.stack ?? err.message : err);
  process.exit(1);
});
