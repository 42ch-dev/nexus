/** Stable backend artifact manifest, compatibility checks, and dev endpoint derivation. */
import { createHash } from 'node:crypto';
import { execFile, spawn } from 'node:child_process';
import { createReadStream } from 'node:fs';
import { access, readFile, rename, writeFile } from 'node:fs/promises';
import { basename, dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { promisify } from 'node:util';

const exec = promisify(execFile);

export const REMEDIATION_COMMAND = 'pnpm dev:backend:refresh';
export const CURRENT_WRITER_PROTOCOL = 0;
export const DEFAULT_DAEMON_PORT = 8420;
export const DEFAULT_DAEMON_HOST = '127.0.0.1';
export const HEALTH_PATH = '/v1/daemon/runtime/health';

const MODULE_DIR = dirname(fileURLToPath(import.meta.url));

export function getRepoRoot() {
  return resolve(MODULE_DIR, '..');
}

export async function resolveTargetDir(env = process.env) {
  if (env.CARGO_TARGET_DIR) {
    return resolve(env.CARGO_TARGET_DIR);
  }
  const home = env.HOME ?? env.USERPROFILE ?? '';
  const shared = home ? join(home, '.cache', 'nexus-target') : '';
  if (shared && (await pathExists(shared))) {
    return shared;
  }
  return resolve(getRepoRoot(), 'target');
}

export function defaultArtifactPath({ profile = 'debug', targetDir } = {}) {
  const resolvedTargetDir = targetDir ?? resolve(getRepoRoot(), 'target');
  return join(resolvedTargetDir, profile, 'nexus42');
}

export function manifestPathForArtifact(artifactPath) {
  return `${artifactPath}.manifest.json`;
}

async function pathExists(path) {
  try {
    await access(path);
    return true;
  } catch {
    return false;
  }
}

async function filesUnder(directory, extension) {
  const { readdir } = await import('node:fs/promises');
  const files = [];
  if (!(await pathExists(directory))) return files;
  for (const item of await readdir(directory, { withFileTypes: true })) {
    const path = join(directory, item.name);
    if (item.isDirectory()) {
      files.push(...await filesUnder(path, extension));
    } else if (path.endsWith(extension)) {
      files.push(path);
    }
  }
  return files.sort();
}

export async function sha256File(filePath) {
  return new Promise((resolvePromise, reject) => {
    const hash = createHash('sha256');
    const stream = createReadStream(filePath);
    stream.on('data', chunk => hash.update(chunk));
    stream.on('error', reject);
    stream.on('end', () => resolvePromise(hash.digest('hex')));
  });
}

export async function computeContractHash(repoRoot = getRepoRoot()) {
  const inputs = [
    ...(await filesUnder(join(repoRoot, 'schemas'), '.json')),
    ...(await filesUnder(join(repoRoot, 'crates', 'nexus-local-db', 'migrations'), '.sql')),
  ].sort();
  const parts = [];
  for (const file of inputs) {
    const rel = relative(repoRoot, file);
    const digest = createHash('sha256').update(await readFile(file)).digest('hex');
    parts.push(`${rel}\0${digest}`);
  }
  return createHash('sha256').update(parts.join('\n')).digest('hex');
}

export function computeDbSchemaRangeFromNames(names) {
  const sorted = [...names].sort();
  if (!sorted.length) {
    throw new Error('No local-db migrations found for dbSchemaRange');
  }
  return { min: sorted[0], max: sorted[sorted.length - 1] };
}

export async function computeDbSchemaRange(repoRoot = getRepoRoot()) {
  const migrations = await filesUnder(join(repoRoot, 'crates', 'nexus-local-db', 'migrations'), '.sql');
  const names = migrations.map(path => basename(path, '.sql'));
  return computeDbSchemaRangeFromNames(names);
}

export async function getHostTriple(env = process.env) {
  const { stdout } = await exec('rustc', ['-vV'], { env });
  const line = stdout.split('\n').find(row => row.startsWith('host: '));
  if (!line) throw new Error('Unable to resolve Rust host triple from rustc -vV');
  return line.slice('host: '.length).trim();
}

export async function getPackageVersion(repoRoot = getRepoRoot()) {
  const cargoToml = await readFile(join(repoRoot, 'Cargo.toml'), 'utf8');
  const match = cargoToml.match(/^\[workspace\.package\][\s\S]*?^version\s*=\s*"([^"]+)"/m);
  if (!match) throw new Error('Unable to resolve workspace package version from Cargo.toml');
  return match[1];
}

function validateManifestShape(raw) {
  if (!raw || typeof raw !== 'object') {
    throw new Error('Backend manifest must be a JSON object');
  }
  const required = [
    'artifactPath',
    'sha256',
    'targetTriple',
    'packageVersion',
    'contractHash',
    'nativeApiVersion',
    'writerProtocol',
    'dbSchemaRange',
  ];
  for (const key of required) {
    if (!(key in raw)) throw new Error(`Backend manifest missing required field: ${key}`);
  }
  if (typeof raw.artifactPath !== 'string' || !raw.artifactPath) {
    throw new Error('Backend manifest artifactPath must be a non-empty string');
  }
  if (typeof raw.sha256 !== 'string' || !/^[a-f0-9]{64}$/.test(raw.sha256)) {
    throw new Error('Backend manifest sha256 must be a 64-character hex digest');
  }
  if (typeof raw.targetTriple !== 'string' || !raw.targetTriple) {
    throw new Error('Backend manifest targetTriple must be a non-empty string');
  }
  if (typeof raw.packageVersion !== 'string' || !raw.packageVersion) {
    throw new Error('Backend manifest packageVersion must be a non-empty string');
  }
  if (typeof raw.contractHash !== 'string' || !/^[a-f0-9]{64}$/.test(raw.contractHash)) {
    throw new Error('Backend manifest contractHash must be a 64-character hex digest');
  }
  if (!(raw.nativeApiVersion === null || typeof raw.nativeApiVersion === 'string')) {
    throw new Error('Backend manifest nativeApiVersion must be null or a string');
  }
  if (!Number.isInteger(raw.writerProtocol) || raw.writerProtocol < 0) {
    throw new Error('Backend manifest writerProtocol must be a non-negative integer');
  }
  if (!raw.dbSchemaRange || typeof raw.dbSchemaRange !== 'object') {
    throw new Error('Backend manifest dbSchemaRange must be an object');
  }
  if (typeof raw.dbSchemaRange.min !== 'string' || typeof raw.dbSchemaRange.max !== 'string') {
    throw new Error('Backend manifest dbSchemaRange.min/max must be strings');
  }
}

export function formatRemediation(message) {
  return `${message} Remediation: ${REMEDIATION_COMMAND}`;
}

export class BackendCompatibilityError extends Error {
  constructor(message) {
    super(formatRemediation(message));
    this.name = 'BackendCompatibilityError';
  }
}

export function parsePortValue(raw, label = 'port') {
  if (typeof raw !== 'string' || raw === '') {
    throw new Error(`Invalid ${label}: ${raw}`);
  }
  if (!/^\d+$/.test(raw)) {
    throw new Error(`Invalid ${label}: ${raw}`);
  }
  const port = Number.parseInt(raw, 10);
  if (port <= 0 || port > 65535) {
    throw new Error(`Invalid ${label}: ${raw}`);
  }
  return port;
}

export class RunningDaemonCompatibilityError extends Error {
  constructor(message, { port } = {}) {
    super(message);
    this.name = 'RunningDaemonCompatibilityError';
    this.port = port;
  }
}

export async function readBackendManifest(artifactPath) {
  const manifestPath = manifestPathForArtifact(artifactPath);
  const raw = JSON.parse(await readFile(manifestPath, 'utf8'));
  validateManifestShape(raw);
  return {
    artifactPath: raw.artifactPath,
    sha256: raw.sha256,
    targetTriple: raw.targetTriple,
    packageVersion: raw.packageVersion,
    contractHash: raw.contractHash,
    nativeApiVersion: raw.nativeApiVersion,
    writerProtocol: raw.writerProtocol,
    dbSchemaRange: {
      min: raw.dbSchemaRange.min,
      max: raw.dbSchemaRange.max,
    },
  };
}

export async function assertCompatibleBackend({ artifactPath, contractHash, protocolVersion }) {
  if (!(await pathExists(artifactPath))) {
    throw new BackendCompatibilityError(`Backend artifact is missing at ${artifactPath}.`);
  }

  const manifestPath = manifestPathForArtifact(artifactPath);
  if (!(await pathExists(manifestPath))) {
    throw new BackendCompatibilityError(`Backend manifest is missing at ${manifestPath}.`);
  }

  let manifest;
  try {
    manifest = await readBackendManifest(artifactPath);
  } catch (err) {
    const detail = err instanceof Error ? err.message : String(err);
    throw new BackendCompatibilityError(`Backend manifest at ${manifestPath} is invalid (${detail}).`);
  }

  const requestedArtifactPath = resolve(artifactPath);
  const recordedArtifactPath = resolve(manifest.artifactPath);
  if (requestedArtifactPath !== recordedArtifactPath) {
    throw new BackendCompatibilityError(
      `Backend manifest artifactPath (${manifest.artifactPath}) does not identify the requested artifact (${artifactPath}).`,
    );
  }

  const digest = await sha256File(artifactPath);
  if (digest !== manifest.sha256) {
    throw new BackendCompatibilityError('Backend artifact digest does not match manifest sha256.');
  }
  if (manifest.contractHash !== contractHash) {
    throw new BackendCompatibilityError('Backend manifest contractHash does not match current contract hash.');
  }
  if (manifest.writerProtocol !== protocolVersion) {
    throw new BackendCompatibilityError(
      `Backend manifest writerProtocol ${manifest.writerProtocol} does not match required protocol ${protocolVersion}.`,
    );
  }

  const currentRange = await computeDbSchemaRange();
  if (
    manifest.dbSchemaRange.min !== currentRange.min ||
    manifest.dbSchemaRange.max !== currentRange.max
  ) {
    throw new BackendCompatibilityError('Backend manifest dbSchemaRange does not match current migrations.');
  }

  return manifest;
}

export async function writeManifestAtomic(manifestPath, manifest) {
  const dir = dirname(manifestPath);
  const tmp = join(dir, `.${basename(manifestPath)}.${process.pid}.tmp`);
  await writeFile(tmp, `${JSON.stringify(manifest, null, 2)}\n`, 'utf8');
  await rename(tmp, manifestPath);
}

async function runCommand(command, args, { cwd, env = process.env } = {}) {
  await new Promise((resolvePromise, reject) => {
    const child = spawn(command, args, { cwd, env, stdio: 'inherit' });
    child.on('error', reject);
    child.on('close', code => {
      if (code === 0) resolvePromise();
      else reject(new Error(`${command} ${args.join(' ')} exited with code ${code}`));
    });
  });
}

async function maybeRunCodegen(repoRoot, previousContractHash, nextContractHash) {
  if (previousContractHash === nextContractHash) return;
  await runCommand('pnpm', ['run', 'codegen'], { cwd: repoRoot, env: { ...process.env, SQLX_OFFLINE: 'true' } });
}

export async function refreshBackend({
  profile = 'debug',
  targetDir,
  repoRoot = getRepoRoot(),
  commandRunner = runCommand,
} = {}) {
  const resolvedTargetDir = targetDir ?? (await resolveTargetDir());
  const artifactPath = defaultArtifactPath({ profile, targetDir: resolvedTargetDir });
  const manifestPath = manifestPathForArtifact(artifactPath);
  const contractHash = await computeContractHash(repoRoot);

  let previousContractHash = null;
  if (await pathExists(manifestPath)) {
    try {
      previousContractHash = (await readBackendManifest(artifactPath)).contractHash;
    } catch {
      previousContractHash = null;
    }
  }

  await maybeRunCodegen(repoRoot, previousContractHash, contractHash);

  const cargoArgs = ['build', '-p', 'nexus42'];
  if (profile === 'release') {
    cargoArgs.push('--release');
  }
  await commandRunner('cargo', cargoArgs, {
    cwd: repoRoot,
    env: { ...process.env, CARGO_TARGET_DIR: resolvedTargetDir },
  });

  if (!(await pathExists(artifactPath))) {
    throw new Error(`Expected backend artifact at ${artifactPath} after cargo build`);
  }

  const manifest = {
    artifactPath,
    sha256: await sha256File(artifactPath),
    targetTriple: await getHostTriple(),
    packageVersion: await getPackageVersion(repoRoot),
    contractHash,
    nativeApiVersion: null,
    writerProtocol: CURRENT_WRITER_PROTOCOL,
    dbSchemaRange: await computeDbSchemaRange(repoRoot),
  };

  await writeManifestAtomic(manifestPath, manifest);
  return manifest;
}

export function resolveDaemonEndpoint({ portEnv, urlEnv } = {}) {
  const portRaw = portEnv ?? '';
  const urlRaw = urlEnv ?? '';
  const hasPort = portRaw !== '';
  const hasUrl = urlRaw !== '';

  if (hasPort && hasUrl) {
    const port = parsePortValue(portRaw, 'NEXUS42_DAEMON_PORT');
    let parsed;
    try {
      parsed = new URL(urlRaw);
    } catch {
      throw new Error(`Invalid VITE_DAEMON_URL: ${urlRaw}`);
    }
    if (parsed.port && parsePortValue(parsed.port, 'VITE_DAEMON_URL port') !== port) {
      throw new Error(
        `NEXUS42_DAEMON_PORT (${port}) conflicts with VITE_DAEMON_URL port (${parsed.port || 'default'}).`,
      );
    }
    if (!parsed.port) {
      parsed.port = String(port);
    }
    return { baseUrl: parsed.origin, port };
  }

  if (hasUrl) {
    const parsed = new URL(urlRaw);
    const port = parsed.port
      ? parsePortValue(parsed.port, 'VITE_DAEMON_URL port')
      : DEFAULT_DAEMON_PORT;
    if (!parsed.port) {
      parsed.port = String(port);
    }
    return { baseUrl: parsed.origin, port };
  }

  const port = hasPort ? parsePortValue(portRaw, 'NEXUS42_DAEMON_PORT') : DEFAULT_DAEMON_PORT;
  const baseUrl = `http://${DEFAULT_DAEMON_HOST}:${port}`;
  return { baseUrl, port };
}

export function formatRunningDaemonRefusal({ baseUrl, port, reason }) {
  return (
    `Incompatible daemon already running at ${baseUrl}: ${reason} ` +
    `Stop it with: nexus42 daemon stop --port ${port}. ` +
    `Then refresh the backend if needed (${REMEDIATION_COMMAND}) and restart dev.`
  );
}

export async function validateDaemonHealth(
  baseUrl,
  { timeoutMs = 5000, fetchImpl = globalThis.fetch, expectedPackageVersion } = {},
) {
  if (typeof fetchImpl !== 'function') {
    throw new Error('fetch is unavailable for daemon health validation');
  }
  const url = new URL(HEALTH_PATH, baseUrl);
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  try {
    const response = await fetchImpl(url, { signal: controller.signal });
    if (!response.ok) {
      throw new Error(`Daemon health request failed with HTTP ${response.status} at ${url}`);
    }
    const body = await response.text();
    let health;
    try {
      health = JSON.parse(body);
    } catch {
      throw new Error(`Daemon health response at ${url} was not valid JSON`);
    }
    if (health?.status !== 'ok') {
      throw new Error(
        `Daemon health response at ${url} reported status ${JSON.stringify(health?.status)} (expected "ok")`,
      );
    }
    if (expectedPackageVersion !== undefined && health.version !== expectedPackageVersion) {
      throw new Error(
        `Daemon health version ${JSON.stringify(health.version)} does not match expected package version ${expectedPackageVersion}`,
      );
    }
    return { url: String(url), status: response.status, body, health };
  } catch (err) {
    if (err.name === 'AbortError') {
      throw new Error(`Daemon health request timed out after ${timeoutMs}ms at ${url}`);
    }
    throw err;
  } finally {
    clearTimeout(timer);
  }
}

export async function assertCompatibleRunningDaemon({
  baseUrl,
  manifest,
  port,
  fetchImpl = globalThis.fetch,
}) {
  try {
    await validateDaemonHealth(baseUrl, {
      fetchImpl,
      expectedPackageVersion: manifest.packageVersion,
    });
  } catch (err) {
    const reason = err instanceof Error ? err.message : String(err);
    throw new RunningDaemonCompatibilityError(formatRunningDaemonRefusal({ baseUrl, port, reason }), {
      port,
    });
  }
}

export async function prepareDevCliWebEnvironment(env = process.env) {
  const repoRoot = getRepoRoot();
  const targetDir = await resolveTargetDir(env);
  const artifactPath = defaultArtifactPath({ targetDir });
  const contractHash = await computeContractHash(repoRoot);

  const manifest = await assertCompatibleBackend({
    artifactPath,
    contractHash,
    protocolVersion: CURRENT_WRITER_PROTOCOL,
  });

  const { baseUrl, port } = resolveDaemonEndpoint({
    portEnv: env.NEXUS42_DAEMON_PORT,
    urlEnv: env.VITE_DAEMON_URL,
  });

  return {
    repoRoot,
    targetDir,
    artifactPath,
    manifest,
    baseUrl,
    port,
    contractHash,
  };
}

export async function runDevCliWebPreflight(env = process.env) {
  const prepared = await prepareDevCliWebEnvironment(env);
  process.stdout.write(`export VITE_DAEMON_URL=${JSON.stringify(prepared.baseUrl)}\n`);
  process.stdout.write(`export NEXUS42_DAEMON_PORT=${JSON.stringify(String(prepared.port))}\n`);
  process.stdout.write(`export NEXUS42_ARTIFACT=${JSON.stringify(prepared.artifactPath)}\n`);
  return prepared;
}

const isMain = process.argv[1] && pathToFileURL(process.argv[1]).href === import.meta.url;
if (isMain && process.argv[2] === '--preflight') {
  runDevCliWebPreflight().catch(err => {
    console.error(err.message ?? err);
    process.exit(1);
  });
}
