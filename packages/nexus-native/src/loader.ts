import { createRequire } from 'node:module';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import type { NativeCompatibility } from '@42ch/nexus-contracts';

const require = createRequire(import.meta.url);
const __dirname = dirname(fileURLToPath(import.meta.url));

const PLACEHOLDER_HASH = '0000000000000000000000000000000000000000000000000000000000000000';

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

export function expectedPlatformPackage(): { name: string; libc?: string } {
  const { platform, arch } = process;
  if (platform === 'darwin' && arch === 'arm64') return { name: '@42ch/nexus-native-darwin-arm64' };
  if (platform === 'darwin' && arch === 'x64') return { name: '@42ch/nexus-native-darwin-x64' };
  if (platform === 'win32' && arch === 'x64') return { name: '@42ch/nexus-native-win32-x64-msvc' };
  if (platform === 'linux' && arch === 'x64') return { name: '@42ch/nexus-native-linux-x64-gnu', libc: 'glibc' };
  throw new Error(`unsupported platform ${platform}/${arch}`);
}

function resolvePlatformPackageRoot(pkgName: string): string {
  const shortName = pkgName.replace('@42ch/', '');
  const workspacePath = join(__dirname, '..', '..', shortName);
  const workspaceManifest = join(workspacePath, 'package.json');
  if (existsSync(workspaceManifest)) {
    return workspacePath;
  }
  return dirname(require.resolve(`${pkgName}/package.json`));
}

export function readPackageManifest(pkgName: string): PlatformPackageManifest {
  const pkgJsonPath = join(resolvePlatformPackageRoot(pkgName), 'package.json');
  const manifest = JSON.parse(readFileSync(pkgJsonPath, 'utf8')) as PlatformPackageManifest;
  if (!manifest.name || !manifest.version) {
    throw new Error(`invalid platform package manifest for ${pkgName}`);
  }
  return manifest;
}

function detectLinuxLibc(): 'glibc' | 'musl' {
  try {
    const report = process.report?.getReport?.() as { header?: { glibcVersionRuntime?: string } } | undefined;
    if (report?.header?.glibcVersionRuntime) return 'glibc';
  } catch {
    // ignore
  }
  return 'musl';
}

function assertPlatformManifest(
  manifest: PlatformPackageManifest,
  expected: ReturnType<typeof expectedPlatformPackage>,
): void {
  const { platform, arch } = process;
  if (manifest.os && !manifest.os.includes(platform)) {
    throw new Error(`platform package os mismatch: expected ${platform}, got ${manifest.os.join(',')}`);
  }
  if (manifest.cpu && !manifest.cpu.includes(arch)) {
    throw new Error(`platform package cpu mismatch: expected ${arch}, got ${manifest.cpu.join(',')}`);
  }
  if (expected.libc === 'glibc') {
    if (detectLinuxLibc() !== 'glibc') {
      throw new Error('linux musl host cannot load gnu platform package');
    }
    if (manifest.libc && !manifest.libc.includes('gnu')) {
      throw new Error('platform package libc mismatch: expected gnu');
    }
  }
}

export function loadNodePath(): string {
  const expected = expectedPlatformPackage();
  const manifest = readPackageManifest(expected.name);
  assertPlatformManifest(manifest, expected);
  const nodePath = join(resolvePlatformPackageRoot(expected.name), 'native', 'nexus_core_node.node');
  if (!existsSync(nodePath)) {
    throw new Error(`platform native artifact missing at ${nodePath}; run packages/nexus-native/scripts/build.mjs`);
  }
  return nodePath;
}

export function loadNativeBinding(): NativeBinding {
  const nodePath = loadNodePath();
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  return require(nodePath) as NativeBinding;
}

export function readCompatibilityManifest(binding: NativeBinding): NativeCompatibility {
  return JSON.parse(binding.compatibility()) as NativeCompatibility;
}

export function readBundledCompatibility(nodePath?: string): NativeCompatibility {
  const resolved = nodePath ?? loadNodePath();
  const compatPath = join(dirname(resolved), 'compatibility.json');
  try {
    return JSON.parse(readFileSync(compatPath, 'utf8')) as NativeCompatibility;
  } catch {
    throw new Error(`missing platform compatibility manifest at ${compatPath}`);
  }
}

export function assertCompatibility(
  manifest: NativeCompatibility,
  expected?: NativeCompatibility,
  packageVersion?: string,
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
  if (packageVersion && manifest.package_version !== packageVersion) {
    throw new Error('package_version mismatch');
  }
  if (expected) {
    if (manifest.contract_tree_sha256 !== expected.contract_tree_sha256) {
      throw new Error('contract_tree_sha256 mismatch');
    }
    if (manifest.package_version !== expected.package_version) {
      throw new Error('package_version mismatch');
    }
    if (manifest.target_triple !== expected.target_triple) {
      throw new Error('target_triple mismatch');
    }
    if (manifest.db_schema_min !== expected.db_schema_min || manifest.db_schema_max !== expected.db_schema_max) {
      throw new Error('db_schema mismatch');
    }
  }
}
