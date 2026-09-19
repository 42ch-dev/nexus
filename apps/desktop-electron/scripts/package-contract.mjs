import { createHash } from 'node:crypto';
import { existsSync, lstatSync, readdirSync, readFileSync, readlinkSync, statSync } from 'node:fs';
import { join, relative, resolve } from 'node:path';

export const PACKAGE_CONTRACT = Object.freeze({
  schemaVersion: 1,
  productName: 'Nexus',
  bundleId: 'io.nexus42.desktop',
  electronVersion: '44.3.0',
  packagerVersion: '20.3.0',
  minimumMacos: '13.0',
  supportedArchitectures: Object.freeze(['arm64', 'x64']),
});

export class PackageContractError extends Error {
  constructor(message, code = 'package.contract') {
    super(`${code}: ${message}`);
    this.name = 'PackageContractError';
    this.code = code;
  }
}

function fail(message, code) {
  throw new PackageContractError(message, code);
}

export function parsePackageArgs(argv, nativeArch = process.arch) {
  const args = { arch: nativeArch, out: null, help: false };
  const seen = new Set();
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--' && index === 0) continue;
    if (arg === '--help' || arg === '-h') {
      if (seen.has('help')) fail('duplicate --help', 'package.args.duplicate');
      seen.add('help');
      args.help = true;
      continue;
    }
    if (arg === '--arch' || arg === '--out') {
      if (seen.has(arg)) fail(`duplicate ${arg}`, 'package.args.duplicate');
      seen.add(arg);
      const value = argv[++index];
      if (!value || value.startsWith('-')) fail(`${arg} requires a value`, 'package.args.value');
      if (arg === '--arch') args.arch = value;
      else args.out = value;
      continue;
    }
    if (arg.startsWith('--arch=') || arg.startsWith('--out=')) {
      fail(`${arg.split('=')[0]} must use a separate value`, 'package.args.syntax');
    }
    fail(`unsupported option ${arg}; accepted options are --arch, --out, and --help`, 'package.args.unknown');
  }
  if (!PACKAGE_CONTRACT.supportedArchitectures.includes(args.arch)) {
    fail(`--arch must be arm64 or x64, got ${args.arch}`, 'package.args.arch');
  }
  return args;
}

export function assertNoSigningEnvironment(env = process.env) {
  const suspicious = Object.keys(env).filter((key) =>
    /^(APPLE_|CSC_|ELECTRON_BUILDER_|SIGNING_|NOTARIZE_)/i.test(key),
  );
  if (suspicious.length > 0) {
    fail(
      `credential/signing environment is not accepted by the unsigned packaging lane (${suspicious.sort().join(', ')})`,
      'package.unsigned.environment',
    );
  }
}

export function sha256Bytes(bytes) {
  return createHash('sha256').update(bytes).digest('hex');
}

export function sha256File(path) {
  return sha256Bytes(readFileSync(path));
}

export function digestTree(root) {
  const hash = createHash('sha256');
  const walk = (current) => {
    const entries = readdirSync(current, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name));
    for (const entry of entries) {
      const absolute = join(current, entry.name);
      const rel = relative(root, absolute).split('\\').join('/');
      if (entry.isSymbolicLink()) fail(`symlink in reproducible input: ${rel}`, 'package.input.symlink');
      if (entry.isDirectory()) {
        hash.update(`D:${rel}\n`);
        walk(absolute);
      } else if (entry.isFile()) {
        hash.update(`F:${rel}:${sha256File(absolute)}\n`);
      } else {
        fail(`unsupported input entry: ${rel}`, 'package.input.entry');
      }
    }
  };
  walk(root);
  return hash.digest('hex');
}

export function assertNoSymlinkEscape(root) {
  const absoluteRoot = resolve(root);
  const queue = [absoluteRoot];
  while (queue.length > 0) {
    const current = queue.pop();
    for (const entry of readdirSync(current, { withFileTypes: true })) {
      const absolute = join(current, entry.name);
      const stat = lstatSync(absolute);
      if (stat.isSymbolicLink()) {
        const target = resolve(absolute, '..', readlinkSync(absolute));
        if (target !== absoluteRoot && !target.startsWith(`${absoluteRoot}/`)) {
          fail(`staging symlink escapes root: ${absolute} -> ${target}`, 'package.staging.symlink');
        }
      } else if (stat.isDirectory()) {
        queue.push(absolute);
      }
    }
  }
}

export function assertNativeCompatibility(manifest, { arch, resetBinding } = {}) {
  if (!manifest || typeof manifest !== 'object' || Array.isArray(manifest)) {
    fail('native compatibility metadata must be an object', 'package.native.compatibility');
  }
  const required = [
    'native_api_version',
    'writer_protocol',
    'target_triple',
    'package_version',
    'contract_tree_sha256',
    'db_schema_min',
    'db_schema_max',
    'napi_minimum',
  ];
  for (const field of required) {
    if (!(field in manifest)) fail(`native compatibility metadata missing ${field}`, 'package.native.compatibility');
  }
  if (manifest.native_api_version !== 1 || manifest.writer_protocol !== 1 || manifest.napi_minimum !== 8) {
    fail('native compatibility protocol/API/N-API floors do not match version 1', 'package.native.compatibility');
  }
  const expectedArch = arch === 'arm64' ? /aarch64|arm64/i : /x86_64|x64/i;
  if (!/darwin|apple/i.test(manifest.target_triple) || !expectedArch.test(manifest.target_triple)) {
    fail(`native target ${manifest.target_triple} is incompatible with darwin/${arch}`, 'package.native.arch');
  }
  if (!/^[a-f0-9]{64}$/.test(manifest.contract_tree_sha256) || /^0+$/.test(manifest.contract_tree_sha256)) {
    fail('native contract hash is missing, malformed, or a placeholder', 'package.native.compatibility');
  }
  if (!Number.isInteger(manifest.db_schema_min) || !Number.isInteger(manifest.db_schema_max) || manifest.db_schema_min > manifest.db_schema_max) {
    fail('native database schema bounds are invalid', 'package.native.compatibility');
  }
  if (resetBinding === false) fail('native package does not expose resetLocalState binding', 'package.native.reset');
  return manifest;
}

export function assertReceipt(receipt) {
  const keys = [
    'schema_version', 'product_name', 'bundle_id', 'version', 'git_revision', 'dirty', 'arch', 'platform',
    'minimum_macos', 'node_version', 'pnpm_version', 'electron_version', 'packager_version',
    'native_contract_hash', 'native_target', 'inputs', 'artifacts', 'signing_performed',
    'notarization_performed', 'inherited_signature_metadata', 'checks',
  ];
  const actual = Object.keys(receipt ?? {}).sort();
  const expected = [...keys].sort();
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    fail(`receipt top-level keys are not the closed version-1 schema: ${actual.join(', ')}`, 'package.receipt.schema');
  }
  if (receipt.schema_version !== 1 || receipt.product_name !== PACKAGE_CONTRACT.productName || receipt.bundle_id !== PACKAGE_CONTRACT.bundleId) {
    fail('receipt identity does not match the frozen package contract', 'package.receipt.identity');
  }
  if (receipt.platform !== 'darwin' || !PACKAGE_CONTRACT.supportedArchitectures.includes(receipt.arch)) {
    fail('receipt platform or architecture is invalid', 'package.receipt.platform');
  }
  if (receipt.signing_performed !== false || receipt.notarization_performed !== false) {
    fail('unsigned receipt contains a signing or notarization result', 'package.receipt.unsigned');
  }
  if (!receipt.inputs || !receipt.artifacts || !receipt.checks) fail('receipt input, artifact, and check sections are required', 'package.receipt.schema');
  return receipt;
}

export function assertRequiredFiles(files, label = 'prerequisite') {
  for (const file of files) {
    if (!existsSync(file)) fail(`${label} missing: ${file}`, 'package.preflight.missing');
    const stat = statSync(file);
    if (!stat.isFile() || stat.size === 0) fail(`${label} is empty or not a file: ${file}`, 'package.preflight.missing');
  }
}
export function assertPreflightFiles(entries) {
  for (const { path, label, code, action } of entries) {
    if (!existsSync(path)) fail(`${label} missing: ${path}; ${action}`, code);
    const stat = statSync(path);
    if (!stat.isFile() || stat.size === 0) fail(`${label} is empty or not a file: ${path}; ${action}`, code);
  }
}

export function assertDependencyClosure({ lockfile, virtualStore, workspaceRoots, requiredPaths = [] }) {
  const action = 'run pnpm install --frozen-lockfile';
  const requiredDirectories = [
    [virtualStore, 'pnpm virtual store'],
    ...workspaceRoots.map((root) => [root, 'workspace node_modules']),
    ...requiredPaths,
  ];
  for (const [path, label] of requiredDirectories) {
    if (!existsSync(path) || !statSync(path).isDirectory()) {
      fail(`${label} missing: ${path}; ${action}`, 'package.preflight.missing_dependency_closure');
    }
  }
  const virtualStoreLockfile = join(virtualStore, 'lock.yaml');
  if (!existsSync(virtualStoreLockfile) || !statSync(virtualStoreLockfile).isFile()) {
    fail(`pnpm virtual-store lockfile missing: ${virtualStoreLockfile}; ${action}`, 'package.preflight.missing_dependency_closure');
  }
  if (!readFileSync(lockfile).equals(readFileSync(virtualStoreLockfile))) {
    fail(`installed dependency closure does not match ${lockfile}; ${action}`, 'package.preflight.missing_dependency_closure');
  }
}
