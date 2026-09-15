#!/usr/bin/env node
/** RFT DX proof runner — cold/warm/no-Cargo evidence for web/studio/shared-ui/desktop-web. */
import { execFile, spawn } from 'node:child_process';
import { access, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { cpus, freemem, totalmem, arch, platform, release } from 'node:os';
import { join, relative, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { promisify } from 'node:util';

import {
  BackendCompatibilityError,
  CURRENT_WRITER_PROTOCOL,
  REMEDIATION_COMMAND,
  assertCompatibleBackend,
  assertCompatibleRunningDaemon,
  computeContractHash,
  computeDbSchemaRange,
  defaultArtifactPath,
  getRepoRoot,
  getHostTriple,
  isDaemonCliStatusRunning,
  manifestPathForArtifact,
  readBackendManifest,
  refreshBackend,
  resolveDaemonEndpoint,
  resolveListenerPid,
  resolveTargetDir,
  sha256File,
  validateDaemonHealth,
  waitForDaemonHealth,
  writeManifestAtomic,
} from './dev-backend-manifest.mjs';

const exec = promisify(execFile);
const GRAPH_PATH = '/v1/daemon/worlds/wld_proof_rft_dx/kb/graph';
const DEFAULT_DAEMON_PORT = 8420;
const DX1_SAMPLE_INTERVAL_MS = 50;

const CARGO_FAMILY = /(?:^|\/)(cargo|rustc|rustup|cc1|clang\+\+?|ld\.lld)(?:\s|$)/i;
const TAURI_FAMILY = /(?:^|\/)(tauri|cargo-tauri)(?:\s|$)/i;
const NATIVE_BUILD = /(?:^|\/)(cmake|ninja|make|meson)(?:\s|$)/i;
const VITE_ORIGIN_RE = /Local:\s+(https?:\/\/[^\s]+)/;

export const SURFACE_CONFIG = {
  web: {
    label: 'web', vitePort: 5173, needsDaemon: true, needsSidecar: false, needsUiWatcher: false,
    markerRelative: 'apps/web/src/proof-rft-dx-marker.ts', markerUrlPath: '/src/proof-rft-dx-marker.ts',
    servedProbePath: '/', devCommand: ['pnpm', '--filter', 'web', 'dev'],
  },
  studio: {
    label: 'studio', vitePort: 5174, needsDaemon: false, needsSidecar: false, needsUiWatcher: false,
    markerRelative: 'apps/design-studio/src/proof-rft-dx-marker.ts', markerUrlPath: '/src/proof-rft-dx-marker.ts',
    servedProbePath: '/', devCommand: ['pnpm', '--filter', 'design-studio', 'dev'],
  },
  'shared-ui': {
    label: 'shared-ui', vitePort: 5173, needsDaemon: true, needsSidecar: false, needsUiWatcher: true,
    markerRelative: 'packages/nexus-ui/src/proof-rft-dx-marker.ts', markerUrlPath: null,
    servedProbePath: '/', devCommand: ['pnpm', '--filter', 'web', 'dev'],
    uiWatcherCommand: ['pnpm', '--filter', '@42ch/nexus-ui', 'dev'],
  },
  'desktop-web': {
    label: 'desktop-web', vitePort: 5173, needsDaemon: true, needsSidecar: true, needsUiWatcher: false,
    markerRelative: 'apps/web/src/proof-rft-dx-marker.ts', markerUrlPath: '/src/proof-rft-dx-marker.ts',
    servedProbePath: '/', devCommand: ['pnpm', 'run', 'dev:desktop:web'],
    loopAlias: 'pnpm run dev:desktop:web',
  },
};

export function parseArgs(argv) {
  const result = {
    surface: null, negative: null, boundaryDemo: false, refreshReproof: false, injectFail: false,
    samples: 30, coldSamples: 10, port: DEFAULT_DAEMON_PORT, out: null, help: false,
  };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--surface') result.surface = argv[++i];
    else if (arg === '--negative') result.negative = argv[++i];
    else if (arg === '--boundary-demo') result.boundaryDemo = true;
    else if (arg === '--refresh-reproof') result.refreshReproof = true;
    else if (arg === '--inject-fail') result.injectFail = true;
    else if (arg === '--samples') result.samples = Number.parseInt(argv[++i], 10);
    else if (arg === '--cold-samples') result.coldSamples = Number.parseInt(argv[++i], 10);
    else if (arg === '--port') result.port = Number.parseInt(argv[++i], 10);
    else if (arg === '--out') result.out = argv[++i];
    else if (arg === '--help' || arg === '-h') result.help = true;
    else throw new Error(`Unknown argument: ${arg}`);
  }
  return result;
}

export function requireOutDir(args) {
  if (!args.out) throw new Error('--out <dir> is required');
  return resolve(args.out);
}

export function nearestRankP95(samples) {
  if (!samples.length) return null;
  const sorted = [...samples].sort((a, b) => a - b);
  const rank = Math.ceil(0.95 * sorted.length);
  return sorted[Math.min(rank - 1, sorted.length - 1)];
}

export function maxSample(samples) { return samples.length ? Math.max(...samples) : null; }

export function evaluateDx2(samples) {
  const p95 = nearestRankP95(samples); const max = maxSample(samples);
  return { p95Ms: p95, maxMs: max, pass: p95 !== null && max !== null && p95 <= 1000 && max <= 2000, threshold: { p95Ms: 1000, maxMs: 2000 } };
}

export function evaluateDx3(samples) {
  const p95 = nearestRankP95(samples); const max = maxSample(samples);
  return { p95Ms: p95, maxMs: max, pass: p95 !== null && max !== null && p95 <= 5000, threshold: { p95Ms: 5000 } };
}

export function isCargoFamilyCommand(commandLine) { return CARGO_FAMILY.test(commandLine) || NATIVE_BUILD.test(commandLine); }
export function isTauriFamilyCommand(commandLine) { return TAURI_FAMILY.test(commandLine); }

export function parsePsLines(output) {
  return output.split('\n').map(l => l.trim()).filter(Boolean).map(line => {
    const m = line.match(/^(\d+)\s+(\d+)\s+(.+)$/);
    return m ? { pid: +m[1], ppid: +m[2], command: m[3] } : null;
  }).filter(Boolean);
}

export function collectDescendants(rootPid, rows) {
  const children = new Map();
  for (const row of rows) { if (!children.has(row.ppid)) children.set(row.ppid, []); children.get(row.ppid).push(row.pid); }
  const seen = new Set([rootPid]); const queue = [rootPid];
  while (queue.length) { const pid = queue.shift(); for (const child of children.get(pid) ?? []) { if (!seen.has(child)) { seen.add(child); queue.push(child); } } }
  return seen;
}

export function countCargoTraces(rootPid, psOutput) {
  const rows = parsePsLines(psOutput); const descendants = collectDescendants(rootPid, rows);
  const traced = rows.filter(r => descendants.has(r.pid));
  const cargoHits = traced.filter(r => isCargoFamilyCommand(r.command));
  const tauriHits = traced.filter(r => isTauriFamilyCommand(r.command));
  return { processCount: traced.length, cargoCount: cargoHits.length, tauriCount: tauriHits.length, cargoCommands: cargoHits.map(r => r.command), tauriCommands: tauriHits.map(r => r.command), tracedPids: [...descendants].sort((a, b) => a - b) };
}

export function isPidInForest(pid, rootPids, psOutput) {
  if (!Number.isInteger(pid) || pid <= 0) return false;
  for (const rootPid of rootPids) {
    const { tracedPids } = countCargoTraces(rootPid, psOutput);
    if (tracedPids.includes(pid)) return true;
  }
  return false;
}

export function summarizeIntervalSamples(samples) {
  const cargoCommands = new Set();
  const tauriCommands = new Set();
  const tracedPids = new Set();
  let maxProcessCount = 0;
  let cargoPositiveSamples = 0;
  for (const sample of samples) {
    if (sample.cargoCount > 0) cargoPositiveSamples += 1;
    maxProcessCount = Math.max(maxProcessCount, sample.processCount ?? 0);
    for (const cmd of sample.cargoCommands ?? []) cargoCommands.add(cmd);
    for (const cmd of sample.tauriCommands ?? []) tauriCommands.add(cmd);
    for (const pid of sample.tracedPids ?? []) tracedPids.add(pid);
  }
  return {
    observationCount: samples.length,
    cargoPositiveSamples,
    processCount: maxProcessCount,
    cargoCount: cargoCommands.size,
    tauriCount: tauriCommands.size,
    cargoCommands: [...cargoCommands],
    tauriCommands: [...tauriCommands],
    tracedPids: [...tracedPids].sort((a, b) => a - b),
    anyCargo: cargoPositiveSamples > 0,
    anyTauri: tauriCommands.size > 0,
  };
}

export function getDx1RootPids({ viteChild, watcherChild, config }) {
  const roots = [];
  if (viteChild?.pid) roots.push(viteChild.pid);
  if (config.needsUiWatcher && watcherChild?.pid) roots.push(watcherChild.pid);
  return roots;
}

export function parseViteOriginFromChunk(text) {
  const match = text.match(VITE_ORIGIN_RE);
  return match ? match[1].trim().replace(/\/$/, '') : null;
}

export function evidenceFilename({ runKind, pass, startedAt }) { return `${startedAt.replace(/[:.]/g, '-')}-${runKind}-${pass ? 'pass' : 'fail'}.json`; }
export function markerSource(sample) { return `/** proof-rft-dx runner-owned marker */\nexport const PROOF_RFT_DX_MARKER = ${JSON.stringify(`sample-${sample}`)};\n`; }
export function markerProbeUrl(viteOrigin, config, repoRoot) {
  if (config.markerUrlPath) return `${viteOrigin}${config.markerUrlPath}`;
  const abs = join(repoRoot, config.markerRelative).replaceAll('\\', '/');
  return `${viteOrigin}/@fs${abs}`;
}

async function runVersion(command, args) {
  try { return (await exec(command, args)).stdout.trim().split('\n')[0]; } catch { return 'N/A'; }
}

export async function captureCompilerVersions() {
  const rustc = await runVersion('rustc', ['--version']);
  const cargo = await runVersion('cargo', ['--version']);
  const linker = process.platform === 'darwin'
    ? await runVersion('clang', ['--version'])
    : await runVersion('ld', ['--version']);
  return { rustc, cargo, linker };
}

export async function captureEnvironment(repoRoot, { includeCompiler = false } = {}) {
  const cpuList = cpus();
  const compiler = includeCompiler ? await captureCompilerVersions() : { rustc: 'N/A', cargo: 'N/A', linker: 'N/A' };
  return {
    os: {
      platform: platform(),
      arch: arch(),
      release: release(),
      version: `${platform()} ${release()}`,
      node: process.version,
    },
    cpu: { model: cpuList[0]?.model ?? 'N/A', logicalCores: cpuList.length },
    compiler,
    memory: { totalBytes: totalmem(), freeBytes: freemem() },
    repoRoot,
    sourceSha: await runGitSha(repoRoot),
  };
}

export async function sampleProcessForest(rootPids) {
  const psOutput = (await exec('ps', ['-eo', 'pid=,ppid=,command='])).stdout;
  const perRoot = [];
  for (const rootPid of rootPids) perRoot.push({ rootPid, ...countCargoTraces(rootPid, psOutput) });
  const merged = summarizeIntervalSamples(perRoot);
  return { perRoot, merged, psOutput };
}

export async function traceDuringEdit(rootPids, fn, { intervalMs = DX1_SAMPLE_INTERVAL_MS } = {}) {
  const intervalSamples = [];
  let running = true;
  const sampler = (async () => {
    while (running) {
      const { perRoot } = await sampleProcessForest(rootPids);
      for (const sample of perRoot) intervalSamples.push(sample);
      await sleep(intervalMs);
    }
  })();
  try {
    const result = await fn();
    return { result, intervalSamples };
  } finally {
    running = false;
    await Promise.race([sampler, sleep(intervalMs * 3)]);
  }
}

async function startDaemonNonBlocking(artifactPath, port, env) {
  await new Promise((resolvePromise, reject) => {
    const child = spawn(artifactPath, ['daemon', 'start', '--port', String(port)], { env, stdio: 'ignore' });
    child.on('error', reject);
    child.on('close', () => resolvePromise());
    setTimeout(resolvePromise, 1000);
  });
}

async function sleep(ms) { await new Promise(r => setTimeout(r, ms)); }
async function pathExists(path) { try { await access(path); return true; } catch { return false; } }
async function runGitSha(repoRoot) { return (await exec('git', ['rev-parse', 'HEAD'], { cwd: repoRoot })).stdout.trim(); }

async function fetchText(url, { timeoutMs = 5000 } = {}) {
  const controller = new AbortController(); const timer = setTimeout(() => controller.abort(), timeoutMs);
  try { const response = await fetch(url, { signal: controller.signal }); return { ok: response.ok, status: response.status, body: await response.text(), url }; }
  finally { clearTimeout(timer); }
}

async function waitForHttpOk(url, { timeoutMs = 120_000, intervalMs = 250 } = {}) {
  const deadline = Date.now() + timeoutMs; let last;
  while (Date.now() < deadline) {
    try { const result = await fetchText(url, { timeoutMs: Math.min(5000, deadline - Date.now()) }); last = result; if (result.ok) return result; }
    catch (err) { last = { error: err instanceof Error ? err.message : String(err), url }; }
    await sleep(intervalMs);
  }
  throw new Error(`Timed out waiting for HTTP OK at ${url}; last=${JSON.stringify(last)}`);
}

async function waitForMarker(viteOrigin, config, repoRoot, expectedSample, { timeoutMs = 5000 } = {}) {
  const url = markerProbeUrl(viteOrigin, config, repoRoot); const needle = `sample-${expectedSample}`; const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const result = await fetchText(url, { timeoutMs: Math.min(2000, deadline - Date.now()) });
    if (result.body.includes(needle)) return { url, bodySnippet: result.body.slice(0, 200) };
    await sleep(100);
  }
  throw new Error(`Timed out waiting for marker ${needle} at ${url}`);
}

class TrackedEdit {
  constructor(absPath) { this.absPath = absPath; this.original = null; this.existed = false; }
  async capture() { this.existed = await pathExists(this.absPath); this.original = this.existed ? await readFile(this.absPath) : null; }
  async write(content) { await writeFile(this.absPath, content, 'utf8'); }
  async restore() { if (this.existed) await writeFile(this.absPath, this.original); else if (await pathExists(this.absPath)) await rm(this.absPath); }
}

async function assertByteIdenticalRestore(edits) {
  for (const edit of edits) {
    const exists = await pathExists(edit.absPath);
    if (edit.existed) { const current = await readFile(edit.absPath); if (!current.equals(edit.original)) throw new Error(`Working tree not restored: ${edit.absPath}`); }
    else if (exists) throw new Error(`Runner-created file still present: ${edit.absPath}`);
  }
}

export function collectProcessTreePids(rootPid, psOutput) {
  if (!Number.isInteger(rootPid) || rootPid <= 0) return [];
  return [...collectDescendants(rootPid, parsePsLines(psOutput))];
}

async function waitForPortFree(port, { timeoutMs = 15_000 } = {}) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const listenerPid = await resolveListenerPid(port);
    if (listenerPid === null) return;
    await sleep(100);
  }
  throw new Error(`Timed out waiting for port ${port} to be released`);
}
async function killProcessTree(child) {
  if (!child?.pid) return;
  let psOutput = (await exec('ps', ['-eo', 'pid=,ppid=,command='])).stdout;
  const pids = collectProcessTreePids(child.pid, psOutput);
  for (const pid of [...pids].sort((a, b) => b - a)) {
    try { process.kill(pid, 'SIGTERM'); } catch {}
  }
  await sleep(500);
  psOutput = (await exec('ps', ['-eo', 'pid=,ppid=,command='])).stdout;
  const survivors = collectProcessTreePids(child.pid, psOutput).filter(pid => {
    try { process.kill(pid, 0); return true; } catch { return false; }
  });
  for (const pid of survivors.sort((a, b) => b - a)) {
    try { process.kill(pid, 'SIGKILL'); } catch {}
  }
  await new Promise(resolve => { child.on('close', resolve); setTimeout(resolve, 1000); });
}

function spawnLogged(command, args, { cwd, env, label }) {
  const child = spawn(command, args, { cwd, env, stdio: ['ignore', 'pipe', 'pipe'] });
  let capturedOrigin = null;
  const onData = chunk => {
    process.stderr.write(`[${label}] ${chunk}`);
    const origin = parseViteOriginFromChunk(chunk.toString());
    if (origin) capturedOrigin = origin;
  };
  child.stdout?.on('data', onData);
  child.stderr?.on('data', onData);
  child.getCapturedOrigin = () => capturedOrigin;
  return child;
}

const EVIDENCE_FIELDS = new Set([
  'runKind', 'pass', 'command', 'error', 'cleanupOutcome', 'timestamps', 'environment', 'surface',
  'artifact', 'criteria', 'endpoint', 'loopCommand', 'sidecarBaseline', 'observedOutcomes', 'notes',
  'negativeCase', 'observedError', 'remediation', 'sample', 'boundary', 'beforeSha256', 'afterSha256',
  'artifactPath',
]);
const EVIDENCE_MAX_DEPTH = 12;
const EVIDENCE_MAX_STRING = 256 * 1024;
const EVIDENCE_MAX_ITEMS = 50_000;

/** Strict evidence schema: only allowlisted top-level fields are persisted. */
function toEvidenceObject(payload) {
  const evidence = {};
  for (const [key, value] of Object.entries(payload)) {
    if (EVIDENCE_FIELDS.has(key)) evidence[key] = boundEvidenceData(value, 0);
  }
  return evidence;
}

/** Strong bounded validator before persistence: plain JSON values only, with depth/size/key-shape limits. */
function boundEvidenceData(value, depth) {
  if (value === null || typeof value === 'boolean') return value;
  if (typeof value === 'number') {
    if (!Number.isFinite(value)) throw new Error('Evidence number must be finite');
    return value;
  }
  if (typeof value === 'string') {
    if (value.length > EVIDENCE_MAX_STRING) throw new Error(`Evidence string exceeds ${EVIDENCE_MAX_STRING} characters`);
    return value;
  }
  if (depth >= EVIDENCE_MAX_DEPTH) throw new Error('Evidence nesting exceeds maximum depth');
  if (Array.isArray(value)) {
    if (value.length > EVIDENCE_MAX_ITEMS) throw new Error('Evidence array exceeds maximum size');
    return value.map(item => boundEvidenceData(item, depth + 1));
  }
  if (typeof value === 'object') {
    const proto = Object.getPrototypeOf(value);
    if (proto !== Object.prototype && proto !== null) throw new Error('Evidence must be plain JSON data');
    const out = {};
    for (const [key, item] of Object.entries(value)) {
      if (key === '' || key.length > 128 || key === '__proto__' || key === 'constructor' || key === 'prototype') {
        throw new Error(`Unsafe evidence key: ${key}`);
      }
      out[key] = boundEvidenceData(item, depth + 1);
    }
    return out;
  }
  throw new Error(`Unsupported evidence value type: ${typeof value}`);
}

export async function writeEvidence(outDir, payload) {
  await mkdir(outDir, { recursive: true });
  const evidence = toEvidenceObject(payload);
  const filename = evidenceFilename({ runKind: payload.runKind, pass: payload.pass, startedAt: payload.timestamps.utcStart });
  if (!/^[\w.-]+\.json$/.test(filename)) throw new Error(`Refusing unsafe evidence filename: ${filename}`);
  const target = join(outDir, filename);
  try {
    await writeFile(target, `${JSON.stringify(evidence, null, 2)}\n`, { encoding: 'utf8', flag: 'wx' });
  } catch (err) {
    if (err.code === 'EEXIST') throw new Error(`Evidence file already exists: ${target}`);
    throw err;
  }
  return target;
}

export function recordDaemonOwnership(wasRunning) {
  return { startedByRunner: !wasRunning, wasRunning };
}
export async function ensureDaemonTracked({ artifactPath, port, env }) {
  const manifest = await readBackendManifest(artifactPath);
  const { baseUrl } = resolveDaemonEndpoint({ portEnv: String(port), urlEnv: env.VITE_DAEMON_URL });
  const statusOutput = (await exec(artifactPath, ['daemon', 'status', '--port', String(port)], { env })).stdout;
  const wasRunning = isDaemonCliStatusRunning(statusOutput);
  const ownership = recordDaemonOwnership(wasRunning);
  if (ownership.startedByRunner) {
    await startDaemonNonBlocking(artifactPath, port, env);
  }
  await waitForDaemonHealth(baseUrl, { expectedPackageVersion: manifest.packageVersion, deadlineMs: 120_000 });
  await assertCompatibleRunningDaemon({ baseUrl, manifest, port, daemonStatusOutput: statusOutput });
  return { baseUrl, manifest, startedByRunner: ownership.startedByRunner };
}

async function validateEndpointGraph(baseUrl) {
  const health = await validateDaemonHealth(baseUrl);
  const graphUrl = new URL(GRAPH_PATH, baseUrl);
  const graph = await fetchText(String(graphUrl));
  return { health, graph: { url: String(graphUrl), status: graph.status, body: JSON.parse(graph.body) } };
}

export async function assertEndpointOwnedByForest(port, rootPids, { psOutput, listenerPid: listenerPidOverride } = {}) {
  const listenerPid = listenerPidOverride ?? await resolveListenerPid(port);
  if (listenerPid === null) return { listenerPid: null, owned: true };
  const output = psOutput ?? (await exec('ps', ['-eo', 'pid=,ppid=,command='])).stdout;
  const owned = isPidInForest(listenerPid, rootPids, output);
  if (!owned) {
    throw new Error(`Port ${port} is occupied by foreign PID ${listenerPid}; refusing to collect timing evidence from a non-child listener`);
  }
  return { listenerPid, owned: true };
}

/** DX-3 cold sample: clock starts at spawn, stops after served-page fetch; origin wait recorded separately. */
export async function measureColdLaunchSample({ spawnChild, resolveOrigin, waitForServed }) {
  const t0 = performance.now();
  const child = spawnChild();
  const originResolveStart = performance.now();
  const origin = await resolveOrigin(child);
  const originResolveMs = performance.now() - originResolveStart;
  await waitForServed(origin);
  return { child, origin, elapsedMs: performance.now() - t0, originResolveMs };
}

export async function resolveViteOrigin({ child, config, rootPids, fallbackOrigin, timeoutMs = 120_000 }) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const captured = child.getCapturedOrigin?.();
    if (captured) {
      const url = new URL(captured);
      const psOutput = (await exec('ps', ['-eo', 'pid=,ppid=,command='])).stdout;
      try {
        await assertEndpointOwnedByForest(url.port, rootPids, { psOutput });
      } catch {
        await sleep(250);
        continue;
      }
      return captured.replace(/\/$/, '');
    }
    const listenerPid = await resolveListenerPid(config.vitePort);
    if (listenerPid !== null) {
      const psOutput = (await exec('ps', ['-eo', 'pid=,ppid=,command='])).stdout;
      try {
        await assertEndpointOwnedByForest(config.vitePort, rootPids, { psOutput });
      } catch {
        await sleep(250);
        continue;
      }
      return fallbackOrigin;
    }
    await sleep(250);
  }
  throw new Error(`Timed out resolving Vite origin for ${config.label}; last fallback=${fallbackOrigin}`);
}

async function runSidecarBaseline(repoRoot) {
  const startedAt = new Date().toISOString(); const t0 = performance.now();
  await new Promise((resolvePromise, reject) => {
    const child = spawn('bash', ['scripts/fetch-sidecar.sh'], { cwd: repoRoot, env: { ...process.env, SIDECAR_ENSURE_ONLY: '1' }, stdio: 'inherit' });
    child.on('error', reject); child.on('close', code => (code === 0 ? resolvePromise() : reject(new Error(`sidecar exit ${code}`))));
  });
  return { kind: 'desktop-web-sidecar-baseline', durationMs: performance.now() - t0, startedAt, endedAt: new Date().toISOString() };
}

async function buildArtifactRecord(artifactPath, manifest) {
  const manifestPath = manifestPathForArtifact(artifactPath);
  return {
    path: artifactPath,
    manifestPath,
    sha256: manifest.sha256,
    manifestSha256: await sha256File(manifestPath),
    contractHash: manifest.contractHash,
  };
}

export function buildFailurePayload({
  runKind, startedAt, command, environment, error, cleanupOutcome, partial = {},
}) {
  return {
    runKind,
    pass: false,
    command,
    error: error instanceof Error ? error.message : String(error),
    cleanupOutcome,
    timestamps: { utcStart: startedAt, utcEnd: new Date().toISOString() },
    environment,
    ...partial,
  };
}

async function recordRunFailure(outDir, payload) {
  try {
    return await writeEvidence(outDir, payload);
  } catch (writeErr) {
    console.error(`Failed to write failure evidence: ${writeErr instanceof Error ? writeErr.message : writeErr}`);
    return null;
  }
}
export async function recordFailureAfterCleanup({ outDir, buildPayload, cleanup }) {
  const cleanupOutcome = {};
  try {
    Object.assign(cleanupOutcome, await cleanup());
  } catch (cleanupErr) {
    cleanupOutcome.cleanupError = cleanupErr instanceof Error ? cleanupErr.message : String(cleanupErr);
  }
  return recordRunFailure(outDir, buildPayload(cleanupOutcome));
}


export async function runSurfaceLoop(options) {
  const { surface, samples, coldSamples, port, outDir, repoRoot, env: baseEnv, injectFail = false } = options;
  const config = SURFACE_CONFIG[surface];
  const startedAt = new Date().toISOString();
  const command = process.argv.join(' ');
  let runError = null;
  let environment = null;
  const cleanupOutcome = { restoredMarker: false, childrenStopped: false, daemonStopped: false };
  const partial = { surface };
  let markerEdit = null;
  const children = [];
  let watcherChild = null;
  let daemonStartedByRunner = false;
  let artifactPath = null;
  let manifest = null;
  let runEnv = baseEnv;

  try {
    environment = await captureEnvironment(repoRoot);
    const targetDir = await resolveTargetDir(baseEnv);
    artifactPath = defaultArtifactPath({ targetDir });
    const contractHash = await computeContractHash(repoRoot);
    runEnv = { ...baseEnv, NEXUS42_DAEMON_PORT: String(port), VITE_DAEMON_URL: `http://127.0.0.1:${port}`, NEXUS42_ARTIFACT: artifactPath };
    manifest = await assertCompatibleBackend({ artifactPath, contractHash, protocolVersion: CURRENT_WRITER_PROTOCOL });
    let endpointCheck = null;
    if (config.needsDaemon) {
      const ensured = await ensureDaemonTracked({ artifactPath, port, env: runEnv });
      daemonStartedByRunner = ensured.startedByRunner;
      endpointCheck = await validateEndpointGraph(ensured.baseUrl);
    }
    const sidecarBaseline = config.needsSidecar ? await runSidecarBaseline(repoRoot) : null;
    const markerPath = join(repoRoot, config.markerRelative);
    markerEdit = new TrackedEdit(markerPath); await markerEdit.capture();
    const fallbackOrigin = `http://127.0.0.1:${config.vitePort}`;
    if (config.needsUiWatcher) {
      watcherChild = spawnLogged(config.uiWatcherCommand[0], config.uiWatcherCommand.slice(1), { cwd: repoRoot, env: runEnv, label: 'nexus-ui-dev' });
      children.push(watcherChild);
      await sleep(3000);
    }
    const warmSamples = []; const coldSampleValues = []; const coldOriginResolveMs = []; const dx1Traces = [];
    const runViteChild = () => spawnLogged(config.devCommand[0], config.devCommand.slice(1), { cwd: repoRoot, env: runEnv, label: surface });
    let viteChild = runViteChild(); children.push(viteChild);
    let rootPids = getDx1RootPids({ viteChild, watcherChild, config });
    let viteOrigin = await resolveViteOrigin({ child: viteChild, config, rootPids, fallbackOrigin });
    for (let i = 0; i < coldSamples; i++) {
      if (viteChild) await killProcessTree(viteChild);
      await waitForPortFree(config.vitePort);
      const cold = await measureColdLaunchSample({
        spawnChild: () => {
          viteChild = runViteChild();
          children.push(viteChild);
          rootPids = getDx1RootPids({ viteChild, watcherChild, config });
          return viteChild;
        },
        resolveOrigin: child => resolveViteOrigin({ child, config, rootPids, fallbackOrigin }),
        waitForServed: origin => waitForHttpOk(`${origin}${config.servedProbePath}`),
      });
      viteOrigin = cold.origin;
      coldSampleValues.push(cold.elapsedMs);
      coldOriginResolveMs.push(cold.originResolveMs);
    }
    if (viteChild) await killProcessTree(viteChild);
    await waitForPortFree(config.vitePort);
    viteChild = runViteChild(); children.push(viteChild);
    rootPids = getDx1RootPids({ viteChild, watcherChild, config });
    viteOrigin = await resolveViteOrigin({ child: viteChild, config, rootPids, fallbackOrigin });
    await waitForHttpOk(`${viteOrigin}${config.servedProbePath}`);
    if (injectFail) throw new Error('Injected failure for evidence path verification');
    for (let i = 0; i < 3 + samples; i++) {
      const { result: markerResult, intervalSamples } = await traceDuringEdit(rootPids, async () => {
        await markerEdit.write(markerSource(i));
        const t0 = performance.now();
        const marker = await waitForMarker(viteOrigin, config, repoRoot, i);
        return { elapsedMs: performance.now() - t0, marker };
      });
      const summary = summarizeIntervalSamples(intervalSamples);
      dx1Traces.push({ sampleIndex: i, intervalSamples: intervalSamples.length, ...summary, marker: markerResult.marker });
      if (i >= 3) warmSamples.push(markerResult.elapsedMs);
    }
    const dx1CargoTotal = dx1Traces.reduce((sum, trace) => sum + (trace.anyCargo ? 1 : 0), 0);
    const dx1TauriTotal = dx1Traces.reduce((sum, trace) => sum + (trace.anyTauri ? 1 : 0), 0);
    const dx2 = evaluateDx2(warmSamples); const dx3 = evaluateDx3(coldSampleValues);
    const dx1Pass = dx1Traces.every(trace => !trace.anyCargo) && (surface !== 'desktop-web' || dx1Traces.every(trace => !trace.anyTauri));
    const payload = {
      runKind: `${surface}-stable-loop`, pass: dx1Pass && dx2.pass && dx3.pass, command, surface,
      loopCommand: config.loopAlias ?? config.devCommand.join(' '),
      criteria: {
        'DX-1': {
          pass: dx1Pass,
          cargoTraceCount: dx1CargoTotal,
          tauriTraceCount: dx1TauriTotal,
          sampling: { mode: 'interval', intervalMs: DX1_SAMPLE_INTERVAL_MS, rootPidsTracked: rootPids },
          samples: dx1Traces,
        },
        'DX-2': { ...dx2, samplesMs: warmSamples },
        'DX-3': {
          ...dx3,
          samplesMs: coldSampleValues,
          originResolveMs: coldOriginResolveMs,
          timing: { clockStartsAt: 'spawn', clockStopsAt: 'served-page', originResolveRecordedSeparately: true },
        },
      },
      endpoint: config.needsDaemon ? { port, baseUrl: `http://127.0.0.1:${port}`, health: endpointCheck.health, graph: endpointCheck.graph, viteOrigin } : { viteOrigin },
      sidecarBaseline,
      artifact: await buildArtifactRecord(artifactPath, manifest),
      timestamps: { utcStart: startedAt, utcEnd: new Date().toISOString() },
      environment,
      observedOutcomes: { warmSampleCount: warmSamples.length, coldSampleCount: coldSampleValues.length },
      notes: [
        'HTTP fetch of Vite-served page; no browser automation.',
        'DX-3 cold samples measure spawn→served-page; origin-resolution wait is recorded separately and never subtracted.',
        config.needsSidecar ? 'Desktop-web sidecar baseline executed before timed DX-2/DX-3 windows.' : null,
      ].filter(Boolean),
    };
    return { payload, evidencePath: await writeEvidence(outDir, payload) };
  } catch (err) {
    runError = err instanceof Error ? err : new Error(String(err));
  } finally {
    for (const child of [...children].reverse()) await killProcessTree(child);
    cleanupOutcome.childrenStopped = true;
    if (markerEdit) {
      try {
        await markerEdit.restore();
        await assertByteIdenticalRestore([markerEdit]);
        cleanupOutcome.restoredMarker = true;
      } catch (restoreErr) {
        cleanupOutcome.restoreError = restoreErr instanceof Error ? restoreErr.message : String(restoreErr);
      }
    }
    if (daemonStartedByRunner && artifactPath) {
      try {
        await exec(artifactPath, ['daemon', 'stop', '--port', String(port)], { env: runEnv });
        cleanupOutcome.daemonStopped = true;
      } catch (stopErr) {
        cleanupOutcome.daemonStopError = stopErr instanceof Error ? stopErr.message : String(stopErr);
      }
    }
    if (runError) {
      const evidencePath = await recordRunFailure(outDir, buildFailurePayload({
        runKind: `${surface}-stable-loop`,
        startedAt,
        command,
        environment: environment ?? { sourceSha: await runGitSha(repoRoot).catch(() => 'unknown') },
        error: runError,
        cleanupOutcome,
        partial: {
          ...partial,
          artifact: manifest && artifactPath ? await buildArtifactRecord(artifactPath, manifest).catch(() => null) : null,
        },
      }));
      runError.evidencePath = evidencePath;
    }
  }
  if (runError) throw runError;
}

export async function runNegativeCase(name, { port, outDir, repoRoot, env }) {
  const startedAt = new Date().toISOString();
  const command = process.argv.join(' ');
  let environment = null;
  const tempDir = join(repoRoot, 'scripts', `.proof-rft-dx-tmp-${process.pid}`);
  let pass = false;
  let observedError = null;
  const sampleMeta = { negativeCase: name, sampleCount: 1, outcome: null };
  let artifactMeta = { path: 'N/A', manifestPath: 'N/A', sha256: 'N/A', manifestSha256: 'N/A', contractHash: 'N/A' };
  let runError = null;
  let expectedNegativeFailure = null;

  try {
    environment = await captureEnvironment(repoRoot);
    await mkdir(tempDir, { recursive: true });
    const targetDir = await resolveTargetDir(env);
    const artifactPath = defaultArtifactPath({ targetDir });
    const contractHash = await computeContractHash(repoRoot);
    artifactMeta = {
      path: artifactPath,
      manifestPath: manifestPathForArtifact(artifactPath),
      sha256: (await pathExists(artifactPath)) ? await sha256File(artifactPath) : 'N/A',
      manifestSha256: (await pathExists(manifestPathForArtifact(artifactPath))) ? await sha256File(manifestPathForArtifact(artifactPath)) : 'N/A',
      contractHash,
    };

    if (name === 'missing-artifact') {
      const missingPath = join(tempDir, 'missing-nexus42');
      artifactMeta = { path: missingPath, manifestPath: manifestPathForArtifact(missingPath), sha256: 'N/A', manifestSha256: 'N/A', contractHash };
      try { await assertCompatibleBackend({ artifactPath: missingPath, contractHash, protocolVersion: CURRENT_WRITER_PROTOCOL }); }
      catch (err) { observedError = err.message; pass = err instanceof BackendCompatibilityError && observedError.includes(REMEDIATION_COMMAND); }
    } else if (name === 'mismatched-contract') {
      const hostTriple = await getHostTriple();
      const fakeArtifact = join(tempDir, 'nexus42'); await writeFile(fakeArtifact, 'fake', 'utf8');
      const manifest = { artifactPath: fakeArtifact, sha256: await sha256File(fakeArtifact), targetTriple: hostTriple, packageVersion: '0.0.0-test', contractHash: 'b'.repeat(64), nativeApiVersion: null, writerProtocol: CURRENT_WRITER_PROTOCOL, dbSchemaRange: await computeDbSchemaRange(repoRoot) };
      await writeManifestAtomic(manifestPathForArtifact(fakeArtifact), manifest);
      artifactMeta = { path: fakeArtifact, manifestPath: manifestPathForArtifact(fakeArtifact), sha256: manifest.sha256, manifestSha256: await sha256File(manifestPathForArtifact(fakeArtifact)), contractHash: manifest.contractHash };
      try { await assertCompatibleBackend({ artifactPath: fakeArtifact, contractHash, protocolVersion: CURRENT_WRITER_PROTOCOL }); }
      catch (err) { observedError = err.message; pass = err instanceof BackendCompatibilityError && observedError.includes('contractHash'); }
    } else if (name === 'wrong-digest') {
      const hostTriple = await getHostTriple();
      const fakeArtifact = join(tempDir, 'nexus42'); await writeFile(fakeArtifact, 'fake', 'utf8');
      const manifest = { artifactPath: fakeArtifact, sha256: 'c'.repeat(64), targetTriple: hostTriple, packageVersion: '0.0.0-test', contractHash, nativeApiVersion: null, writerProtocol: CURRENT_WRITER_PROTOCOL, dbSchemaRange: await computeDbSchemaRange(repoRoot) };
      await writeManifestAtomic(manifestPathForArtifact(fakeArtifact), manifest);
      artifactMeta = { path: fakeArtifact, manifestPath: manifestPathForArtifact(fakeArtifact), sha256: await sha256File(fakeArtifact), manifestSha256: await sha256File(manifestPathForArtifact(fakeArtifact)), contractHash: manifest.contractHash };
      try { await assertCompatibleBackend({ artifactPath: fakeArtifact, contractHash, protocolVersion: CURRENT_WRITER_PROTOCOL }); }
      catch (err) { observedError = err.message; pass = err instanceof BackendCompatibilityError && observedError.includes('digest'); }
    } else if (name === 'incompatible-daemon') {
      const manifest = await assertCompatibleBackend({ artifactPath, contractHash, protocolVersion: CURRENT_WRITER_PROTOCOL });
      artifactMeta = await buildArtifactRecord(artifactPath, manifest);
      const stalePort = port + 7; const staleUrl = `http://127.0.0.1:${stalePort}`; let started = false;
      try {
        await startDaemonNonBlocking(artifactPath, stalePort, env); started = true;
        await waitForDaemonHealth(staleUrl, { expectedPackageVersion: manifest.packageVersion, deadlineMs: 120_000 });
        const statusOutput = (await exec(artifactPath, ['daemon', 'status', '--port', String(stalePort)], { env })).stdout;
        try {
          await assertCompatibleRunningDaemon({
            baseUrl: staleUrl,
            manifest: { ...manifest, sha256: 'd'.repeat(64) },
            port: stalePort,
            daemonStatusOutput: statusOutput,
          });
        } catch (err) {
          observedError = err.message;
          pass = Boolean(observedError.includes('stop')) || observedError.includes('digest');
        }
      } finally { if (started) await exec(artifactPath, ['daemon', 'stop', '--port', String(stalePort)]); }
    } else throw new Error(`Unknown negative case: ${name}`);

    sampleMeta.outcome = pass ? 'refused-as-expected' : 'unexpected-pass';
    const payload = {
      runKind: `negative-${name}`, pass, command, negativeCase: name, observedError, remediation: REMEDIATION_COMMAND,
      artifact: artifactMeta, sample: sampleMeta,
      timestamps: { utcStart: startedAt, utcEnd: new Date().toISOString() }, environment,
    };
    const evidencePath = await writeEvidence(outDir, payload);
    if (!pass) throw new Error(`Negative case ${name} did not fail as expected`);
    return { payload, evidencePath };
  } catch (err) {
    if (err instanceof Error && err.message.startsWith('Negative case')) {
      expectedNegativeFailure = err;
    } else {
      runError = err instanceof Error ? err : new Error(String(err));
    }
  } finally {
    const cleanupOutcome = { tempDirRemoved: false };
    try {
      await rm(tempDir, { recursive: true, force: true });
      cleanupOutcome.tempDirRemoved = true;
    } catch (rmErr) {
      cleanupOutcome.tempDirRemoveError = rmErr instanceof Error ? rmErr.message : String(rmErr);
    }
    if (runError) {
      await recordRunFailure(outDir, buildFailurePayload({
        runKind: `negative-${name}`,
        startedAt,
        command,
        environment: environment ?? { sourceSha: await runGitSha(repoRoot).catch(() => 'unknown') },
        error: runError,
        cleanupOutcome,
        partial: { negativeCase: name, observedError, artifact: artifactMeta, sample: sampleMeta },
      }));
    }
  }
  if (expectedNegativeFailure) throw expectedNegativeFailure;
  if (runError) throw runError;
}

export async function runBoundaryDemo({ port, outDir, repoRoot }) {
  const startedAt = new Date().toISOString();
  const command = process.argv.join(' ');
  let environment = null;
  const schemaProbe = join(repoRoot, 'schemas', '.proof-rft-dx-contract-touch.json');
  const touch = new TrackedEdit(schemaProbe);
  let refusalError = null;
  let manifest = null;
  let artifactPath = null;
  let runError = null;

  try {
    environment = await captureEnvironment(repoRoot);
    const targetDir = await resolveTargetDir(process.env);
    artifactPath = defaultArtifactPath({ targetDir });
    const contractHash = await computeContractHash(repoRoot);
    manifest = await assertCompatibleBackend({ artifactPath, contractHash, protocolVersion: CURRENT_WRITER_PROTOCOL });
    await touch.capture();
    await touch.write(`${JSON.stringify({ proof: 'touch' })}\n`);
    try { await assertCompatibleBackend({ artifactPath, contractHash: await computeContractHash(repoRoot), protocolVersion: CURRENT_WRITER_PROTOCOL }); }
    catch (err) { refusalError = err.message; }
    const refreshStarted = new Date().toISOString();
    const refreshedManifest = await refreshBackend({ profile: 'debug', targetDir, repoRoot });
    environment = await captureEnvironment(repoRoot, { includeCompiler: true });
    const reproof = await assertCompatibleBackend({ artifactPath, contractHash: await computeContractHash(repoRoot), protocolVersion: CURRENT_WRITER_PROTOCOL });
    const payload = {
      runKind: 'stable-vs-backend-boundary', pass: Boolean(refusalError?.includes(REMEDIATION_COMMAND)) && reproof.sha256 === refreshedManifest.sha256,
      command,
      boundary: {
        contractInputTouch: relative(repoRoot, schemaProbe),
        stableLoopRefusal: refusalError,
        refreshCommand: REMEDIATION_COMMAND,
        refresh: { utcStart: refreshStarted, utcEnd: new Date().toISOString(), manifestSha256: refreshedManifest.sha256 },
        reproofManifestSha256: reproof.sha256,
      },
      endpoint: { port },
      artifact: await buildArtifactRecord(artifactPath, reproof),
      timestamps: { utcStart: startedAt, utcEnd: new Date().toISOString() },
      environment,
    };
    return { payload, evidencePath: await writeEvidence(outDir, payload) };
  } catch (err) {
    runError = err instanceof Error ? err : new Error(String(err));
  } finally {
    const cleanupOutcome = { fixtureRestored: false };
    try {
      await touch.restore();
      await assertByteIdenticalRestore([touch]);
      cleanupOutcome.fixtureRestored = true;
    } catch (restoreErr) {
      cleanupOutcome.restoreError = restoreErr instanceof Error ? restoreErr.message : String(restoreErr);
    }
    if (runError) {
      await recordRunFailure(outDir, buildFailurePayload({
        runKind: 'stable-vs-backend-boundary',
        startedAt,
        command,
        environment: environment ?? { sourceSha: await runGitSha(repoRoot).catch(() => 'unknown') },
        error: runError,
        cleanupOutcome,
        partial: {
          boundary: { contractInputTouch: relative(repoRoot, schemaProbe), stableLoopRefusal: refusalError },
          artifact: manifest && artifactPath ? await buildArtifactRecord(artifactPath, manifest).catch(() => null) : null,
        },
      }));
    }
  }
  if (runError) throw runError;
}

export async function runRefreshReproof({ outDir, repoRoot }) {
  const startedAt = new Date().toISOString();
  const command = process.argv.join(' ');
  let environment = null;
  let artifactPath = null;

  try {
    environment = await captureEnvironment(repoRoot);
    const targetDir = await resolveTargetDir(process.env);
    artifactPath = defaultArtifactPath({ targetDir });
    const before = await readBackendManifest(artifactPath);
    const refreshed = await refreshBackend({ profile: 'debug', targetDir, repoRoot });
    environment = await captureEnvironment(repoRoot, { includeCompiler: true });
    const after = await assertCompatibleBackend({ artifactPath, contractHash: await computeContractHash(repoRoot), protocolVersion: CURRENT_WRITER_PROTOCOL });
    const payload = {
      runKind: 'refresh-reproof', pass: after.sha256 === refreshed.sha256, command,
      beforeSha256: before.sha256, afterSha256: after.sha256,
      artifact: await buildArtifactRecord(artifactPath, after),
      timestamps: { utcStart: startedAt, utcEnd: new Date().toISOString() },
      environment,
    };
    return { payload, evidencePath: await writeEvidence(outDir, payload) };
  } catch (err) {
    await recordRunFailure(outDir, buildFailurePayload({
      runKind: 'refresh-reproof',
      startedAt,
      command,
      environment: environment ?? { sourceSha: await runGitSha(repoRoot).catch(() => 'unknown') },
      error: err,
      cleanupOutcome: {},
      partial: { artifactPath },
    }));
    throw err;
  }
}

function printHelp() {
  console.log(`Usage:\n  node scripts/proof-rft-dx.mjs --surface <web|studio|shared-ui|desktop-web> --samples 30 --cold-samples 10 --port 18420 --out <dir>\n  node scripts/proof-rft-dx.mjs --negative <missing-artifact|mismatched-contract|wrong-digest|incompatible-daemon> --port 18420 --out <dir>\n  node scripts/proof-rft-dx.mjs --boundary-demo --port 18420 --out <dir>\n  node scripts/proof-rft-dx.mjs --refresh-reproof --out <dir>\n  node scripts/proof-rft-dx.mjs --surface web --inject-fail --samples 0 --cold-samples 0 --port 18420 --out <dir>`);
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) { printHelp(); return; }
  const repoRoot = getRepoRoot(); const env = { ...process.env };
  if (args.negative) {
    const outDir = requireOutDir(args);
    const result = await runNegativeCase(args.negative, { port: args.port, outDir, repoRoot, env });
    console.log(`negative ${args.negative}: PASS → ${result.evidencePath}`);
    return;
  }
  if (args.boundaryDemo) {
    const outDir = requireOutDir(args);
    const result = await runBoundaryDemo({ port: args.port, outDir, repoRoot });
    console.log(`boundary-demo: ${result.payload.pass ? 'PASS' : 'FAIL'} → ${result.evidencePath}`);
    return;
  }
  if (args.refreshReproof) {
    const outDir = requireOutDir(args);
    const result = await runRefreshReproof({ outDir, repoRoot });
    console.log(`refresh-reproof: ${result.payload.pass ? 'PASS' : 'FAIL'} → ${result.evidencePath}`);
    return;
  }
  if (!args.surface) throw new Error('--surface is required');
  const outDir = requireOutDir(args);
  const result = await runSurfaceLoop({
    surface: args.surface,
    samples: args.samples,
    coldSamples: args.coldSamples,
    port: args.port,
    outDir,
    repoRoot,
    env,
    injectFail: args.injectFail,
  });
  console.log(`${args.surface}: ${result.payload.pass ? 'PASS' : 'FAIL'} cargo=${result.payload.criteria['DX-1'].cargoTraceCount} → ${result.evidencePath}`);
  if (!result.payload.pass) process.exitCode = 1;
}

const isMain = process.argv[1] && pathToFileURL(process.argv[1]).href === import.meta.url;
if (isMain) {
  main().catch(() => {
    console.error('proof-rft-dx failed');
    process.exit(1);
  });
}
