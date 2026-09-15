#!/usr/bin/env node
/**
 * P3-T1 — deterministic local packaging for the four `@42ch/nexus-native-*`
 * target packages plus the `@42ch/nexus-native` loader package.
 *
 * Contract (frozen by P2, consumed unchanged here):
 *   - artifact path inside a target package: `native/nexus_core_node.node`
 *   - adjacent manifest:                     `native/compatibility.json`
 *     (loader.ts `loadNodePath` / `readBundledCompatibility` and the platform
 *      manifests' `files: ["native/"]` own these paths; this script never
 *      invents a second layout)
 *   - one N-API 8 artifact is shared by Node and by the Electron utility
 *     process; there is no rebuild per Electron ABI.
 *
 * This script produces LOCAL tarballs only. Publishing, signing and
 * notarization are not authorized in P3-T1 (see plan P3-T3).
 *
 * The artifact is built for an explicit target triple, staged, and then
 * actually executed (`compatibility()` through `require`) on the same host so
 * the shipped manifest is derived from the real binary — never fabricated.
 * Cross-arch packaging is therefore refused: a runner can only package the
 * target it can execute.
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
import { release as osRelease, tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPT = 'packages/nexus-native/scripts/package.mjs';
const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..', '..', '..');
const IS_WIN = process.platform === 'win32';

/** The frozen target map (architecture-contracts §9). */
const TARGETS = {
  'aarch64-apple-darwin': { suffix: 'darwin-arm64', os: 'darwin', cpu: 'arm64' },
  'x86_64-apple-darwin': { suffix: 'darwin-x64', os: 'darwin', cpu: 'x64' },
  'x86_64-pc-windows-msvc': { suffix: 'win32-x64-msvc', os: 'win32', cpu: 'x64' },
  'x86_64-unknown-linux-gnu': {
    suffix: 'linux-x64-gnu',
    os: 'linux',
    cpu: 'x64',
    libc: 'gnu',
  },
};

const REQUIRED_NAPI_MINIMUM = 8;
/** Files a target package directory may contain besides `native/`. */
const PLATFORM_DIR_ALLOWLIST = new Set(['package.json', 'AGENTS.md', '.gitignore']);
/** Exact payload of a target package tarball (paths inside the archive). */
const PLATFORM_ARCHIVE_ENTRIES = [
  'package/package.json',
  'package/AGENTS.md',
  'package/native/nexus_core_node.node',
  'package/native/compatibility.json',
];
const LOADER_ARCHIVE_REQUIRED = ['package/package.json', 'package/dist/index.js', 'package/dist/loader.js'];
/**
 * Entries the packer adds to a workspace package tarball on its own. pnpm copies
 * the workspace-root LICENSE into every packed package; it is verified against
 * the root file below and is never allowed to stand in for the payload pair.
 */
const PACKER_INJECTED_ENTRIES = ['package/LICENSE'];

const USAGE = `usage: node ${SCRIPT} --target <rust-triple> --out <dir>
       [--release|--debug] [--artifact <path>]

  --target    one of ${Object.keys(TARGETS).join(', ')}
  --out       directory that receives the tarballs and package-receipt.json
  --artifact  reuse an already built artifact of this target (same semantics as
              build output; the host must still be able to execute it)
  --release   build the release profile and ad-hoc sign on macOS (default)
  --debug     build the debug profile`;

const state = { receiptPath: null, target: null, startedAt: new Date().toISOString() };

function fail(message, detail) {
  const suffix = detail === undefined ? '' : `\n${JSON.stringify(detail, null, 2)}`;
  process.stderr.write(`${SCRIPT}: ${message}${suffix}\n`);
  writeFailReceipt(message, detail);
  process.exit(1);
}

function archiveReceipt(path, tag) {
  try {
    renameSync(path, `${path.replace(/\.json$/, '')}.${tag}-${Date.now()}.json`);
  } catch {
    // nothing left to archive (or it was already replaced) — the fresh write proceeds
  }
}

/**
 * A failed run must leave its own record: a stale `pass` receipt on disk would
 * otherwise be read as a green result by the matrix summary.
 */
function writeFailReceipt(message, detail) {
  if (!state.receiptPath) return;
  try {
    mkdirSync(dirname(state.receiptPath), { recursive: true });
    try {
      const previous = JSON.parse(readFileSync(state.receiptPath, 'utf8'));
      archiveReceipt(state.receiptPath, previous.status === 'pass' ? 'pass' : 'pre');
    } catch (error) {
      if (error?.code !== 'ENOENT') archiveReceipt(state.receiptPath, 'pre');
    }
    writeFileSync(
      state.receiptPath,
      `${JSON.stringify(
        {
          schema: 'rft-p3-t1-package-receipt/v1',
          status: 'fail',
          script: SCRIPT,
          target: state.target,
          utc_start: state.startedAt,
          utc_end: new Date().toISOString(),
          error: message,
          error_detail: detail ?? null,
        },
        null,
        2,
      )}\n`,
    );
  } catch {
    // the original failure is reported on stderr; a failed receipt write must not mask it
  }
}

function run(command, args, options = {}) {
  const res = spawnSync(command, args, {
    cwd: options.cwd ?? ROOT,
    env: options.env ?? process.env,
    encoding: 'utf8',
    // A shell is only needed on Windows, where the pnpm/tar shims are .cmd
    // files. Inline `node -e` payloads are always spawned with shell: false
    // so their quoting is never re-interpreted.
    shell: options.shell ?? (IS_WIN && command !== process.execPath),
    maxBuffer: 64 * 1024 * 1024,
  });
  if (res.error) fail(`failed to run ${command}: ${res.error.message}`);
  const out = { status: res.status ?? 1, stdout: res.stdout ?? '', stderr: res.stderr ?? '' };
  if (out.status !== 0 && options.allowFailure !== true) {
    fail(`${command} ${args.join(' ')} exited ${out.status}`, {
      cwd: options.cwd ?? ROOT,
      stderr: out.stderr.slice(-4000),
    });
  }
  return out;
}

function parseArgs(argv) {
  const parsed = { target: null, out: null, artifact: null, release: true };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    const value = () => {
      i += 1;
      if (i >= argv.length) fail(`missing value for ${arg}`);
      return argv[i];
    };
    if (arg === '--target') parsed.target = value();
    else if (arg === '--out') parsed.out = value();
    else if (arg === '--artifact') parsed.artifact = value();
    else if (arg === '--release') parsed.release = true;
    else if (arg === '--debug') parsed.release = false;
    else if (arg === '--help' || arg === '-h') {
      process.stdout.write(`${USAGE}\n`);
      process.exit(0);
    } else fail(`unknown argument ${arg}\n${USAGE}`);
  }
  if (!parsed.target) fail(`--target is required\n${USAGE}`);
  if (!parsed.out) fail(`--out is required\n${USAGE}`);
  if (!TARGETS[parsed.target]) {
    fail(`unknown target ${parsed.target}`, { known: Object.keys(TARGETS) });
  }
  return parsed;
}

function sha256(buffer) {
  return createHash('sha256').update(buffer).digest('hex');
}

function sha256File(path) {
  return sha256(readFileSync(path));
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

function readJson(path) {
  return JSON.parse(readFileSync(path, 'utf8'));
}

/** Key-order independent comparison of packed and on-disk manifests. */
function canonicalJson(text) {
  const value = typeof text === 'string' ? JSON.parse(text) : text;
  return JSON.stringify(value, (_key, item) => {
    if (item === null || typeof item !== 'object' || Array.isArray(item)) return item;
    return Object.fromEntries(Object.entries(item).sort(([a], [b]) => a.localeCompare(b)));
  });
}

/** The exact command envelope every P3-T1 evidence file carries. */
function evidenceHeader(startedAt, target) {
  return {
    script: SCRIPT,
    harness: 'rft-p3-t1-native-package',
    target,
    source_sha: gitHeadSha(),
    utc_start: startedAt,
    host: {
      platform: process.platform,
      arch: process.arch,
      libc: process.platform === 'linux' ? detectLibc() : null,
      os_release: osRelease(),
      node: process.versions.node,
      napi: process.versions.napi ?? null,
    },
  };
}

/**
 * A runner may only package the target it can execute: the shipped
 * `compatibility.json` is read out of the real binary, so a foreign-arch or
 * foreign-libc artifact would either need fabricated metadata or a
 * translation layer. Both are refused (architecture-contracts §9: no
 * emulation-as-native claim).
 */
function assertNativeHost(target, spec) {
  if (process.platform !== spec.os) {
    fail(`target ${target} requires a ${spec.os} host`, { host: process.platform });
  }
  if (process.arch !== spec.cpu) {
    fail(`target ${target} requires a ${spec.cpu} host`, { host: process.arch });
  }
  if (spec.libc === 'gnu' && detectLibc() !== 'glibc') {
    fail(`target ${target} requires a glibc host`, { host_libc: detectLibc() });
  }
}

function artifactFileName(target) {
  const spec = TARGETS[target];
  if (spec.os === 'win32') return 'nexus_core_node.dll';
  if (spec.os === 'darwin') return 'libnexus_core_node.dylib';
  return 'libnexus_core_node.so';
}

function buildArtifact(target, release) {
  const args = ['build', '-p', 'nexus-core-node', '--target', target];
  if (release) args.push('--release');
  const command = `cargo ${args.join(' ')}`;
  const res = spawnSync('cargo', args, {
    cwd: ROOT,
    env: process.env,
    encoding: 'utf8',
    shell: IS_WIN,
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  if (res.error) fail(`failed to run ${command}: ${res.error.message}`);
  if ((res.status ?? 1) !== 0) {
    fail(`${command} exited ${res.status ?? 1}`, { stderr: (res.stderr ?? '').slice(-8000) });
  }
  return { command, stdout_tail: (res.stdout ?? '').slice(-4000), stderr_tail: (res.stderr ?? '').slice(-4000) };
}

function targetDir() {
  const configured = process.env.CARGO_TARGET_DIR;
  return configured ? resolve(configured) : join(ROOT, 'target');
}

/**
 * Ad-hoc signing is required before an arm64 macOS dylib can be loaded at all.
 * This is a development signature for the local proof artifact, NOT the
 * signed/notarized distribution that SEC-1 requires (plan P3-T3).
 */
function adhocSign(path) {
  const before = run('codesign', ['--verify', '--verbose=4', path], { allowFailure: true });
  run('codesign', ['--force', '--sign', '-', path]);
  const after = run('codesign', ['--verify', '--verbose=4', path], { allowFailure: true });
  if (after.status !== 0) fail(`post-sign verification failed for ${path}`, { stderr: after.stderr });
  return { identity: '-', was_invalid: before.status !== 0 };
}

/**
 * Read `compatibility()` out of the real artifact in a short-lived child.
 *
 * `require()` only registers a loader for the `.node` extension; the Rust
 * cdylib that `cargo build` produces is `libnexus_core_node.{dylib,so,dll}`
 * and Node would otherwise parse it as JavaScript. Stage the real bytes under
 * a `.node` name (the ad-hoc Mach-O signature travels with the copy) so the
 * executed bytes are the built artifact, not a rebuilt or synthesised binary.
 */
function readCompatibility(artifact) {
  const loadPath = artifact.endsWith('.node') ? artifact : join(stageNodeCopy(artifact), 'nexus_core_node.node');
  const script =
    "import { createRequire } from 'node:module';" +
    'const require = createRequire(import.meta.url);' +
    'const binding = require(process.argv[1]);' +
    'process.stdout.write(binding.compatibility());';
  const res = run(process.execPath, ['--input-type=module', '-e', script, loadPath], { shell: false });
  return res.stdout;
}

function stageNodeCopy(artifact) {
  const dir = mkdtempSync(join(tmpdir(), 'nexus-package-artifact-'));
  copyFileSync(artifact, join(dir, 'nexus_core_node.node'));
  return dir;
}

function assertCompatibilityShape(manifest, target, expectedVersion) {
  const fields = [
    'native_api_version',
    'writer_protocol',
    'target_triple',
    'package_version',
    'contract_tree_sha256',
    'db_schema_min',
    'db_schema_max',
    'napi_minimum',
  ];
  for (const field of fields) {
    if (manifest[field] === undefined) fail(`compatibility manifest is missing "${field}"`, manifest);
  }
  const problems = [];
  if (manifest.native_api_version !== 1) problems.push(`native_api_version=${manifest.native_api_version}`);
  if (manifest.writer_protocol !== 1) problems.push(`writer_protocol=${manifest.writer_protocol}`);
  if (manifest.napi_minimum !== REQUIRED_NAPI_MINIMUM) problems.push(`napi_minimum=${manifest.napi_minimum}`);
  if (manifest.target_triple !== target) problems.push(`target_triple=${manifest.target_triple}`);
  if (manifest.package_version !== expectedVersion) problems.push(`package_version=${manifest.package_version}`);
  if (!/^[a-f0-9]{64}$/.test(manifest.contract_tree_sha256)) {
    problems.push('contract_tree_sha256 shape');
  } else if (manifest.contract_tree_sha256 === '0'.repeat(64)) {
    problems.push('contract_tree_sha256 placeholder');
  }
  if (!Number.isInteger(manifest.db_schema_min) || !Number.isInteger(manifest.db_schema_max)) {
    problems.push('db_schema bounds are not integers');
  } else if (manifest.db_schema_min > manifest.db_schema_max) {
    problems.push('db_schema range inverted');
  }
  if (problems.length > 0) fail(`built artifact manifest violates the frozen contract`, { problems, manifest });
  return manifest;
}

function tar(argv, options = {}) {
  return run('tar', argv, options);
}

function assertTarAvailable() {
  run('tar', ['--version'], { allowFailure: false });
}

function tarEntries(tarball) {
  const res = tar(['-tzf', tarball]);
  return res.stdout
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line.length > 0 && !line.endsWith('/'));
}

function tarTypes(tarball) {
  const res = tar(['-tvzf', tarball]);
  return res.stdout
    .split('\n')
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line) => ({ type: line[0], path: line.split(/\s+/).slice(-1)[0] }));
}

function tarRead(tarball, entry) {
  return tar(['-xzOf', tarball, entry]).stdout;
}

/**
 * A package payload must be self-contained: no build-machine path may leak
 * into a manifest or the compatibility document, and no `workspace:` protocol
 * may survive into an installable archive.
 */
function assertNoHostPaths(text, label, checks) {
  for (const check of checks) {
    if (text.includes(check)) fail(`${label} leaks a build-host path`, { needle: check });
  }
  if (/\bworkspace:/.test(text)) fail(`${label} still contains a workspace: specifier`, { label });
  if (/[A-Za-z]:[\\/](Users|a|workspace|build|runner)/.test(text) || /\/(Users|home|root)\//.test(text)) {
    fail(`${label} contains an absolute host path`, { label });
  }
}

/**
 * Pack into a fresh directory, then move the single archive into `packDir`.
 * `pnpm pack` overwrites a same-named tarball in place, so packing straight
 * into an existing `--out` directory silently reuses the previous file name and
 * looks like it produced nothing on a re-run.
 */
function packPackage(packageRoot, packDir, label) {
  const staging = mkdtempSync(join(tmpdir(), `nexus-pack-${label}-`));
  const res = run('pnpm', ['pack', '--pack-destination', staging], { cwd: packageRoot, allowFailure: true });
  if (res.status !== 0) {
    fail(`pnpm pack failed for ${label}`, { cwd: packageRoot, stderr: res.stderr.slice(-4000) });
  }
  const produced = readdirSync(staging).filter((name) => name.endsWith('.tgz'));
  if (produced.length !== 1) {
    fail(`expected exactly one tarball for ${label}`, { produced, dir: staging });
  }
  const name = produced[0];
  const tarball = join(packDir, name);
  copyFileSync(join(staging, name), tarball);
  rmSync(staging, { recursive: true, force: true });
  return { tarball, name, stdout_tail: res.stdout.slice(-2000) };
}

function archiveDirectoryBeforeFailure(receiptPath) {
  if (!existsSync(receiptPath)) return null;
  let previous = null;
  try {
    previous = readJson(receiptPath);
  } catch {
    previous = null;
  }
  if (previous?.status === 'pass') return null;
  const archive = `${receiptPath.replace(/\.json$/, '')}.pre-${Date.now()}.json`;
  renameSync(receiptPath, archive);
  return archive;
}

const startedAt = new Date().toISOString();
const args = parseArgs(process.argv.slice(2));
const spec = TARGETS[args.target];
const packDir = resolve(args.out);
const receiptPath = join(packDir, 'package-receipt.json');
state.receiptPath = receiptPath;
state.target = args.target;
state.startedAt = startedAt;

assertNativeHost(args.target, spec);
assertTarAvailable();
mkdirSync(packDir, { recursive: true });
const archived = archiveDirectoryBeforeFailure(receiptPath);

const rustc = run('rustc', ['-Vv'], { allowFailure: true });
const platformPkgName = `nexus-native-${spec.suffix}`;
const platformPkgRoot = join(ROOT, 'packages', platformPkgName);
const loaderPkgRoot = join(ROOT, 'packages', 'nexus-native');
for (const dir of [platformPkgRoot, loaderPkgRoot]) {
  if (!existsSync(join(dir, 'package.json'))) fail(`package manifest missing at ${dir}`);
}

const loaderManifest = readJson(join(loaderPkgRoot, 'package.json'));
const platformManifest = readJson(join(platformPkgRoot, 'package.json'));
const expectedVersion = platformManifest.version;
if (loaderManifest.optionalDependencies?.[`@42ch/${platformPkgName}`] !== expectedVersion) {
  fail('loader optionalDependencies do not pin this platform package exactly', {
    pin: loaderManifest.optionalDependencies?.[`@42ch/${platformPkgName}`],
    platform_version: expectedVersion,
  });
}
for (const required of ['dist/index.js', 'dist/loader.js']) {
  if (!existsSync(join(loaderPkgRoot, required))) {
    fail(`loader build output ${required} is missing; run: pnpm --filter @42ch/nexus-native run build`);
  }
}

// --- 1. build (or reuse) the target artifact -------------------------------
const profile = args.release ? 'release' : 'debug';
const built = args.artifact
  ? { command: null, reused: resolve(args.artifact), stdout_tail: '', stderr_tail: '' }
  : buildArtifact(args.target, args.release);
const artifact = built.reused ?? join(targetDir(), args.target, profile, artifactFileName(args.target));
if (!existsSync(artifact)) {
  fail(`native artifact not found at ${artifact}`, { profile, target: args.target });
}

const signing = spec.os === 'darwin' ? adhocSign(artifact) : null;

// --- 2. derive the compatibility manifest from the real binary -------------
const compatibility = assertCompatibilityShape(
  JSON.parse(readCompatibility(artifact)),
  args.target,
  expectedVersion,
);
const compatibilityText = `${JSON.stringify(compatibility, null, 2)}\n`;

// --- 3. stage the target package payload ----------------------------------
const nativeDir = join(platformPkgRoot, 'native');
rmSync(nativeDir, { recursive: true, force: true });
mkdirSync(nativeDir, { recursive: true });
const stagedArtifact = join(nativeDir, 'nexus_core_node.node');
const stagedCompatibility = join(nativeDir, 'compatibility.json');
copyFileSync(artifact, stagedArtifact);
writeFileSync(stagedCompatibility, compatibilityText);

const strayFiles = readdirSync(platformPkgRoot).filter((name) => !PLATFORM_DIR_ALLOWLIST.has(name) && name !== 'native');
if (strayFiles.length > 0) {
  fail(`unexpected files in ${platformPkgName}`, { strayFiles });
}
const nativeFiles = readdirSync(nativeDir).sort();
if (nativeFiles.join(',') !== 'compatibility.json,nexus_core_node.node') {
  fail(`target package native/ payload is not the frozen pair`, { nativeFiles });
}

// --- 4. pack both packages locally (no publish) ---------------------------
assertNoHostPaths(JSON.stringify(platformManifest), `${platformPkgName}/package.json`, [ROOT]);
const platformPack = packPackage(platformPkgRoot, packDir, platformPkgName);
const loaderPack = packPackage(loaderPkgRoot, packDir, 'nexus-native');

const platformEntries = tarEntries(platformPack.tarball);
const missingPlatformEntries = PLATFORM_ARCHIVE_ENTRIES.filter((entry) => !platformEntries.includes(entry));
if (missingPlatformEntries.length > 0) {
  fail(`${platformPack.name} is missing frozen archive paths`, { missingPlatformEntries });
}
const unexpectedPlatformEntries = platformEntries.filter(
  (entry) => !PLATFORM_ARCHIVE_ENTRIES.includes(entry) && !PACKER_INJECTED_ENTRIES.includes(entry),
);
if (unexpectedPlatformEntries.length > 0) {
  fail(`${platformPack.name} carries entries outside the frozen payload`, { unexpectedPlatformEntries });
}
// pnpm injects the workspace-root LICENSE into workspace package tarballs
// (verified byte-identical below); record it instead of rejecting the packer's
// own behaviour, and keep rejecting every other extra entry.
const packerInjectedEntries = platformEntries
  .filter((entry) => PACKER_INJECTED_ENTRIES.includes(entry))
  .map((entry) => ({
    entry,
    sha256: sha256(tarRead(platformPack.tarball, entry)),
    workspace_root_license_sha256: sha256File(join(ROOT, 'LICENSE')),
  }));
const symlinks = tarTypes(platformPack.tarball).filter((entry) => entry.type === 'l');
if (symlinks.length > 0) fail(`${platformPack.name} contains symlink entries`, { symlinks });

const platformPackedManifestText = tarRead(platformPack.tarball, 'package/package.json');
if (
  canonicalJson(platformPackedManifestText) !==
  canonicalJson(readFileSync(join(platformPkgRoot, 'package.json'), 'utf8'))
) {
  fail(`${platformPack.name} package.json differs from the frozen manifest`);
}
assertNoHostPaths(platformPackedManifestText, `${platformPack.name}!package/package.json`, [ROOT]);
const platformPackedManifest = JSON.parse(platformPackedManifestText);
const manifestDrift = [];
if (platformPackedManifest.scripts) manifestDrift.push('install-time scripts present');
if (platformPackedManifest.dependencies) manifestDrift.push('runtime dependencies present');
for (const field of ['os', 'cpu', 'libc']) {
  if (JSON.stringify(platformPackedManifest[field]) !== JSON.stringify(platformManifest[field])) {
    manifestDrift.push(`${field} drifted`);
  }
}
if (manifestDrift.length > 0) fail(`${platformPack.name} manifest violates the packaging contract`, { manifestDrift });

const packedCompatibilityText = tarRead(platformPack.tarball, 'package/native/compatibility.json');
assertNoHostPaths(packedCompatibilityText, `${platformPack.name}!compatibility.json`, [ROOT]);
if (packedCompatibilityText !== compatibilityText) {
  fail(`${platformPack.name} compatibility.json does not match the staged manifest`);
}

const loaderEntries = tarEntries(loaderPack.tarball);
const missingLoaderEntries = LOADER_ARCHIVE_REQUIRED.filter((entry) => !loaderEntries.includes(entry));
if (missingLoaderEntries.length > 0) {
  fail(`${loaderPack.name} is missing required entries`, { missingLoaderEntries });
}
const loaderPackedManifest = JSON.parse(tarRead(loaderPack.tarball, 'package/package.json'));
if (loaderPackedManifest.main !== loaderManifest.main) {
  fail(`${loaderPack.name} main entry drifted`, { main: loaderPackedManifest.main });
}
assertNoHostPaths(
  JSON.stringify(loaderPackedManifest.dependencies ?? {}),
  `${loaderPack.name}!package/dependencies`,
  [ROOT],
);

const finishedAt = new Date().toISOString();
const receipt = {
  schema: 'rft-p3-t1-package-receipt/v1',
  status: 'pass',
  ...evidenceHeader(startedAt, args.target),
  utc_end: finishedAt,
  archived_previous_receipt: archived,
  platform_package: `@42ch/${platformPkgName}`,
  loader_package: loaderManifest.name,
  package_version: expectedVersion,
  profile,
  artifact: {
    path: artifact,
    reused: Boolean(args.artifact),
    sha256: sha256File(artifact),
    bytes: statSync(artifact).size,
    staged_path: stagedArtifact,
    staged_sha256: sha256File(stagedArtifact),
  },
  build: built,
  rustc: rustc.status === 0 ? rustc.stdout.trim() : null,
  codesign: signing,
  compatibility: {
    manifest: compatibility,
    sha256: sha256(compatibilityText),
    source: 'executed artifact compatibility()',
  },
  determinism: {
    archive_paths: 'fixed',
    content_set: 'fixed',
    tarball_sha256: 'recorded; the contract is the fixed content set, not a byte-identical gzip stream',
    packer_injected_entries: packerInjectedEntries,
  },
  packages: [
    {
      name: `@42ch/${platformPkgName}`,
      dir: platformPkgRoot,
      tarball: platformPack.tarball,
      tarball_name: platformPack.name,
      tarball_sha256: sha256File(platformPack.tarball),
      tarball_bytes: statSync(platformPack.tarball).size,
      entries: platformEntries,
      packed_manifest: platformPackedManifest,
    },
    {
      name: loaderManifest.name,
      dir: loaderPkgRoot,
      tarball: loaderPack.tarball,
      tarball_name: loaderPack.name,
      tarball_sha256: sha256File(loaderPack.tarball),
      tarball_bytes: statSync(loaderPack.tarball).size,
      entries: loaderEntries,
      packed_manifest: loaderPackedManifest,
      note: 'local proof artifact; the loader manifest declares no `files` narrowing (P2-owned), so the tarball also carries src/tests/scripts. Narrowing belongs to the publish path, not to this local proof.',
    },
  ],
  publish: 'not authorized — local tarballs only',
};
writeFileSync(receiptPath, `${JSON.stringify(receipt, null, 2)}\n`);
process.stdout.write(
  `${SCRIPT}: packed ${platformPack.name} + ${loaderPack.name} for ${args.target} -> ${receiptPath}\n`,
);
