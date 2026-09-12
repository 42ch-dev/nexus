#!/usr/bin/env node
/** RFT DX proof runner — cold/warm/no-Cargo evidence for web/studio/shared-ui/desktop-web. */
import { execFile, spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import { access, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { freemem, totalmem, arch, platform, release, version as nodeVersion } from 'node:os';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { promisify } from 'node:util';

import {
  BackendCompatibilityError,
  CURRENT_WRITER_PROTOCOL,
  RunningDaemonCompatibilityError,
  REMEDIATION_COMMAND,
  assertCompatibleBackend,
  assertCompatibleRunningDaemon,
  computeContractHash,
  computeDbSchemaRange,
  defaultArtifactPath,
  getRepoRoot,
  isDaemonCliStatusRunning,
  manifestPathForArtifact,
  readBackendManifest,
  refreshBackend,
  resolveDaemonEndpoint,
  resolveTargetDir,
  sha256File,
  validateDaemonHealth,
  waitForDaemonHealth,
  writeManifestAtomic,
} from './dev-backend-manifest.mjs';

const exec = promisify(execFile);
const GRAPH_PATH = '/v1/daemon/worlds/wld_proof_rft_dx/kb/graph';
const DEFAULT_DAEMON_PORT = 8420;

const CARGO_FAMILY = /(?:^|\/)(cargo|rustc|rustup|cc1|clang\+\+?|ld\.lld)(?:\s|$)/i;
const TAURI_FAMILY = /(?:^|\/)(tauri|cargo-tauri)(?:\s|$)/i;
const NATIVE_BUILD = /(?:^|\/)(cmake|ninja|make|meson)(?:\s|$)/i;

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
    servedProbePath: '/', devCommand: ['pnpm', '--filter', 'web', 'dev'],
  },
};

export function parseArgs(argv) {
  const result = { surface: null, negative: null, boundaryDemo: false, refreshReproof: false, samples: 30, coldSamples: 10, port: DEFAULT_DAEMON_PORT, out: null, help: false };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--surface') result.surface = argv[++i];
    else if (arg === '--negative') result.negative = argv[++i];
    else if (arg === '--boundary-demo') result.boundaryDemo = true;
    else if (arg === '--refresh-reproof') result.refreshReproof = true;
    else if (arg === '--samples') result.samples = Number.parseInt(argv[++i], 10);
    else if (arg === '--cold-samples') result.coldSamples = Number.parseInt(argv[++i], 10);
    else if (arg === '--port') result.port = Number.parseInt(argv[++i], 10);
    else if (arg === '--out') result.out = argv[++i];
    else if (arg === '--help' || arg === '-h') result.help = true;
    else throw new Error(`Unknown argument: ${arg}`);
  }
  return result;
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
  return { processCount: traced.length, cargoCount: cargoHits.length, tauriCount: tauriHits.length, cargoCommands: cargoHits.map(r => r.command), tauriCommands: tauriHits.map(r => r.command), tracedPids: [...descendants].sort((a,b)=>a-b) };
}

export function evidenceFilename({ runKind, pass, startedAt }) { return `${startedAt.replace(/[:.]/g, '-')}-${runKind}-${pass ? 'pass' : 'fail'}.json`; }
export function markerSource(sample) { return `/** proof-rft-dx runner-owned marker */\nexport const PROOF_RFT_DX_MARKER = ${JSON.stringify(`sample-${sample}`)};\n`; }
export function markerProbeUrl(viteOrigin, config, repoRoot) {
  if (config.markerUrlPath) return `${viteOrigin}${config.markerUrlPath}`;
  const abs = join(repoRoot, config.markerRelative).replaceAll('\\', '/');
  return `${viteOrigin}/@fs${abs}`;
}


async function startDaemonNonBlocking(artifactPath, port, env) {
  await new Promise((resolve, reject) => {
    const child = spawn(artifactPath, ['daemon', 'start', '--port', String(port)], { env, stdio: 'ignore' });
    child.on('error', reject);
    child.on('close', () => resolve());
    setTimeout(resolve, 1000);
  });
}

async function sleep(ms) { await new Promise(r => setTimeout(r, ms)); }
async function pathExists(path) { try { await access(path); return true; } catch { return false; } }
async function runGitSha(repoRoot) { return (await exec('git', ['rev-parse', 'HEAD'], { cwd: repoRoot })).stdout.trim(); }
async function captureEnvironment(repoRoot) { return { os: { platform: platform(), arch: arch(), release: release(), node: nodeVersion() }, memory: { totalBytes: totalmem(), freeBytes: freemem() }, repoRoot, sourceSha: await runGitSha(repoRoot) }; }
async function sampleProcessTree(rootPid) { return countCargoTraces(rootPid, (await exec('ps', ['-eo', 'pid=,ppid=,command='])).stdout); }

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

async function killProcessTree(child) {
  if (!child || child.exitCode !== null) return;
  try { process.kill(child.pid, 'SIGTERM'); } catch { return; }
  await sleep(500);
  if (child.exitCode === null) { try { process.kill(child.pid, 'SIGKILL'); } catch {} }
  await new Promise(resolve => { child.on('close', resolve); setTimeout(resolve, 2000); });
}

function spawnLogged(command, args, { cwd, env, label }) {
  const child = spawn(command, args, { cwd, env, stdio: ['ignore', 'pipe', 'pipe'] });
  child.stdout?.on('data', c => process.stderr.write(`[${label}] ${c}`));
  child.stderr?.on('data', c => process.stderr.write(`[${label}] ${c}`));
  return child;
}

export async function writeEvidence(outDir, payload) {
  await mkdir(outDir, { recursive: true });
  const filename = evidenceFilename({ runKind: payload.runKind, pass: payload.pass, startedAt: payload.timestamps.utcStart });
  const target = join(outDir, filename);
  if (existsSync(target)) throw new Error(`Evidence file already exists: ${target}`);
  await writeFile(target, `${JSON.stringify(payload, null, 2)}\n`, 'utf8');
  return target;
}

async function ensureDaemon({ artifactPath, port, env }) {
  const manifest = await readBackendManifest(artifactPath);
  const { baseUrl } = resolveDaemonEndpoint({ portEnv: String(port), urlEnv: env.VITE_DAEMON_URL });
  const statusOutput = (await exec(artifactPath, ['daemon', 'status', '--port', String(port)], { env })).stdout;
  if (!isDaemonCliStatusRunning(statusOutput)) { await startDaemonNonBlocking(artifactPath, port, env); }
  await waitForDaemonHealth(baseUrl, { expectedPackageVersion: manifest.packageVersion, deadlineMs: 120_000 });
  if (isDaemonCliStatusRunning(statusOutput)) await assertCompatibleRunningDaemon({ baseUrl, manifest, port, daemonStatusOutput: statusOutput });
  return { baseUrl, manifest };
}

async function validateEndpointGraph(baseUrl) {
  const health = await validateDaemonHealth(baseUrl);
  const graphUrl = new URL(GRAPH_PATH, baseUrl);
  const graph = await fetchText(String(graphUrl));
  return { health, graph: { url: String(graphUrl), status: graph.status, body: JSON.parse(graph.body) } };
}

async function runSidecarBaseline(repoRoot) {
  const startedAt = new Date().toISOString(); const t0 = performance.now();
  await new Promise((resolve, reject) => {
    const child = spawn('bash', ['scripts/fetch-sidecar.sh'], { cwd: repoRoot, env: { ...process.env, SIDECAR_PROFILE: 'debug' }, stdio: 'inherit' });
    child.on('error', reject); child.on('close', code => (code === 0 ? resolve() : reject(new Error(`sidecar exit ${code}`))));
  });
  return { kind: 'desktop-web-sidecar-baseline', durationMs: performance.now() - t0, startedAt, endedAt: new Date().toISOString() };
}

export async function runSurfaceLoop(options) {
  const { surface, samples, coldSamples, port, outDir, repoRoot, env: baseEnv } = options;
  const config = SURFACE_CONFIG[surface];
  const startedAt = new Date().toISOString();
  const environment = await captureEnvironment(repoRoot);
  const targetDir = await resolveTargetDir(baseEnv);
  const artifactPath = defaultArtifactPath({ targetDir });
  const contractHash = await computeContractHash(repoRoot);
  const manifest = await assertCompatibleBackend({ artifactPath, contractHash, protocolVersion: CURRENT_WRITER_PROTOCOL });
  const env = { ...baseEnv, NEXUS42_DAEMON_PORT: String(port), VITE_DAEMON_URL: `http://127.0.0.1:${port}`, NEXUS42_ARTIFACT: artifactPath };
  const { baseUrl } = resolveDaemonEndpoint({ portEnv: env.NEXUS42_DAEMON_PORT, urlEnv: env.VITE_DAEMON_URL });
  const endpointCheck = config.needsDaemon ? await (async () => { await ensureDaemon({ artifactPath, port, env }); return await validateEndpointGraph(baseUrl); })() : null;
  const sidecarBaseline = config.needsSidecar ? await runSidecarBaseline(repoRoot) : null;
  const markerPath = join(repoRoot, config.markerRelative);
  const markerEdit = new TrackedEdit(markerPath); await markerEdit.capture();
  const children = []; const viteOrigin = `http://127.0.0.1:${config.vitePort}`; let daemonStartedByRunner = false;
  try {
    if (config.needsDaemon) {
      const statusOutput = (await exec(artifactPath, ['daemon', 'status', '--port', String(port)], { env })).stdout;
      if (!isDaemonCliStatusRunning(statusOutput)) { daemonStartedByRunner = true; await ensureDaemon({ artifactPath, port, env }); }
    }
    if (config.needsUiWatcher) { children.push(spawnLogged(config.uiWatcherCommand[0], config.uiWatcherCommand.slice(1), { cwd: repoRoot, env, label: 'nexus-ui-dev' })); await sleep(3000); }
    const warmSamples = []; const coldSampleValues = []; const dx1Traces = [];
    const runViteChild = () => spawnLogged(config.devCommand[0], config.devCommand.slice(1), { cwd: repoRoot, env, label: surface });
    let viteChild = runViteChild(); children.push(viteChild);
    for (let i = 0; i < coldSamples; i++) { if (viteChild) await killProcessTree(viteChild); viteChild = runViteChild(); children.push(viteChild); const t0 = performance.now(); await waitForHttpOk(`${viteOrigin}${config.servedProbePath}`); coldSampleValues.push(performance.now() - t0); }
    if (viteChild) await killProcessTree(viteChild); viteChild = runViteChild(); children.push(viteChild); await waitForHttpOk(`${viteOrigin}${config.servedProbePath}`);
    for (let i = 0; i < 3 + samples; i++) { await markerEdit.write(markerSource(i)); const t0 = performance.now(); await waitForMarker(viteOrigin, config, repoRoot, i); const elapsed = performance.now() - t0; dx1Traces.push(await sampleProcessTree(viteChild.pid)); if (i >= 3) warmSamples.push(elapsed); }
    const dx1CargoTotal = dx1Traces.reduce((s, t) => s + t.cargoCount, 0);
    const dx1TauriTotal = dx1Traces.reduce((s, t) => s + t.tauriCount, 0);
    const dx2 = evaluateDx2(warmSamples); const dx3 = evaluateDx3(coldSampleValues);
    const dx1Pass = dx1CargoTotal === 0 && (surface !== 'desktop-web' || dx1TauriTotal === 0);
    const payload = {
      runKind: `${surface}-stable-loop`, pass: dx1Pass && dx2.pass && dx3.pass, command: process.argv.join(' '), surface,
      criteria: { 'DX-1': { pass: dx1Pass, cargoTraceCount: dx1CargoTotal, tauriTraceCount: dx1TauriTotal, samples: dx1Traces }, 'DX-2': { ...dx2, samplesMs: warmSamples }, 'DX-3': { ...dx3, samplesMs: coldSampleValues } },
      endpoint: config.needsDaemon ? { port, baseUrl, health: endpointCheck.health, graph: endpointCheck.graph } : null,
      sidecarBaseline, artifact: { path: artifactPath, manifestPath: manifestPathForArtifact(artifactPath), sha256: manifest.sha256, contractHash: manifest.contractHash },
      timestamps: { utcStart: startedAt, utcEnd: new Date().toISOString() }, environment,
      observedOutcomes: { warmSampleCount: warmSamples.length, coldSampleCount: coldSampleValues.length },
      notes: ['HTTP fetch of Vite-served marker module; no browser automation.'],
    };
    return { payload, evidencePath: await writeEvidence(outDir, payload) };
  } finally {
    for (const child of [...children].reverse()) await killProcessTree(child);
    await markerEdit.restore(); await assertByteIdenticalRestore([markerEdit]);
    if (daemonStartedByRunner) { try { await exec(artifactPath, ['daemon', 'stop', '--port', String(port)]); } catch {} }
  }
}

export async function runNegativeCase(name, { port, outDir, repoRoot, env }) {
  const startedAt = new Date().toISOString(); const environment = await captureEnvironment(repoRoot);
  const targetDir = await resolveTargetDir(env); const artifactPath = defaultArtifactPath({ targetDir });
  const contractHash = await computeContractHash(repoRoot); let pass = false; let observedError = null;
  const tempDir = join(repoRoot, 'scripts', `.proof-rft-dx-tmp-${process.pid}`); await mkdir(tempDir, { recursive: true });
  try {
    if (name === 'missing-artifact') {
      try { await assertCompatibleBackend({ artifactPath: join(tempDir, 'missing-nexus42'), contractHash, protocolVersion: CURRENT_WRITER_PROTOCOL }); }
      catch (err) { observedError = err.message; pass = err instanceof BackendCompatibilityError && observedError.includes(REMEDIATION_COMMAND); }
    } else if (name === 'mismatched-contract') {
      const fakeArtifact = join(tempDir, 'nexus42'); await writeFile(fakeArtifact, 'fake', 'utf8');
      const manifest = { artifactPath: fakeArtifact, sha256: await sha256File(fakeArtifact), targetTriple: 'test', packageVersion: '0.0.0-test', contractHash: 'b'.repeat(64), nativeApiVersion: null, writerProtocol: CURRENT_WRITER_PROTOCOL, dbSchemaRange: await computeDbSchemaRange(repoRoot) };
      await writeManifestAtomic(manifestPathForArtifact(fakeArtifact), manifest);
      try { await assertCompatibleBackend({ artifactPath: fakeArtifact, contractHash, protocolVersion: CURRENT_WRITER_PROTOCOL }); }
      catch (err) { observedError = err.message; pass = err instanceof BackendCompatibilityError && observedError.includes('contractHash'); }
    } else if (name === 'wrong-digest') {
      const fakeArtifact = join(tempDir, 'nexus42'); await writeFile(fakeArtifact, 'fake', 'utf8');
      const manifest = { artifactPath: fakeArtifact, sha256: 'c'.repeat(64), targetTriple: 'test', packageVersion: '0.0.0-test', contractHash, nativeApiVersion: null, writerProtocol: CURRENT_WRITER_PROTOCOL, dbSchemaRange: await computeDbSchemaRange(repoRoot) };
      await writeManifestAtomic(manifestPathForArtifact(fakeArtifact), manifest);
      try { await assertCompatibleBackend({ artifactPath: fakeArtifact, contractHash, protocolVersion: CURRENT_WRITER_PROTOCOL }); }
      catch (err) { observedError = err.message; pass = err instanceof BackendCompatibilityError && observedError.includes('digest'); }
    } else if (name === 'incompatible-daemon') {
      const manifest = await assertCompatibleBackend({ artifactPath, contractHash, protocolVersion: CURRENT_WRITER_PROTOCOL });
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
          pass = observedError.includes('stop') || observedError.includes('digest');
        }
      } finally { if (started) await exec(artifactPath, ['daemon', 'stop', '--port', String(stalePort)]); }
    } else throw new Error(`Unknown negative case: ${name}`);
    const payload = { runKind: `negative-${name}`, pass, command: process.argv.join(' '), negativeCase: name, observedError, remediation: REMEDIATION_COMMAND, timestamps: { utcStart: startedAt, utcEnd: new Date().toISOString() }, environment };
    const evidencePath = await writeEvidence(outDir, payload);
    if (!pass) throw new Error(`Negative case ${name} did not fail as expected`);
    return { payload, evidencePath };
  } finally { await rm(tempDir, { recursive: true, force: true }); }
}

export async function runBoundaryDemo({ port, outDir, repoRoot }) {
  const startedAt = new Date().toISOString(); const environment = await captureEnvironment(repoRoot);
  const targetDir = await resolveTargetDir(process.env); const artifactPath = defaultArtifactPath({ targetDir });
  const contractHash = await computeContractHash(repoRoot);
  await assertCompatibleBackend({ artifactPath, contractHash, protocolVersion: CURRENT_WRITER_PROTOCOL });
  const schemaProbe = join(repoRoot, 'schemas', '.proof-rft-dx-contract-touch.json');
  const touch = new TrackedEdit(schemaProbe); await touch.capture(); let refusalError = null;
  try {
    await touch.write(`${JSON.stringify({ proof: 'touch' })}\n`);
    try { await assertCompatibleBackend({ artifactPath, contractHash: await computeContractHash(repoRoot), protocolVersion: CURRENT_WRITER_PROTOCOL }); }
    catch (err) { refusalError = err.message; }
  } finally { await touch.restore(); await assertByteIdenticalRestore([touch]); }
  const refreshStarted = new Date().toISOString();
  const refreshedManifest = await refreshBackend({ profile: 'debug', targetDir, repoRoot });
  const reproof = await assertCompatibleBackend({ artifactPath, contractHash: await computeContractHash(repoRoot), protocolVersion: CURRENT_WRITER_PROTOCOL });
  const payload = { runKind: 'stable-vs-backend-boundary', pass: Boolean(refusalError?.includes(REMEDIATION_COMMAND)) && reproof.sha256 === refreshedManifest.sha256, command: process.argv.join(' '), boundary: { contractInputTouch: relative(repoRoot, schemaProbe), stableLoopRefusal: refusalError, refreshCommand: REMEDIATION_COMMAND, refresh: { utcStart: refreshStarted, utcEnd: new Date().toISOString(), manifestSha256: refreshedManifest.sha256 }, reproofManifestSha256: reproof.sha256 }, endpoint: { port }, timestamps: { utcStart: startedAt, utcEnd: new Date().toISOString() }, environment };
  return { payload, evidencePath: await writeEvidence(outDir, payload) };
}

export async function runRefreshReproof({ outDir, repoRoot }) {
  const startedAt = new Date().toISOString(); const environment = await captureEnvironment(repoRoot);
  const targetDir = await resolveTargetDir(process.env); const artifactPath = defaultArtifactPath({ targetDir });
  const before = await readBackendManifest(artifactPath);
  const refreshed = await refreshBackend({ profile: 'debug', targetDir, repoRoot });
  const after = await assertCompatibleBackend({ artifactPath, contractHash: await computeContractHash(repoRoot), protocolVersion: CURRENT_WRITER_PROTOCOL });
  const payload = { runKind: 'refresh-reproof', pass: after.sha256 === refreshed.sha256, command: process.argv.join(' '), beforeSha256: before.sha256, afterSha256: after.sha256, timestamps: { utcStart: startedAt, utcEnd: new Date().toISOString() }, environment };
  return { payload, evidencePath: await writeEvidence(outDir, payload) };
}

function printHelp() {
  console.log(`Usage:\n  node scripts/proof-rft-dx.mjs --surface <web|studio|shared-ui|desktop-web> --samples 30 --cold-samples 10 --port 18420 --out <dir>\n  node scripts/proof-rft-dx.mjs --negative <missing-artifact|mismatched-contract|wrong-digest|incompatible-daemon> --port 18420 --out <dir>\n  node scripts/proof-rft-dx.mjs --boundary-demo --port 18420 --out <dir>\n  node scripts/proof-rft-dx.mjs --refresh-reproof --out <dir>`);
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) { printHelp(); return; }
  const repoRoot = getRepoRoot(); const env = { ...process.env };
  if (args.negative) { const result = await runNegativeCase(args.negative, { port: args.port, outDir: resolve(args.out), repoRoot, env }); console.log(`negative ${args.negative}: PASS → ${result.evidencePath}`); return; }
  if (args.boundaryDemo) { const result = await runBoundaryDemo({ port: args.port, outDir: resolve(args.out), repoRoot }); console.log(`boundary-demo: ${result.payload.pass ? 'PASS' : 'FAIL'} → ${result.evidencePath}`); return; }
  if (args.refreshReproof) { const result = await runRefreshReproof({ outDir: resolve(args.out), repoRoot }); console.log(`refresh-reproof: ${result.payload.pass ? 'PASS' : 'FAIL'} → ${result.evidencePath}`); return; }
  if (!args.surface || !args.out) throw new Error('--surface and --out are required');
  const result = await runSurfaceLoop({ surface: args.surface, samples: args.samples, coldSamples: args.coldSamples, port: args.port, outDir: resolve(args.out), repoRoot, env });
  console.log(`${args.surface}: ${result.payload.pass ? 'PASS' : 'FAIL'} cargo=${result.payload.criteria['DX-1'].cargoTraceCount} → ${result.evidencePath}`);
  if (!result.payload.pass) process.exitCode = 1;
}

const isMain = process.argv[1] && pathToFileURL(process.argv[1]).href === import.meta.url;
if (isMain) { main().catch(err => { console.error(err.stack ?? err.message ?? err); process.exit(1); }); }
