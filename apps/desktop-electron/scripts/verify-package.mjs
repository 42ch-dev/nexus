#!/usr/bin/env node
/**
 * Read-only verifier for one published unsigned Electron architecture directory.
 * It inspects receipts, archive digests, app metadata, Mach-O headers, and the
 * compiled host policy. It never mounts, rewrites, signs, or launches an app.
 */
import { execFileSync, spawnSync } from 'node:child_process';
import { existsSync, lstatSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { basename, join, relative, resolve } from 'node:path';
import { assertReceipt, PACKAGE_CONTRACT, sha256File } from './package-contract.mjs';

class VerificationError extends Error {}

function fail(message) {
  throw new VerificationError(message);
}

function parseArgs(argv) {
  const args = { dir: null, help: false };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--help' || arg === '-h') {
      if (args.help) fail('duplicate --help');
      args.help = true;
    } else if (arg === '--dir') {
      if (args.dir !== null) fail('duplicate --dir');
      const value = argv[++index];
      if (!value || value.startsWith('-')) fail('--dir requires a value');
      args.dir = value;
    } else if (arg.startsWith('--')) {
      fail(`unsupported option ${arg}; accepted option is --dir`);
    } else {
      fail(`unexpected argument ${arg}`);
    }
  }
  if (!args.help && args.dir === null) fail('--dir is required');
  return args;
}

function usage() {
  console.error('Usage: node apps/desktop-electron/scripts/verify-package.mjs --dir artifacts/desktop/<version>/darwin-<arch>');
  console.error('Checks package identity, receipt/archive/app digests, native headers, and compiled host policy. GUI qualification is not claimed.');
}

function requireFile(path, label) {
  if (!existsSync(path)) fail(`${label} missing: ${path}`);
  const stat = lstatSync(path);
  if (!stat.isFile() || stat.size === 0) fail(`${label} is empty or not a regular file: ${path}`);
}

function requireDirectory(path, label) {
  if (!existsSync(path) || !lstatSync(path).isDirectory()) fail(`${label} missing: ${path}`);
}

function walkFiles(root) {
  const files = [];
  const visit = (current) => {
    for (const entry of readdirSync(current, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
      const path = join(current, entry.name);
      const stat = lstatSync(path);
      if (stat.isSymbolicLink()) {
        continue;
      }
      if (stat.isDirectory()) visit(path);
      else if (stat.isFile()) files.push(path);
      else fail(`unsupported app bundle entry: ${relative(root, path)}`);
    }
  };
  visit(root);
  return files;
}

function appManifest(appPath) {
  return walkFiles(appPath)
    .map((path) => ({
      path: relative(appPath, path).split('\\').join('/'),
      bytes: statSync(path).size,
      sha256: sha256File(path),
    }))
    .sort((a, b) => a.path.localeCompare(b.path));
}

function jsonPlist(path) {
  const result = spawnSync('plutil', ['-convert', 'json', '-o', '-', '--', path], { encoding: 'utf8' });
  if (result.status !== 0) fail(`Info.plist cannot be read as JSON: ${(result.stderr || '').trim()}`);
  try {
    return JSON.parse(result.stdout);
  } catch (error) {
    fail(`Info.plist JSON is malformed: ${error.message}`);
  }
}

function inspectMachO(path, expectedArch, minimumMacos, label) {
  let output;
  try {
    output = execFileSync('file', ['-b', path], { encoding: 'utf8' }).trim();
  } catch (error) {
    fail(`${label} header inspection failed for ${path}: ${error.message}`);
  }
  if (!/Mach-O/.test(output)) fail(`${label} is not a Mach-O binary: ${path} (${output})`);
  const archPattern = expectedArch === 'arm64' ? /arm64|aarch64/i : /x86_64|x64/i;
  if (!archPattern.test(output)) fail(`${label} architecture does not match darwin/${expectedArch}: ${path} (${output})`);
  let loadCommands;
  try {
    loadCommands = execFileSync('otool', ['-l', path], { encoding: 'utf8' });
  } catch (error) {
    fail(`${label} load-command inspection failed for ${path}: ${error.message}`);
  }
  const floors = [...loadCommands.matchAll(/\bminos\s+(\d+)\.(\d+)(?:\.(\d+))?/g)].map((match) => [
    Number(match[1]),
    Number(match[2]),
    Number(match[3] ?? 0),
  ]);
  const maximum = String(minimumMacos).split('.').map(Number);
  if (floors.some(([major, minor, patch]) => major > maximum[0] || (major === maximum[0] && (minor > maximum[1] || (minor === maximum[1] && patch > (maximum[2] ?? 0)))))) {
    fail(`${label} requires a macOS release newer than ${minimumMacos}: ${path}`);
  }
  return { file: output, minos: floors };
}

function inspectArchives(dir, receipt, version, arch) {
  const expected = {
    dmg: `Nexus-${version}-darwin-${arch}-unsigned.dmg`,
    zip: `Nexus-${version}-darwin-${arch}-unsigned.app.zip`,
  };
  const sums = readFileSync(join(dir, 'SHA256SUMS'), 'utf8').trim().split(/\r?\n/).filter(Boolean);
  const sumMap = new Map();
  for (const line of sums) {
    const match = line.match(/^([a-f0-9]{64})\s+(.+)$/);
    if (!match) fail(`SHA256SUMS contains malformed row: ${line}`);
    if (sumMap.has(match[2])) fail(`SHA256SUMS contains duplicate row: ${match[2]}`);
    sumMap.set(match[2], match[1]);
  }
  for (const name of Object.values(expected)) {
    const path = join(dir, name);
    requireFile(path, 'distribution artifact');
    const digest = sha256File(path);
    if (sumMap.get(name) !== digest) fail(`SHA256SUMS digest mismatch for ${name}`);
    const receiptArtifact = receipt.artifacts[name];
    if (!receiptArtifact || receiptArtifact.path !== name || receiptArtifact.sha256 !== digest || receiptArtifact.bytes !== statSync(path).size) {
      fail(`receipt artifact record mismatch for ${name}`);
    }
  }
  if (sumMap.size !== 2) fail(`SHA256SUMS must contain exactly the app ZIP and DMG (${sumMap.size} rows found)`);
  return { names: expected, sha256: Object.fromEntries(Object.entries(expected).map(([kind, name]) => [kind, sumMap.get(name)])) };
}

function verifyHostPolicy(appPath) {
  const asar = join(appPath, 'Contents', 'Resources', 'app.asar');
  requireFile(asar, 'compiled host archive');
  const source = readFileSync(asar).toString('utf8');
  const policies = [
    ['contextIsolation', /contextIsolation\s*:\s*true/],
    ['nodeIntegration', /nodeIntegration\s*:\s*false/],
    ['sandbox', /sandbox\s*:\s*true/],
    ['webSecurity', /webSecurity\s*:\s*true/],
  ];
  for (const [name, pattern] of policies) {
    if (!pattern.test(source)) fail(`compiled host policy missing ${name} invariant`);
  }
  if (!/utilityProcess\.fork|utility-host/.test(source)) fail('compiled host policy has no utility-process service owner');
  return policies.map(([name]) => name);
}

function verifyPackage(dir) {
  requireDirectory(dir, 'package directory');
  const receiptPath = join(dir, 'receipt.json');
  requireFile(receiptPath, 'receipt');
  const receipt = JSON.parse(readFileSync(receiptPath, 'utf8'));
  assertReceipt(receipt);
  const archDir = basename(dir);
  if (archDir !== `darwin-${receipt.arch}`) fail(`package directory ${archDir} does not match receipt architecture ${receipt.arch}`);
  if (receipt.minimum_macos !== PACKAGE_CONTRACT.minimumMacos) fail(`minimum macOS must be ${PACKAGE_CONTRACT.minimumMacos}`);
  if (receipt.inherited_signature_metadata?.product_pipeline_signature !== 'none') fail('receipt claims a product signature');
  if (receipt.inherited_signature_metadata?.vendor_signature_preserved !== true) fail('receipt does not honestly preserve vendor-signature metadata');
  const appPath = join(dir, 'Nexus.app');
  requireDirectory(appPath, 'Nexus.app');
  const plistPath = join(appPath, 'Contents', 'Info.plist');
  requireFile(plistPath, 'app Info.plist');
  const plist = jsonPlist(plistPath);
  if (plist.CFBundleIdentifier !== PACKAGE_CONTRACT.bundleId) fail(`Info.plist bundle ID mismatch: ${plist.CFBundleIdentifier}`);
  if (plist.CFBundleName !== PACKAGE_CONTRACT.productName && plist.CFBundleDisplayName !== PACKAGE_CONTRACT.productName) fail('Info.plist product name mismatch');
  if (String(plist.CFBundleShortVersionString ?? plist.CFBundleVersion) !== String(receipt.version)) fail('Info.plist version mismatch');
  const executable = plist.CFBundleExecutable || 'Nexus';
  const executablePath = join(appPath, 'Contents', 'MacOS', executable);
  requireFile(executablePath, 'app executable');
  const headers = [{ path: executablePath, label: 'app executable', header: inspectMachO(executablePath, receipt.arch, receipt.minimum_macos, 'app executable') }];
  const unpacked = join(appPath, 'Contents', 'Resources', 'app.asar.unpacked');
  requireDirectory(unpacked, 'unpacked native dependency closure');
  for (const path of walkFiles(unpacked).filter((candidate) => candidate.endsWith('.node'))) {
    headers.push({ path, label: 'native dependency', header: inspectMachO(path, receipt.arch, receipt.minimum_macos, 'native dependency') });
  }
  if (headers.length < 2) fail('no unpacked native .node payload was found');
  const manifest = appManifest(appPath);
  if (JSON.stringify(manifest) !== JSON.stringify(receipt.inputs.app_file_manifest)) fail('receipt app_file_manifest does not match the published app');
  const archives = inspectArchives(dir, receipt, receipt.version, receipt.arch);
  verifyHostPolicy(appPath);
  return {
    package_dir: dir,
    product: { name: receipt.product_name, bundle_id: receipt.bundle_id, version: receipt.version },
    architecture: receipt.arch,
    app: { path: 'Nexus.app', files: manifest.length, executable: executable, headers },
    archives,
    checks: {
      receipt: 'pass',
      app_manifest: 'pass',
      archive_digests: 'pass',
      native_headers: 'pass',
      compiled_host_policy: 'pass',
      signing_performed: false,
      notarization_performed: false,
      gui_qualification: 'not claimed',
    },
  };
}

const args = parseArgs(process.argv.slice(2));
if (args.help) {
  usage();
} else {
  try {
    console.log(JSON.stringify(verifyPackage(resolve(args.dir)), null, 2));
  } catch (error) {
    console.error(`verify-package: ${error instanceof Error ? error.message : String(error)}`);
    process.exitCode = 1;
  }
}
