#!/usr/bin/env node
/**
 * Nexus unsigned Electron package driver.
 *
 * This entry point has one lane: native-architecture, unsigned macOS packaging.
 * It never discovers credentials, invokes signing tools, or fetches dependencies.
 */
import { execFileSync, spawnSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, renameSync, rmSync, statSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { packager } from '@electron/packager';
import {
  PACKAGE_CONTRACT,
  assertNativeCompatibility,
  assertNoSigningEnvironment,
  assertNoSymlinkEscape,
  assertReceipt,
  assertRequiredFiles,
  digestTree,
  parsePackageArgs,
  sha256File,
} from './package-contract.mjs';

const appRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const repoRoot = resolve(appRoot, '..', '..');
const rootPackage = JSON.parse(readFileSync(join(repoRoot, 'package.json'), 'utf8'));
const product = JSON.parse(readFileSync(join(appRoot, 'resources', 'product.json'), 'utf8'));
const version = rootPackage.version;
const webDist = join(repoRoot, 'apps', 'web', 'dist');
const serviceRoot = join(repoRoot, 'apps', 'nexus-service');
const nativeLoaderRoot = join(repoRoot, 'packages', 'nexus-native');
const lockfile = join(repoRoot, 'pnpm-lock.yaml');
const nativePlatformRoot = (arch) => join(repoRoot, 'packages', arch === 'arm64' ? 'nexus-native-darwin-arm64' : 'nexus-native-darwin-x64');
const outputRoot = (out) => resolve(repoRoot, out ?? 'artifacts/desktop');

function usage() {
  console.error('Usage: pnpm --dir apps/desktop-electron run package -- [--arch arm64|x64] [--out <dir>]');
  console.error('Options are intentionally closed: --arch, --out, --help. This lane always produces unsigned artifacts.');
}

function command(commandName, args, options = {}) {
  const result = spawnSync(commandName, args, {
    cwd: options.cwd ?? repoRoot,
    env: { ...process.env, ...(options.env ?? {}) },
    encoding: 'utf8',
    stdio: options.stdio ?? 'pipe',
  });
  if (result.status !== 0) {
    const details = `${result.stderr ?? ''}${result.stdout ?? ''}`.trim();
    throw new Error(`${commandName} ${args.join(' ')} failed${details ? `: ${details}` : ''}`);
  }
  return result.stdout ?? '';
}

function commandExists(name) {
  try {
    execFileSync('sh', ['-c', `command -v ${name}`], { cwd: repoRoot, stdio: 'ignore' });
    return true;
  } catch {
    return false;
  }
}

function versionAtLeast(actual, minimum) {
  const a = actual.match(/(\d+)\.(\d+)(?:\.(\d+))?/);
  const b = minimum.match(/(\d+)\.(\d+)(?:\.(\d+))?/);
  if (!a || !b) return false;
  const left = [Number(a[1]), Number(a[2]), Number(a[3] ?? 0)];
  const right = [Number(b[1]), Number(b[2]), Number(b[3] ?? 0)];
  return left[0] > right[0] || (left[0] === right[0] && (left[1] > right[1] || (left[1] === right[1] && left[2] >= right[2])));
}

function nativeManifest(arch) {
  const root = nativePlatformRoot(arch);
  const artifact = join(root, 'native', 'nexus_core_node.node');
  const compatibilityPath = join(root, 'native', 'compatibility.json');
  assertRequiredFiles([artifact, compatibilityPath], 'native payload');
  const manifest = JSON.parse(readFileSync(compatibilityPath, 'utf8'));
  return { artifact, compatibilityPath, manifest };
}

function nativeResetBindingProbe() {
  const probe = [
    "import * as native from '@42ch/nexus-native';",
    "if (typeof native.resetLocalState !== 'function') throw new Error('resetLocalState binding is missing');",
    "const compatibility = native.nativeCompatibility();",
    "process.stdout.write(JSON.stringify(compatibility));",
  ].join(' ');
  const output = command(process.execPath, ['--input-type=module', '-e', probe], { cwd: repoRoot });
  return JSON.parse(output);
}

function preflight(arch) {
  if (process.platform !== 'darwin') throw new Error('package.preflight.platform: unsigned Electron packaging requires macOS (darwin)');
  if (process.arch !== arch) throw new Error(`package.preflight.arch: target ${arch} requires a native ${arch} runner, got ${process.arch}`);
  if (!versionAtLeast(process.versions.node, '22.22.0')) throw new Error(`package.preflight.node: Node >=22.22 is required, got ${process.versions.node}`);
  if (!existsSync(lockfile)) throw new Error(`package.preflight.lockfile: missing ${lockfile}`);
  if (!commandExists('pnpm')) throw new Error('package.preflight.pnpm: pnpm >=11 is required');
  const pnpmVersion = command('pnpm', ['--version']).trim();
  if (!versionAtLeast(pnpmVersion, '11.0.0')) throw new Error(`package.preflight.pnpm: pnpm >=11 is required, got ${pnpmVersion}`);
  for (const tool of ['hdiutil', 'ditto']) {
    if (!commandExists(tool)) throw new Error(`package.preflight.tool: missing ${tool}; install the macOS packaging tool and retry`);
  }
  assertRequiredFiles([
    join(appRoot, 'resources', 'product.json'),
    join(appRoot, 'resources', 'icons', 'app.icns'),
  ], 'desktop resource');
  const appPackage = JSON.parse(readFileSync(join(appRoot, 'package.json'), 'utf8'));
  if (product.id !== PACKAGE_CONTRACT.bundleId || product.name !== PACKAGE_CONTRACT.productName) {
    throw new Error('package.preflight.identity: resources/product.json does not match Nexus identity contract');
  }
  if (product.version !== version || appPackage.version !== version) {
    throw new Error(`package.preflight.version: root/product/Electron versions must match (${version})`);
  }
  const native = nativeManifest(arch);
  const compatibility = assertNativeCompatibility(native.manifest, { arch });
  return { pnpmVersion, native, compatibility };
}

function verifyLoadedNative(preflightInfo, arch) {
  let loadedCompatibility;
  try {
    loadedCompatibility = nativeResetBindingProbe();
  } catch (error) {
    throw new Error(`package.preflight.native: resetLocalState/nativeCompatibility probe failed: ${error.message}`);
  }
  assertNativeCompatibility(loadedCompatibility, { arch, resetBinding: true });
  if (loadedCompatibility.contract_tree_sha256 !== preflightInfo.compatibility.contract_tree_sha256) {
    throw new Error('package.preflight.native: executed compatibility hash differs from bundled metadata');
  }
  return { ...preflightInfo, compatibility: loadedCompatibility };
}

function ensureBuildOutputs() {
  const outputs = [
    [join(webDist, 'index.html'), ['--dir', 'apps/web', 'run', 'build']],
    [join(appRoot, 'dist', 'main.js'), ['--dir', 'apps/desktop-electron', 'run', 'build']],
    [join(appRoot, 'dist', 'preload.js'), ['--dir', 'apps/desktop-electron', 'run', 'build']],
    [join(serviceRoot, 'dist', 'index.js'), ['--dir', 'apps/nexus-service', 'run', 'build']],
    [join(serviceRoot, 'dist', 'main.js'), ['--dir', 'apps/nexus-service', 'run', 'build']],
    [join(nativeLoaderRoot, 'dist', 'index.js'), ['--dir', 'packages/nexus-native', 'run', 'build']],
  ];
  for (const [path, args] of outputs) {
    if (!existsSync(path)) command('pnpm', args, { stdio: 'inherit' });
  }
  assertRequiredFiles(outputs.map(([path]) => path), 'compiled package input');
}

function deployWorkspacePackage(filter, destination) {
  mkdirSync(destination, { recursive: true });
  command('pnpm', ['deploy', '--filter', filter, '--prod', '--offline', destination], { stdio: 'inherit' });
  assertNoSymlinkEscape(destination);
}

function stageWorkspace(stagingRoot, arch) {
  const appStage = join(stagingRoot, 'app');
  const serviceStage = join(stagingRoot, 'service');
  deployWorkspacePackage('nexus-desktop-electron', appStage);
  deployWorkspacePackage('@42ch/nexus-service', serviceStage);
  cpSync(join(appRoot, 'resources'), join(appStage, 'resources'), { recursive: true, dereference: true });
  cpSync(webDist, join(stagingRoot, 'web-dist'), { recursive: true, dereference: true });
  assertNoSymlinkEscape(stagingRoot);
  // Ensure deploy selected the same target package as the native preflight.
  const platformPackage = join(serviceStage, 'node_modules', '@42ch', `nexus-native-darwin-${arch}`, 'native', 'nexus_core_node.node');
  assertRequiredFiles([platformPackage], 'deployed native closure');
  return { appStage, serviceStage, webStage: join(stagingRoot, 'web-dist') };
}

function appFileManifest(appPath) {
  const entries = [];
  const walk = (root, current = root) => {
    for (const entry of readdirSyncSafe(current)) {
      const absolute = join(current, entry);
      const rel = relative(root, absolute).split('\\').join('/');
      if (entry.isDirectory()) walk(root, absolute);
      else entries.push({ path: rel, bytes: statSafe(absolute), sha256: sha256File(absolute) });
    }
  };
  walk(appPath);
  return entries.sort((a, b) => a.path.localeCompare(b.path));
}

function readdirSyncSafe(path) {
  // Dynamic import is unnecessary here; keeping this helper explicit makes the
  // manifest traversal easy to audit against symlink checks.
  return readdirSync(path, { withFileTypes: true }).filter((entry) => !entry.isSymbolicLink());
}
function statSafe(path) { return statSync(path).size; }

function gitReceipt() {
  const revision = command('git', ['rev-parse', 'HEAD']).trim();
  const dirty = command('git', ['status', '--porcelain']).trim().length > 0;
  return { revision, dirty };
}

function createReceipt({ arch, preflightInfo, artifacts, appPath, staging }) {
  const git = gitReceipt();
  const receipt = {
    schema_version: 1,
    product_name: PACKAGE_CONTRACT.productName,
    bundle_id: PACKAGE_CONTRACT.bundleId,
    version,
    git_revision: git.revision,
    dirty: git.dirty,
    arch,
    platform: 'darwin',
    minimum_macos: product.minimum_macos ?? PACKAGE_CONTRACT.minimumMacos,
    node_version: process.versions.node,
    pnpm_version: preflightInfo.pnpmVersion,
    electron_version: PACKAGE_CONTRACT.electronVersion,
    packager_version: PACKAGE_CONTRACT.packagerVersion,
    native_contract_hash: preflightInfo.compatibility.contract_tree_sha256,
    native_target: preflightInfo.compatibility.target_triple,
    inputs: {
      lockfile_sha256: sha256File(lockfile),
      web_dist_sha256: digestTree(staging.webStage),
      service_sha256: digestTree(staging.serviceStage),
      native_sha256: sha256File(preflightInfo.native.artifact),
      native_compatibility_sha256: sha256File(preflightInfo.native.compatibilityPath),
      app_file_manifest: appFileManifest(appPath),
    },
    artifacts: Object.fromEntries(artifacts.map((artifact) => [artifact.relative, {
      path: artifact.relative,
      bytes: artifact.bytes,
      sha256: artifact.sha256,
    }])),
    signing_performed: false,
    notarization_performed: false,
    inherited_signature_metadata: {
      product_pipeline_signature: 'none',
      vendor_signature_preserved: true,
      inspected_without_signing_tools: true,
      note: 'Vendor Mach-O signatures, if present, were not stripped or modified.',
    },
    checks: {
      identity: { result: 'pass', detail: `${PACKAGE_CONTRACT.productName} / ${PACKAGE_CONTRACT.bundleId}` },
      architecture: { result: 'pass', detail: `native darwin/${arch}` },
      native_compatibility: { result: 'pass', detail: 'executed compatibility metadata matched bundled metadata' },
      reset_binding: { result: 'pass', detail: 'resetLocalState binding exported by the native loader' },
      symlink_closure: { result: 'pass', detail: 'staged app, service, and web inputs contain no escaping symlink' },
      unsigned: { result: 'pass', detail: 'packager received no signing configuration and no signing tool was invoked' },
    },
  };
  return assertReceipt(receipt);
}

function createDmg(appPath, destination) {
  const dmgStage = mkdtempSync(join(tmpdir(), 'nexus-dmg-'));
  try {
    cpSync(appPath, join(dmgStage, 'Nexus.app'), { recursive: true, dereference: true });
    symlinkSync('/Applications', join(dmgStage, 'Applications'));
    command('hdiutil', ['create', '-volname', 'Nexus', '-srcfolder', dmgStage, '-ov', '-format', 'UDZO', destination], { stdio: 'inherit' });
  } finally {
    rmSync(dmgStage, { recursive: true, force: true });
  }
}

function createZip(appPath, destination) {
  command('ditto', ['-c', '-k', '--sequesterRsrc', '--keepParent', appPath, destination], { stdio: 'inherit' });
}

function publishAtomic(stagingDir, finalDir) {
  const required = [
    join(stagingDir, 'Nexus.app'),
    join(stagingDir, `Nexus-${version}-darwin-${basename(finalDir).replace('darwin-', '')}-unsigned.dmg`),
    join(stagingDir, `Nexus-${version}-darwin-${basename(finalDir).replace('darwin-', '')}-unsigned.app.zip`),
    join(stagingDir, 'SHA256SUMS'),
    join(stagingDir, 'receipt.json'),
  ];
  assertRequiredFiles(required, 'staged package');
  const parent = dirname(finalDir);
  mkdirSync(parent, { recursive: true });
  const backup = `${finalDir}.previous-${process.pid}`;
  let movedExisting = false;
  try {
    if (existsSync(finalDir)) {
      rmSync(backup, { recursive: true, force: true });
      renameSync(finalDir, backup);
      movedExisting = true;
    }
    renameSync(stagingDir, finalDir);
    if (movedExisting) rmSync(backup, { recursive: true, force: true });
  } catch (error) {
    if (movedExisting && !existsSync(finalDir) && existsSync(backup)) renameSync(backup, finalDir);
    throw error;
  }
}

async function main() {
  const args = parsePackageArgs(process.argv.slice(2));
  if (args.help) {
    usage();
    return;
  }
  assertNoSigningEnvironment();
  const preflightInfo = preflight(args.arch);
  ensureBuildOutputs();
  const verifiedPreflight = verifyLoadedNative(preflightInfo, args.arch);
  const destinationRoot = outputRoot(args.out);
  mkdirSync(destinationRoot, { recursive: true });
  const finalDir = join(destinationRoot, version, `darwin-${args.arch}`);
  const publishStage = mkdtempSync(join(destinationRoot, `.staging-${args.arch}-`));
  const workspaceStage = mkdtempSync(join(tmpdir(), 'nexus-package-workspace-'));
  try {
    const staged = stageWorkspace(workspaceStage, args.arch);
    const packageParent = join(publishStage, 'packager');
    mkdirSync(packageParent, { recursive: true });
    const artifacts = await packager({
      dir: staged.appStage,
      out: packageParent,
      platform: 'darwin',
      arch: args.arch,
      electronVersion: PACKAGE_CONTRACT.electronVersion,
      appBundleId: PACKAGE_CONTRACT.bundleId,
      name: PACKAGE_CONTRACT.productName,
      overwrite: false,
      prune: true,
      asarIntegrityDigest: false,
      asar: { unpack: '**/*.node' },
      extraResource: [staged.webStage, staged.serviceStage],
    });
    if (!artifacts?.length) throw new Error('package.packager: no app artifact returned');
    const appPath = artifacts.find((path) => path.endsWith('.app')) ?? artifacts[0];
    const publishedApp = join(publishStage, 'Nexus.app');
    cpSync(appPath, publishedApp, { recursive: true, dereference: true });
    rmSync(packageParent, { recursive: true, force: true });
    const dmgName = `Nexus-${version}-darwin-${args.arch}-unsigned.dmg`;
    const zipName = `Nexus-${version}-darwin-${args.arch}-unsigned.app.zip`;
    const dmgPath = join(publishStage, dmgName);
    const zipPath = join(publishStage, zipName);
    createDmg(publishedApp, dmgPath);
    createZip(publishedApp, zipPath);
    const receipt = createReceipt({
      arch: args.arch,
      preflightInfo: verifiedPreflight,
      artifacts: [
        { relative: zipName, bytes: statSync(zipPath).size, sha256: sha256File(zipPath) },
        { relative: dmgName, bytes: statSync(dmgPath).size, sha256: sha256File(dmgPath) },
      ],
      appPath: publishedApp,
      staging: staged,
    });
    writeFileSync(join(publishStage, 'receipt.json'), `${JSON.stringify(receipt, null, 2)}\n`);
    writeFileSync(join(publishStage, 'SHA256SUMS'), `${sha256File(zipPath)}  ${zipName}\n${sha256File(dmgPath)}  ${dmgName}\n`);
    publishAtomic(publishStage, finalDir);
    console.log(JSON.stringify(receipt, null, 2));
  } catch (error) {
    rmSync(publishStage, { recursive: true, force: true });
    throw error;
  } finally {
    rmSync(workspaceStage, { recursive: true, force: true });
  }
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
});
