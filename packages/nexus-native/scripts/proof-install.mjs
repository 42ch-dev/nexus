#!/usr/bin/env node
/**
 * P3-T1 — empty-project install/run proof for one native target package.
 *
 * What this proves (PKG-1):
 *   - the local tarballs install into an empty, lockfile-free project with NO
 *     compiler/rustup available and no install-time script execution;
 *   - the installed `@42ch/nexus-native` facade fences the shipped manifest
 *     against the real artifact and then executes real native graph / create /
 *     update / provider / cancel / close work through the installed payload;
 *   - wrong package metadata, wrong target/version/hash and a missing or
 *     corrupted artifact are rejected BEFORE any database or effect, with the
 *     seeded home byte-identical afterwards.
 *
 * The consumer program is generated into the temporary project (it is part of
 * the proof, not of the product). The driver never imports the repo loader:
 * everything under test comes from the installed tarballs.
 */
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  renameSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { release as osRelease, tmpdir, totalmem, cpus } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPT = 'packages/nexus-native/scripts/proof-install.mjs';
const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..', '..', '..');
const IS_WIN = process.platform === 'win32';
const DELIMITER = IS_WIN ? ';' : ':';

const TARGETS = {
  'aarch64-apple-darwin': { suffix: 'darwin-arm64', os: 'darwin', cpu: 'arm64', workspace: 'default' },
  'x86_64-apple-darwin': { suffix: 'darwin-x64', os: 'darwin', cpu: 'x64', workspace: 'default' },
  'x86_64-pc-windows-msvc': { suffix: 'win32-x64-msvc', os: 'win32', cpu: 'x64', workspace: 'default' },
  'x86_64-unknown-linux-gnu': {
    suffix: 'linux-x64-gnu',
    os: 'linux',
    cpu: 'x64',
    libc: 'gnu',
    workspace: 'default',
  },
};

const DENIED_TOOLS = [
  'cargo',
  'rustc',
  'rustup',
  'cc',
  'gcc',
  'g++',
  'clang',
  'clang++',
  'cl',
  'link',
  'ld',
  'make',
  'cmake',
  'msbuild',
  'node-gyp',
  'xcodebuild',
];

const USAGE = `usage: node ${SCRIPT} --target <rust-triple> --node-version <x.y.z> --out <dir>
       [--pack-dir <dir>] [--fixture <path>] [--python <path>]

  --target        one of ${Object.keys(TARGETS).join(', ')}
  --node-version  the Node.js version this run must execute under (exact)
  --out           evidence directory (install-proof.json is written here)
  --pack-dir      reuse tarballs from a previous package.mjs run
                  (default: pack into <out>/../native-packages/<suffix>)
  --fixture       ACP fixture path (default: the repo mock_acp_workflow.py)
  --python        absolute interpreter for the ACP fixture (default: autodetect)`;

const state = { phase: 'setup', startedAt: new Date().toISOString(), evidenceWritten: false };

function fail(message, detail) {
  const suffix = detail === undefined ? '' : `\n${JSON.stringify(detail, null, 2)}`;
  process.stderr.write(`${SCRIPT}: ${message}${suffix}\n`);
  if (!state.evidenceWritten) {
    record('fatal', false, { message, detail: detail ?? null });
    try {
      writeEvidence({ fatal: message, fatal_detail: detail ?? null });
    } catch {
      // the failure is already on stderr; a failed evidence write must not mask it
    }
  }
  process.exit(1);
}

function run(command, args, options = {}) {
  const res = spawnSync(command, args, {
    cwd: options.cwd ?? ROOT,
    env: options.env ?? process.env,
    encoding: 'utf8',
    shell: options.shell ?? (IS_WIN && options.forceNoShell !== true),
    maxBuffer: 64 * 1024 * 1024,
  });
  if (res.error) {
    if (options.allowFailure) return { status: 127, stdout: '', stderr: res.error.message };
    fail(`failed to run ${command}: ${res.error.message}`);
  }
  return { status: res.status ?? 1, stdout: res.stdout ?? '', stderr: res.stderr ?? '' };
}

function sha256(buffer) {
  return createHash('sha256').update(buffer).digest('hex');
}

function sha256File(path) {
  return sha256(readFileSync(path));
}

/** Key-order independent comparison of packed and installed manifests. */
function canonicalJson(text) {
  const value = typeof text === 'string' ? JSON.parse(text) : text;
  return JSON.stringify(value, (_key, item) => {
    if (item === null || typeof item !== 'object' || Array.isArray(item)) return item;
    return Object.fromEntries(Object.entries(item).sort(([a], [b]) => a.localeCompare(b)));
  });
}

function detectLibc() {
  try {
    const report = process.report?.getReport?.();
    if (report?.header?.glibcVersionRuntime) return 'glibc';
  } catch {
    // fall through to musl
  }
  return 'musl';
}

function gitHeadSha() {
  const res = run('git', ['rev-parse', 'HEAD'], { allowFailure: true });
  return res.status === 0 ? res.stdout.trim() : 'unknown';
}

function parseArgs(argv) {
  const parsed = { target: null, nodeVersion: null, out: null, packDir: null, fixture: null, python: null };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    const value = () => {
      i += 1;
      if (i >= argv.length) fail(`missing value for ${arg}`);
      return argv[i];
    };
    if (arg === '--target') parsed.target = value();
    else if (arg === '--node-version') parsed.nodeVersion = value();
    else if (arg === '--out') parsed.out = value();
    else if (arg === '--pack-dir') parsed.packDir = value();
    else if (arg === '--fixture') parsed.fixture = value();
    else if (arg === '--python') parsed.python = value();
    else if (arg === '--help' || arg === '-h') {
      process.stdout.write(`${USAGE}\n`);
      process.exit(0);
    } else fail(`unknown argument ${arg}\n${USAGE}`);
  }
  if (!parsed.target || !parsed.nodeVersion || !parsed.out) fail(`--target, --node-version and --out are required\n${USAGE}`);
  if (!TARGETS[parsed.target]) fail(`unknown target ${parsed.target}`, { known: Object.keys(TARGETS) });
  return parsed;
}

// --- package/tarball helpers ----------------------------------------------

function tar(argv, options = {}) {
  const res = run('tar', argv, options);
  if (res.status !== 0 && options.allowFailure !== true) {
    fail(`tar ${argv.join(' ')} exited ${res.status}`, { stderr: res.stderr.slice(-2000) });
  }
  return res;
}

function packedManifest(tarball) {
  return JSON.parse(tar(['-xzOf', tarball, 'package/package.json']).stdout);
}

/**
 * Pack one workspace package into `packDir` and return the tarball path.
 *
 * Packing into a fresh directory is required: `pnpm pack` overwrites a
 * same-named tarball in place, so a "which new file appeared" diff against an
 * existing `packDir` sees nothing on a re-run and the driver fails before it
 * ever reaches the install step.
 */
function packInto(dir, packDir) {
  const staging = mkdtempSync(join(tmpdir(), 'nexus-install-pack-'));
  const res = run('pnpm', ['pack', '--pack-destination', staging], { cwd: dir });
  if (res.status !== 0) fail(`pnpm pack failed in ${dir}`, { stderr: res.stderr.slice(-2000) });
  const produced = readdirSync(staging).filter((name) => name.endsWith('.tgz'));
  if (produced.length !== 1) fail(`expected one tarball from ${dir}`, { produced });
  const tarball = join(packDir, produced[0]);
  copyFileSync(join(staging, produced[0]), tarball);
  rmSync(staging, { recursive: true, force: true });
  return tarball;
}

/** Find a workspace package directory by exact package name. */
function workspacePackageDir(name) {
  for (const group of ['packages', 'apps', 'tooling']) {
    const groupDir = join(ROOT, group);
    if (!existsSync(groupDir)) continue;
    for (const entry of readdirSync(groupDir)) {
      const manifestPath = join(groupDir, entry, 'package.json');
      if (!existsSync(manifestPath)) continue;
      try {
        if (JSON.parse(readFileSync(manifestPath, 'utf8')).name === name) return join(groupDir, entry);
      } catch {
        // not a manifest we can use
      }
    }
  }
  return null;
}

/**
 * Resolve the complete local install set: the loader, the target platform
 * package, and every non-optional dependency the packed loader declares. Any
 * dependency that is not a workspace package is an error — this proof never
 * reaches the registry.
 */
function buildInstallSet(packDir, receipt) {
  const set = new Map();
  for (const entry of receipt.packages) {
    set.set(entry.packed_manifest.name, entry.tarball);
  }
  const queue = [...set.keys()];
  while (queue.length > 0) {
    const name = queue.shift();
    const manifest = packedManifest(set.get(name));
    for (const [depName, spec] of Object.entries(manifest.dependencies ?? {})) {
      if (set.has(depName)) continue;
      if (typeof spec !== 'string' || spec.includes('workspace:')) {
        fail(`packed ${name} declares an unreplaced dependency spec`, { depName, spec });
      }
      const dir = workspacePackageDir(depName);
      if (!dir) {
        fail(`packed ${name} depends on ${depName}@${spec} which is not a workspace package`, {
          reason: 'the install proof resolves local tarballs only; it never contacts the registry',
        });
      }
      set.set(depName, packInto(dir, packDir));
      queue.push(depName);
    }
  }
  return set;
}

// --- environment denial ----------------------------------------------------

function writeDenyShims(projDir) {
  const denyBin = join(projDir, 'deny-bin');
  mkdirSync(denyBin, { recursive: true });
  for (const tool of DENIED_TOOLS) {
    if (IS_WIN) {
      writeFileSync(
        join(denyBin, `${tool}.cmd`),
        `@echo off\r\n>>"%NEXUS_DENY_LOG%" echo ${tool}\r\n>&2 echo denied: ${tool} is unavailable in the install proof\r\nexit /b 127\r\n`,
      );
    } else {
      writeFileSync(
        join(denyBin, tool),
        `#!/bin/sh\nprintf '%s\\n' "${tool}" >> "$NEXUS_DENY_LOG"\necho "denied: ${tool} is unavailable in the install proof" >&2\nexit 127\n`,
        { mode: 0o755 },
      );
    }
  }
  return denyBin;
}

/**
 * Child environment for install/run: a shim directory shadows every compiler
 * and rustup entry point, the rustup/cargo toolchain directories are dropped
 * from PATH, the toolchain env vars are removed, and CC/CXX/AR/LD are pointed
 * at the shims so a build tool that honours them is denied as well. Any
 * attempt to build is recorded in the deny log, so "no compiler ran" is
 * measured, not asserted.
 *
 * Only rustup/cargo directories are dropped: `/usr/bin` and `/bin` carry the
 * system compiler *and* the interpreter the ACP fixture needs, so those stay
 * in PATH with the shims shadowing them by precedence.
 */
function deniedEnv(denyBin, denyLog) {
  const toolchainDirs = ['cargo', 'rustc', 'rustup'];
  const entries = (process.env.PATH ?? '').split(DELIMITER).filter(Boolean);
  const removed = [];
  const kept = entries.filter((entry) => {
    const hit = toolchainDirs.some((tool) => {
      if (IS_WIN) return [`${tool}.exe`, `${tool}.cmd`].some((name) => existsSync(join(entry, name)));
      return existsSync(join(entry, tool));
    });
    if (hit) removed.push(entry);
    return !hit;
  });
  const env = { ...process.env, PATH: [denyBin, ...kept].join(DELIMITER), NEXUS_DENY_LOG: denyLog };
  for (const key of ['CARGO_HOME', 'RUSTUP_HOME', 'CARGO', 'RUSTC', 'RUSTDOC', 'RUSTFLAGS', 'CARGO_TARGET_DIR', 'MSBuild', 'MSVC', 'VSCMD_VER', 'VCToolsVersion', 'WindowsSdkDir', 'WindowsSDKVersion']) {
    delete env[key];
  }
  for (const key of ['CC', 'CXX', 'AR', 'LD']) {
    env[key] = join(denyBin, IS_WIN ? `${key.toLowerCase()}.cmd` : key.toLowerCase());
  }
  return { env, removed_path_entries: removed };
}

// --- home helpers ----------------------------------------------------------

function homeDigest(home) {
  const entries = [];
  const walk = (dir, prefix) => {
    for (const entry of readdirSync(dir, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
      const rel = prefix ? `${prefix}/${entry.name}` : entry.name;
      const abs = join(dir, entry.name);
      if (entry.isDirectory()) walk(abs, rel);
      else if (entry.isFile()) entries.push(`${rel} ${statSync(abs).size} ${sha256File(abs)}`);
      else entries.push(`${rel} ${entry.isSymbolicLink() ? 'symlink' : 'other'}`);
    }
  };
  walk(home, '');
  return { digest: sha256(entries.join('\n')), entries };
}

function fixtureStartPids(logPath) {
  if (!existsSync(logPath)) return [];
  return readFileSync(logPath, 'utf8')
    .split('\n')
    .filter(Boolean)
    .map((line) => {
      try {
        return JSON.parse(line);
      } catch {
        return null;
      }
    })
    .filter((entry) => entry?.event === 'start' && Number.isInteger(entry.pid))
    .map((entry) => entry.pid);
}

function pidAlive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return error?.code === 'EPERM';
  }
}

function resolvePython(explicit) {
  if (explicit) {
    const res = run(explicit, ['-c', 'import sys; print(sys.executable)'], { allowFailure: true, forceNoShell: true });
    if (res.status !== 0) fail(`--python ${explicit} is not a usable interpreter`, { stderr: res.stderr });
    return res.stdout.trim();
  }
  for (const candidate of [process.env.PYTHON, process.env.PYTHON3, 'python3', 'python'].filter(Boolean)) {
    const res = run(candidate, ['-c', 'import sys; print(sys.executable)'], { allowFailure: true, forceNoShell: true });
    if (res.status === 0 && res.stdout.trim()) return res.stdout.trim();
  }
  fail('no python3 interpreter found for the ACP fixture; pass --python <absolute path>');
  return null;
}

function agentHostToml(python, fixture, logPath, extraEnv = {}) {
  const lines = [
    '[[providers]]',
    'id = "mock-acp"',
    'protocol = "acp"',
    `command = "${python.replaceAll('\\', '/')}"`,
    `args = ["${fixture.replaceAll('\\', '/')}"]`,
    'enabled = true',
    '',
    '[providers.env]',
    `ACP_FIXTURE_LOG = "${logPath.replaceAll('\\', '/')}"`,
  ];
  for (const [key, value] of Object.entries(extraEnv)) lines.push(`${key} = "${value}"`);
  return `${lines.join('\n')}\n`;
}

// --- the generated consumer ------------------------------------------------

const CONSUMER_SOURCE = `import { copyFileSync, existsSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import * as native from '@42ch/nexus-native';

const argv = process.argv.slice(2);
const opts = new Map();
for (let i = 0; i < argv.length; i += 2) opts.set(argv[i], argv[i + 1]);
const home = opts.get('--home');
const out = opts.get('--out');
const mode = opts.get('--mode');
const label = opts.get('--label') || null;
const happyConfig = opts.get('--happy-config') || null;
const blockedConfig = opts.get('--blocked-config') || null;

function shadowedTools() {
  const names = ['cargo', 'rustc', 'rustup', 'cc', 'gcc', 'clang', 'cl', 'link', 'ld'];
  const dirs = (process.env.PATH || '').split(process.platform === 'win32' ? ';' : ':');
  const found = {};
  for (const name of names) {
    const candidates = process.platform === 'win32' ? [name + '.cmd', name + '.exe', name] : [name];
    for (const dir of dirs) {
      let done = false;
      for (const candidate of candidates) {
        if (existsSync(join(dir, candidate))) {
          found[name] = join(dir, candidate);
          done = true;
          break;
        }
      }
      if (done) break;
    }
  }
  return found;
}

function owner() {
  return { creator_id: 'proof-install', workspace_root: home, orchestration_run_id: null };
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function drain(core, operationId) {
  let sawDelta = false;
  let terminal = null;
  let events = 0;
  const batches = [];
  for (let i = 0; i < 40 && !terminal; i += 1) {
    const batch = await core.nextProviderEvents(operationId, 16, 256 * 1024);
    batches.push({ events: (batch.events || []).length, has_more: Boolean(batch.has_more) });
    for (const event of batch.events || []) {
      events += 1;
      if (event.MessageDelta) sawDelta = true;
      if (event.OpFinished) terminal = 'OpFinished';
      if (event.OpFailed) terminal = 'OpFailed';
    }
    if (!batch.has_more && terminal) break;
    await sleep(50);
  }
  return { sawDelta, terminal, events, batch_count: batches.length };
}

async function runStage(stage, configPath) {
  if (configPath) copyFileSync(configPath, join(home, '.nexus42', 'agent-host', 'config.toml'));
  const stageResult = { stage };
  const core = await native.openCore({ user_home: home, access: 'engine_owner', allow_uninitialized: false });
  const probe = await core.providerCall({
    request_id: stage + '-probe',
    method: 'probe',
    deadline_ms: 30000,
    payload: { provider_id: 'mock-acp', timeout_ms: 30000, cwd: home, owner: owner() },
  });
  stageResult.probe = probe;
  const launch = await core.providerCall({
    request_id: stage + '-launch',
    method: 'launch',
    deadline_ms: 30000,
    payload: { provider_id: 'mock-acp', cwd: home, mcp_servers: [], owner: owner() },
  });
  stageResult.launch = launch;
  const execute = await core.providerCall({
    request_id: stage + '-execute',
    method: 'execute',
    session_id: launch.session_id,
    deadline_ms: 30000,
    payload: { Prompt: { op_id: crypto.randomUUID(), content: [{ Text: { text: 'install-proof' } }], permission_scope: null } },
  });
  stageResult.execute = execute;
  if (stage === 'cancel') {
    const started = Date.now();
    const cancelReply = await core
      .providerCall({
        request_id: stage + '-cancel',
        method: 'cancel',
        session_id: launch.session_id,
        operation_id: execute.operation_id,
        deadline_ms: 30000,
        payload: {},
      })
      .catch((error) => ({ ok: false, error: String(error && error.message ? error.message : error) }));
    stageResult.cancel_ms = Date.now() - started;
    stageResult.cancel = cancelReply;
    // The accepted cancel outcome is exact, not a duration: a successful reply
    // that echoes the cancelled operation through the same session. A
    // rejection, an interrupted cleanup (ok:false), or a mismatched id is a
    // failed cancel even though a duration was measured.
    stageResult.cancel_accepted =
      cancelReply?.ok === true &&
      cancelReply.operation_id === execute.operation_id &&
      cancelReply.session_id === launch.session_id;
  }
  stageResult.stream = await drain(core, execute.operation_id);
  stageResult.shutdown = await core
    .providerCall({
      request_id: stage + '-shutdown',
      method: 'shutdown',
      session_id: launch.session_id,
      deadline_ms: 30000,
      payload: {},
    })
    .catch((error) => ({ ok: false, error: String(error && error.message ? error.message : error) }));
  stageResult.close = await core.close();
  return stageResult;
}

async function runPositive() {
  const result = { compat: native.nativeCompatibility(), shadowed_tools: shadowedTools(), stages: {} };
  const core = await native.openCore({ user_home: home, access: 'engine_owner', allow_uninitialized: false });
  const principal = await core.activePrincipal();
  result.principal = principal;
  const graph = await core.worldKbGraph(principal, 'wld_owned', false);
  result.graph_entities_before = (graph.entities || []).length;
  result.created = await core.patchWorldKbEntity(principal, 'wld_owned', {
    entity_id: 'kb_1f2e3d4c',
    expected_version: 0,
    patch: { title: 'Install Proof', block_type: 'character' },
  });
  result.updated = await core.patchWorldKbEntity(principal, 'wld_owned', {
    entity_id: 'kb_1f2e3d4c',
    expected_version: 1,
    patch: { title: 'Install Proof v2' },
  });
  const graphAfter = await core.worldKbGraph(principal, 'wld_owned', false);
  // The graph projection names the entity key_block_id; only the patch
  // request speaks entity_id.
  const entity = (graphAfter.entities || []).find((row) => row.key_block_id === 'kb_1f2e3d4c');
  result.graph_entity_version = entity ? entity.version : null;
  result.core_close = await core.close();
  result.stages.happy = await runStage('happy', happyConfig);
  result.stages.cancel = await runStage('cancel', blockedConfig);
  return result;
}

async function runNegative() {
  let error = null;
  try {
    await native.openCore({ user_home: home, access: 'engine_owner', allow_uninitialized: false });
  } catch (thrown) {
    error = String(thrown && thrown.message ? thrown.message : thrown);
  }
  return { label, rejected: Boolean(error), error, shadowed_tools: shadowedTools() };
}

async function main() {
  const result = mode === 'negative' ? await runNegative() : await runPositive();
  writeFileSync(out, JSON.stringify(result, null, 2) + '\\n');
  if (mode === 'negative' && !result.rejected) process.exit(1);
}

main().catch((error) => {
  try {
    writeFileSync(out, JSON.stringify({ fatal: String(error && error.stack ? error.stack : error) }, null, 2) + '\\n');
  } catch {
    // the driver reports the missing file
  }
  process.exit(1);
});
`;

// --- main ------------------------------------------------------------------

const args = parseArgs(process.argv.slice(2));
const spec = TARGETS[args.target];
const outDir = resolve(args.out);
const packDir = resolve(args.packDir ?? join(outDir, '..', 'native-packages', spec.suffix));
const evidencePath = join(outDir, 'install-proof.json');
const checks = [];
const record = (name, ok, detail) => checks.push({ name, ok: Boolean(ok), detail });

function archiveEvidenceFile(path, tag) {
  try {
    renameSync(path, `${path.replace(/\.json$/, '')}.${tag}-${Date.now()}.json`);
  } catch {
    // nothing left to archive (or it was already replaced) — the fresh write proceeds
  }
}

function writeEvidence(extra = {}) {
  const payload = {
    schema: 'rft-p3-t1-native-install-proof/v1',
    status: checks.every((check) => check.ok) && checks.length > 0 ? 'pass' : 'fail',
    script: SCRIPT,
    target: args.target,
    platform_package: `nexus-native-${spec.suffix}`,
    node_version_requested: args.nodeVersion,
    phase_reached: state.phase,
    source_sha: gitHeadSha(),
    utc_start: state.startedAt,
    utc_end: new Date().toISOString(),
    environment: {
      platform: process.platform,
      arch: process.arch,
      libc: process.platform === 'linux' ? detectLibc() : null,
      os_release: osRelease(),
      node: process.versions.node,
      napi: process.versions.napi ?? null,
      cpu: cpus()[0]?.model ?? null,
      cpu_count: cpus().length,
      total_memory_bytes: totalmem(),
      tmpdir: tmpdir(),
    },
    checks,
    ...state.evidence,
    ...extra,
  };
  mkdirSync(outDir, { recursive: true });
  try {
    const previous = JSON.parse(readFileSync(evidencePath, 'utf8'));
    if (previous.status !== 'pass') archiveEvidenceFile(evidencePath, 'pre');
  } catch (error) {
    if (error?.code !== 'ENOENT') archiveEvidenceFile(evidencePath, 'pre');
  }
  writeFileSync(evidencePath, `${JSON.stringify(payload, null, 2)}\n`);
  state.evidenceWritten = true;
  return payload;
}

if (process.platform !== spec.os || process.arch !== spec.cpu) {
  fail('target cannot be executed natively on this host', {
    target: args.target,
    host: { platform: process.platform, arch: process.arch },
    rule: 'no emulation-as-native claim',
  });
}
if (spec.libc === 'gnu' && detectLibc() !== 'glibc') {
  fail('target requires a glibc host', { host_libc: detectLibc() });
}
if (process.versions.node !== args.nodeVersion) {
  fail('this driver must run under the Node version it proves', {
    required: args.nodeVersion,
    actual: process.versions.node,
  });
}

tar(['--version']);

// 1. pack (or reuse) the local tarballs ------------------------------------
state.phase = 'package';
let receipt;
if (existsSync(join(packDir, 'package-receipt.json'))) {
  receipt = JSON.parse(readFileSync(join(packDir, 'package-receipt.json'), 'utf8'));
  if (receipt.status !== 'pass') fail('existing package receipt is not a pass', { packDir });
  if (receipt.target !== args.target) fail('existing package receipt is for another target', { packDir });
} else {
  const res = run(process.execPath, [join(__dirname, 'package.mjs'), '--target', args.target, '--out', packDir]);
  if (res.status !== 0) fail('packaging failed', { stderr: res.stderr.slice(-4000) });
  receipt = JSON.parse(readFileSync(join(packDir, 'package-receipt.json'), 'utf8'));
}
record('package_receipt_pass', receipt.status === 'pass', {
  tarballs: receipt.packages.map((entry) => entry.tarball_name ?? entry.tarball),
});

// 2. resolve the hermetic install set --------------------------------------
state.phase = 'install-set';
const installSet = buildInstallSet(packDir, receipt);
state.evidence = {
  package_receipt: {
    path: join(packDir, 'package-receipt.json'),
    platform_package: receipt.platform_package,
    loader_package: receipt.loader_package,
    package_version: receipt.package_version,
    artifact: receipt.artifact,
    compatibility: receipt.compatibility.manifest,
    codesign: receipt.codesign,
  },
  install_set: [...installSet.entries()].map(([name, tarball]) => ({
    name,
    tarball,
    tarball_name: tarball.split(/[\\/]/).pop(),
    sha256: sha256File(tarball),
  })),
};

// 3. empty temporary project -----------------------------------------------
state.phase = 'empty-project';
const projDir = mkdtempSync(join(tmpdir(), 'nexus-native-install-'));
const dependencies = {};
for (const [name, tarball] of installSet.entries()) {
  dependencies[name] = `file:${tarball}`;
}
writeFileSync(
  join(projDir, 'package.json'),
  `${JSON.stringify({ name: 'nexus-native-install-proof', version: '0.0.0', private: true, dependencies }, null, 2)}\n`,
);
writeFileSync(join(projDir, 'consumer.mjs'), CONSUMER_SOURCE);
const denyBin = writeDenyShims(projDir);
const denyLog = join(projDir, 'deny.log');
writeFileSync(denyLog, '');
const { env: denied, removed_path_entries } = deniedEnv(denyBin, denyLog);
state.evidence.install = {
  cwd: projDir,
  command: 'npm install --omit=optional --ignore-scripts --no-audit --no-fund --no-package-lock',
  compiler_denial: {
    deny_bin: denyBin,
    removed_path_entries,
    shadowed_tools: DENIED_TOOLS,
    enforced_cc_env: ['CC', 'CXX', 'AR', 'LD'],
  },
  project_entries_before: readdirSync(projDir).sort(),
};

// 4. seed the disposable home (outer fixture, not part of the denial scope) --
state.phase = 'seed-home';
const home = join(projDir, 'home');
// Canonical agent-host config expected by the merged native host:
// crates/nexus-agent-host/src/config.rs agent_host_config_path().
const agentHostConfigPath = join(home, '.nexus42', 'agent-host', 'config.toml');
mkdirSync(dirname(agentHostConfigPath), { recursive: true });
const fixture = resolve(args.fixture ?? join(ROOT, 'crates', 'nexus-agent-host', 'tests', 'fixtures', 'mock_acp_workflow.py'));
if (!existsSync(fixture)) fail('ACP fixture missing', { fixture });
const python = resolvePython(args.python);
const fixtureWorkspace = mkdtempSync(join(tmpdir(), 'nexus-install-fixture-'));
const fixtureLog = join(fixtureWorkspace, 'fixture.log');
const seedCommand = `cargo run -q --target ${args.target} -p nexus-core-node --bin native-wire-fixture-seed -- <home>`;
const seed = run('cargo', ['run', '-q', '--target', args.target, '-p', 'nexus-core-node', '--bin', 'native-wire-fixture-seed', '--', home], {
  cwd: ROOT,
  env: process.env,
});
state.evidence.fixture = {
  stage: 'outer driver fixture (cargo is available here on purpose; it is denied for install/run)',
  seed_command: seedCommand,
  seed_exit: seed.status,
  seed_stdout_tail: seed.stdout.slice(-1000),
  python,
  fixture,
  dataset: {
    identity: 'native_wire_fixture_seed',
    owner_creator: 'test_creator',
    workspace_slug: 'default',
    worlds: ['wld_owned', 'wld_foreign'],
    seeded_entities: ['kb_mod@0', 'kb_cas@2'],
    seeded_candidates: ['xj_job1', 'xj_job2'],
  },
};
if (seed.status !== 0) fail('fixture seed failed', { stderr: seed.stderr.slice(-4000) });

const happyConfigPath = join(projDir, 'agent-host.happy.toml');
const blockedConfigPath = join(projDir, 'agent-host.blocked.toml');
writeFileSync(happyConfigPath, agentHostToml(python, fixture, fixtureLog));
writeFileSync(blockedConfigPath, agentHostToml(python, fixture, fixtureLog, { BLOCK_PROMPT: '1' }));
copyFileSync(happyConfigPath, agentHostConfigPath);
// 5. install into the empty project with compilers denied -------------------
state.phase = 'install';
const installOut = join(projDir, 'install.log');
const install = run('npm', ['install', '--omit=optional', '--ignore-scripts', '--no-audit', '--no-fund', '--no-package-lock'], {
  cwd: projDir,
  env: denied,
  allowFailure: true,
});
writeFileSync(installOut, `${install.stdout}\n${install.stderr}\n`);
record('empty_project_install', install.status === 0, {
  exit: install.status,
  log: installOut,
  log_tail: `${install.stdout}\n${install.stderr}`.trim().split('\n').slice(-40),
  stderr_tail: install.stderr.slice(-2000),
});
if (install.status !== 0) {
  writeEvidence({ install_stdout_tail: install.stdout.slice(-4000) });
  process.exit(1);
}

const installedPlatformPkg = join(projDir, 'node_modules', '@42ch', `nexus-native-${spec.suffix}`);
const installedArtifact = join(installedPlatformPkg, 'native', 'nexus_core_node.node');
const installedCompatibility = join(installedPlatformPkg, 'native', 'compatibility.json');
record('installed_payload_paths', existsSync(installedArtifact) && existsSync(installedCompatibility), {
  artifact: installedArtifact,
  compatibility: installedCompatibility,
});
if (!existsSync(installedArtifact)) {
  writeEvidence();
  process.exit(1);
}
state.evidence.installed = {
  platform_package_dir: installedPlatformPkg,
  artifact_sha256: sha256File(installedArtifact),
  artifact_bytes: statSync(installedArtifact).size,
  compatibility_sha256: sha256File(installedCompatibility),
  compatibility_matches_staged: sha256File(installedCompatibility) === receipt.compatibility.sha256,
};
record(
  'installed_artifact_matches_packed_payload',
  sha256File(installedArtifact) === receipt.artifact.sha256,
  { installed: sha256File(installedArtifact), packed: receipt.artifact.sha256 },
);
const repoPlatformManifest = join(ROOT, 'packages', `nexus-native-${spec.suffix}`, 'package.json');
record(
  'installed_platform_manifest_is_frozen_metadata',
  canonicalJson(readFileSync(join(installedPlatformPkg, 'package.json'), 'utf8')) ===
    canonicalJson(readFileSync(repoPlatformManifest, 'utf8')),
  { installed: join(installedPlatformPkg, 'package.json'), frozen: repoPlatformManifest },
);

// 6. positive run: real graph/create/update/provider/cancel/close -----------
state.phase = 'positive-run';
const positiveOut = join(projDir, 'positive.json');
const positive = run(
  process.execPath,
  [join(projDir, 'consumer.mjs'), '--mode', 'positive', '--home', home, '--out', positiveOut, '--happy-config', happyConfigPath, '--blocked-config', blockedConfigPath],
  { cwd: projDir, env: denied, allowFailure: true, forceNoShell: true },
);
state.evidence.positive_run = {
  command: `${process.execPath} consumer.mjs --mode positive`,
  exit: positive.status,
  stdout_tail: positive.stdout.slice(-4000),
  stderr_tail: positive.stderr.slice(-4000),
};
let positiveResult = null;
if (existsSync(positiveOut)) {
  try {
    positiveResult = JSON.parse(readFileSync(positiveOut, 'utf8'));
  } catch (error) {
    positiveResult = { parse_error: String(error) };
  }
}
state.evidence.positive_result = positiveResult;
record('positive_run_exit_zero', positive.status === 0, { exit: positive.status });

const positiveUnusable = positiveResult === null || positiveResult.fatal !== undefined;
if (!positiveUnusable) {
  const compat = positiveResult.compat ?? {};
  record(
    'installed_manifest_fences_against_real_artifact',
    compat.target_triple === args.target &&
      compat.package_version === receipt.package_version &&
      compat.native_api_version === 1 &&
      compat.writer_protocol === 1 &&
      compat.napi_minimum === 8,
    compat,
  );
  record('graph_read_through_installed_payload', (positiveResult.graph_entities_before ?? 0) > 0, {
    entities: positiveResult.graph_entities_before,
  });
  record('create_through_installed_payload', positiveResult.created?.version === 1, positiveResult.created);
  record('update_through_installed_payload', positiveResult.updated?.version === 2, positiveResult.updated);
  record('graph_reflects_update', positiveResult.graph_entity_version === 2, {
    version: positiveResult.graph_entity_version,
  });
  const happy = positiveResult.stages?.happy ?? {};
  record('provider_probe_available', happy.probe?.health?.available === true, happy.probe);
  record('provider_launch_session', Boolean(happy.launch?.session_id), happy.launch);
  record('provider_operation_started', Boolean(happy.execute?.operation_id), happy.execute);
  record('provider_stream_delta_and_terminal', happy.stream?.sawDelta === true && Boolean(happy.stream?.terminal), happy.stream);
  record('provider_shutdown_ok', happy.shutdown?.ok === true, happy.shutdown);
  record(
    'happy_close_confirmed',
    happy.close?.cleanup_confirmed === true && happy.close?.state === 'closed',
    happy.close,
  );
  const cancel = positiveResult.stages?.cancel ?? {};
  // The accepted cancel outcome is required, not merely observed: `ok:true`
  // echoing the cancelled operation through the same session, plus a measured
  // duration. The reply and any error are retained so a non-ok or interrupted
  // cancel is visible in the evidence instead of passing as a timing sample.
  // Whether the cancelled operation then declares its terminal inside the
  // 2 s LIFE-2 budget remains P2's criterion; here it is recorded.
  record(
    'provider_cancel_accepted',
    cancel.cancel_accepted === true && typeof cancel.cancel_ms === 'number',
    {
      cancel_accepted: cancel.cancel_accepted ?? false,
      cancel_ms: cancel.cancel_ms ?? null,
      cancel_reply: cancel.cancel ?? null,
      cancel_error: cancel.cancel?.error ?? null,
      terminal_observed: cancel.stream?.terminal ?? null,
    },
  );
  record(
    'cancel_close_confirmed',
    cancel.close?.cleanup_confirmed === true && cancel.close?.state === 'closed',
    cancel.close,
  );
  record(
    'compilers_shadowed_in_child',
    Object.values(positiveResult.shadowed_tools ?? {}).some((value) => String(value).includes('deny-bin')),
    positiveResult.shadowed_tools,
  );
} else {
  record('positive_result_json', false, positiveResult);
}

// 7. negative cases: reject before open, with no DB/effect ------------------
state.phase = 'negative-cases';
const otherTriple = Object.keys(TARGETS).find((triple) => triple !== args.target);
const mutations = [
  {
    name: 'platform_os_mismatch',
    expect: /os mismatch/,
    apply: () => editJson(join(installedPlatformPkg, 'package.json'), (manifest) => {
      manifest.os = [spec.os === 'linux' ? 'win32' : 'linux'];
    }),
  },
  {
    name: 'platform_cpu_mismatch',
    expect: /cpu mismatch/,
    apply: () => editJson(join(installedPlatformPkg, 'package.json'), (manifest) => {
      manifest.cpu = [spec.cpu === 'x64' ? 'arm64' : 'x64'];
    }),
  },
  ...(spec.os === 'linux'
    ? [
        {
          name: 'platform_libc_missing',
          expect: /missing "libc"/,
          apply: () => editJson(join(installedPlatformPkg, 'package.json'), (manifest) => {
            delete manifest.libc;
          }),
        },
      ]
    : []),
  {
    name: 'target_triple_mismatch',
    expect: /target_triple mismatch/,
    apply: () => editJson(installedCompatibility, (manifest) => {
      manifest.target_triple = otherTriple;
    }),
  },
  {
    name: 'package_version_mismatch',
    expect: /package_version mismatch/,
    apply: () => editJson(installedCompatibility, (manifest) => {
      manifest.package_version = '9.9.9';
    }),
  },
  {
    name: 'contract_hash_mismatch',
    expect: /contract_tree_sha256 mismatch/,
    apply: () => editJson(installedCompatibility, (manifest) => {
      manifest.contract_tree_sha256 = 'b'.repeat(64);
    }),
  },
  {
    name: 'placeholder_hash',
    expect: /contract_tree_sha256 mismatch|placeholder rejected/,
    apply: () => editJson(installedCompatibility, (manifest) => {
      manifest.contract_tree_sha256 = '0'.repeat(64);
    }),
  },
  {
    name: 'napi_minimum_downgrade',
    expect: /napi_minimum mismatch/,
    apply: () => editJson(installedCompatibility, (manifest) => {
      manifest.napi_minimum = 7;
    }),
  },
  {
    name: 'db_schema_range_drift',
    expect: /db_schema_min mismatch/,
    apply: () => editJson(installedCompatibility, (manifest) => {
      manifest.db_schema_min = 999;
    }),
  },
  {
    name: 'compatibility_manifest_missing',
    expect: /compatibility manifest/,
    apply: () => withBackup(installedCompatibility, (path) => rmSync(path)),
  },
  {
    name: 'artifact_missing',
    expect: /artifact missing/,
    apply: () => withBackup(installedArtifact, (path) => rmSync(path)),
  },
  {
    name: 'artifact_corrupt',
    expect: /./,
    apply: () => withBackup(installedArtifact, (path) => writeFileSync(path, Buffer.alloc(4096, 0xff))),
  },
];

function editJson(path, edit) {
  const original = readFileSync(path);
  const manifest = JSON.parse(original.toString('utf8'));
  edit(manifest);
  writeFileSync(path, `${JSON.stringify(manifest, null, 2)}\n`);
  return () => writeFileSync(path, original);
}

function withBackup(path, mutate) {
  const backup = `${path}.proof-backup`;
  copyFileSync(path, backup);
  mutate(path);
  return () => {
    rmSync(path, { force: true });
    renameSync(backup, path);
  };
}

const negativeResults = [];
for (const mutation of mutations) {
  const restore = mutation.apply();
  const before = homeDigest(home);
  const caseOut = join(projDir, `negative-${mutation.name}.json`);
  const res = run(
    process.execPath,
    [join(projDir, 'consumer.mjs'), '--mode', 'negative', '--label', mutation.name, '--home', home, '--out', caseOut],
    { cwd: projDir, env: denied, allowFailure: true, forceNoShell: true },
  );
  const after = homeDigest(home);
  restore();
  let payload = null;
  if (existsSync(caseOut)) {
    try {
      payload = JSON.parse(readFileSync(caseOut, 'utf8'));
    } catch (error) {
      payload = { parse_error: String(error) };
    }
  }
  const error = payload?.error ?? null;
  // The negative consumer exits 0 when it observed a rejection and 1 when the
  // open unexpectedly succeeded; either way the driver judges on the recorded
  // observation, not on the exit code alone.
  const rejected = payload?.rejected === true && Boolean(error) && mutation.expect.test(error);
  const noEffect = before.digest === after.digest;
  negativeResults.push({
    name: mutation.name,
    expected_pattern: String(mutation.expect),
    consumer_exit: res.status,
    observed_error: error,
    rejected_before_open: rejected,
    home_unchanged: noEffect,
    home_digest_before: before.digest,
    home_digest_after: after.digest,
    files_before: before.entries.length,
  });
  record(`negative:${mutation.name}`, rejected && noEffect, {
    expected: String(mutation.expect),
    observed: error,
    consumer_exit: res.status,
    home_unchanged: noEffect,
  });
}
state.evidence.negatives = negativeResults;

// 8. no compiler was ever attempted, and no fixture child survived ----------
state.phase = 'denial-and-reaping';
const denyLines = readFileSync(denyLog, 'utf8').split('\n').filter(Boolean);
record('no_compiler_invocation_attempted', denyLines.length === 0, { deny_log: denyLines });
const startPids = fixtureStartPids(fixtureLog);
const survivors = startPids.filter(pidAlive);
state.evidence.fixture_processes = {
  log: fixtureLog,
  started_pids: startPids,
  surviving_pids: survivors,
  reaped: survivors.length === 0,
};
record('fixture_children_reaped', startPids.length > 0 && survivors.length === 0, {
  started: startPids.length,
  survivors,
});
state.evidence.home_files = homeDigest(home).entries.length;
state.evidence.deny_log_path = denyLog;
state.evidence.project_dir = projDir;

const evidence = writeEvidence();
process.stdout.write(`${SCRIPT}: ${evidence.status} -> ${evidencePath}\n`);
process.exit(evidence.status === 'pass' ? 0 : 1);
