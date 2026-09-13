import { createRequire } from 'node:module';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import type { NativeCompatibility } from '@42ch/nexus-contracts';

const require = createRequire(import.meta.url);
const __dirname = dirname(fileURLToPath(import.meta.url));

const PLACEHOLDER_HASH = '0'.repeat(64);

export interface ProviderCallbacksNative {
  call(requestJson: string): Promise<string>;
  next(requestJson: string): Promise<string>;
}

export interface NativeCoreBinding {
  activePrincipal(): Promise<string>;
  worldKbGraph(principal: string, worldId: string, includeSuggested: boolean): Promise<Uint8Array>;
  patchWorldKbEntity(principal: string, worldId: string, requestJson: Uint8Array): Promise<Uint8Array>;
  worldKbCandidates(
    principal: string,
    worldId: string,
    limit?: number | null,
    cursor?: string | null,
  ): Promise<Uint8Array>;
  hostQuery(requestJson: Uint8Array): Promise<Uint8Array>;
  changes(principal: string, requestJson: Uint8Array): Promise<Uint8Array>;
  providerCall(requestJson: Uint8Array): Promise<Uint8Array>;
  nextProviderEvents(operationId: string, maxEvents: number, maxBytes: number): Promise<Uint8Array>;
  close(): Promise<Uint8Array>;
}

export interface NativeBinding {
  compatibility(): string;
  open(optionsJson: string, callbacks?: ProviderCallbacksNative): NativeCoreBinding;
}

interface PlatformPackageManifest {
  name: string;
  version: string;
  os?: string[];
  cpu?: string[];
  libc?: string[];
}

/** The one platform this process may load, with its Rust target triple. */
export interface PlatformTarget {
  name: string;
  targetTriple: string;
  libc?: 'glibc';
}

/** Runtime-derived expectations a manifest must match before any DB open. */
export interface RuntimeExpectations {
  target_triple: string;
  package_version: string;
  contract_tree_sha256?: string;
}

export function detectLinuxLibc(): 'glibc' | 'musl' {
  try {
    const report = process.report?.getReport?.() as
      | { header?: { glibcVersionRuntime?: string } }
      | undefined;
    if (report?.header?.glibcVersionRuntime) return 'glibc';
  } catch {
    // fall through to musl
  }
  return 'musl';
}

/** Current OS/arch/libc → the exact platform package and Rust target triple. */
export function expectedPlatformPackage(): PlatformTarget {
  const { platform, arch } = process;
  if (platform === 'darwin' && arch === 'arm64') {
    return { name: '@42ch/nexus-native-darwin-arm64', targetTriple: 'aarch64-apple-darwin' };
  }
  if (platform === 'darwin' && arch === 'x64') {
    return { name: '@42ch/nexus-native-darwin-x64', targetTriple: 'x86_64-apple-darwin' };
  }
  if (platform === 'win32' && arch === 'x64') {
    return { name: '@42ch/nexus-native-win32-x64-msvc', targetTriple: 'x86_64-pc-windows-msvc' };
  }
  if (platform === 'linux' && arch === 'x64') {
    return {
      name: '@42ch/nexus-native-linux-x64-gnu',
      targetTriple: 'x86_64-unknown-linux-gnu',
      libc: 'glibc',
    };
  }
  throw new Error(`unsupported platform ${platform}/${arch}`);
}

export function expectedTargetTriple(): string {
  return expectedPlatformPackage().targetTriple;
}

function resolvePlatformPackageRoot(pkgName: string): string {
  const shortName = pkgName.replace('@42ch/', '');
  const workspacePath = join(__dirname, '..', '..', shortName);
  if (existsSync(join(workspacePath, 'package.json'))) {
    return workspacePath;
  }
  return dirname(require.resolve(`${pkgName}/package.json`));
}

export function readPackageManifest(pkgName: string): PlatformPackageManifest {
  const pkgJsonPath = join(resolvePlatformPackageRoot(pkgName), 'package.json');
  const parsed = JSON.parse(readFileSync(pkgJsonPath, 'utf8')) as PlatformPackageManifest;
  if (typeof parsed.name !== 'string' || typeof parsed.version !== 'string') {
    throw new Error(`invalid platform package manifest for ${pkgName}`);
  }
  return parsed;
}

/** Exact-platform fencing: os/cpu (and Linux libc) are required, not optional. */
function assertPlatformManifest(
  manifest: PlatformPackageManifest,
  target: PlatformTarget,
): void {
  const { platform, arch } = process;
  if (!manifest.os?.length) throw new Error('platform package manifest is missing "os"');
  if (!manifest.os.includes(platform)) {
    throw new Error(`platform package os mismatch: expected ${platform}, got ${manifest.os.join(',')}`);
  }
  if (!manifest.cpu?.length) throw new Error('platform package manifest is missing "cpu"');
  if (!manifest.cpu.includes(arch)) {
    throw new Error(`platform package cpu mismatch: expected ${arch}, got ${manifest.cpu.join(',')}`);
  }
  if (target.libc === 'glibc') {
    if (detectLinuxLibc() !== 'glibc') {
      throw new Error('linux musl host cannot load the gnu platform package');
    }
    if (!manifest.libc?.length) throw new Error('platform package manifest is missing "libc"');
    if (!manifest.libc.includes('gnu')) {
      throw new Error(`platform package libc mismatch: expected gnu, got ${manifest.libc.join(',')}`);
    }
  }
}

export function loadNodePath(): string {
  const target = expectedPlatformPackage();
  assertPlatformManifest(readPackageManifest(target.name), target);
  const nodePath = join(resolvePlatformPackageRoot(target.name), 'native', 'nexus_core_node.node');
  if (!existsSync(nodePath)) {
    throw new Error(
      `platform native artifact missing at ${nodePath}; run packages/nexus-native/scripts/build.mjs`,
    );
  }
  return nodePath;
}

export function loadNativeBinding(): NativeBinding {
  const nodePath = loadNodePath();
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  return require(nodePath) as NativeBinding;
}

export function readCompatibilityManifest(binding: NativeBinding): NativeCompatibility {
  const parsed = JSON.parse(binding.compatibility()) as NativeCompatibility;
  assertManifestShape(parsed, 'native compatibility manifest');
  return parsed;
}

/** The manifest shipped beside the loaded artifact — required, never optional. */
export function readBundledCompatibility(nodePath?: string): NativeCompatibility {
  const resolved = nodePath ?? loadNodePath();
  const compatPath = join(dirname(resolved), 'compatibility.json');
  if (!existsSync(compatPath)) {
    throw new Error(`missing platform compatibility manifest at ${compatPath}`);
  }
  const parsed = JSON.parse(readFileSync(compatPath, 'utf8')) as NativeCompatibility;
  assertManifestShape(parsed, 'platform compatibility manifest');
  return parsed;
}

function assertManifestShape(manifest: NativeCompatibility, label: string): void {
  const fields: readonly (keyof NativeCompatibility)[] = [
    'native_api_version',
    'writer_protocol',
    'target_triple',
    'package_version',
    'contract_tree_sha256',
    'db_schema_min',
    'db_schema_max',
    'napi_minimum',
  ];
  if (typeof manifest !== 'object' || manifest === null) {
    throw new Error(`${label}: not an object`);
  }
  for (const field of fields) {
    if (manifest[field] === undefined) throw new Error(`${label}: missing "${field}"`);
  }
}

/**
 * Fence a manifest against the *runtime-derived* expectations (target triple,
 * package version) plus, when supplied, the adjacent manifest's contract hash.
 */
export function assertCompatibility(
  manifest: NativeCompatibility,
  expected?: RuntimeExpectations,
): void {
  const napiVersion = Number(process.versions.napi ?? '0');
  if (napiVersion < manifest.napi_minimum) {
    throw new Error(`napi version ${napiVersion} < required ${manifest.napi_minimum}`);
  }
  if (manifest.native_api_version !== 1) {
    throw new Error(`native_api_version mismatch: ${manifest.native_api_version}`);
  }
  if (manifest.writer_protocol !== 1) {
    throw new Error(`writer_protocol mismatch: ${manifest.writer_protocol}`);
  }
  if (!/^[a-f0-9]{64}$/.test(manifest.contract_tree_sha256)) {
    throw new Error('contract_tree_sha256 shape invalid');
  }
  if (manifest.contract_tree_sha256 === PLACEHOLDER_HASH) {
    throw new Error('contract_tree_sha256 placeholder rejected');
  }
  if (manifest.db_schema_min > manifest.db_schema_max) {
    throw new Error('db_schema range invalid');
  }
  if (expected) {
    if (manifest.target_triple !== expected.target_triple) {
      throw new Error(
        `target_triple mismatch: host requires ${expected.target_triple}, artifact reports ${manifest.target_triple}`,
      );
    }
    if (manifest.package_version !== expected.package_version) {
      throw new Error(
        `package_version mismatch: expected ${expected.package_version}, got ${manifest.package_version}`,
      );
    }
    if (
      expected.contract_tree_sha256 &&
      manifest.contract_tree_sha256 !== expected.contract_tree_sha256
    ) {
      throw new Error('contract_tree_sha256 mismatch against the adjacent manifest');
    }
  }
}
