import { createRequire } from 'node:module';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import type { NativeCompatibility } from '@42ch/nexus-contracts';

const require = createRequire(import.meta.url);
const __dirname = dirname(fileURLToPath(import.meta.url));

export interface NativeBinding {
  compatibility(): string;
  registerProviderCallbacks(callbacks: ProviderCallbacksNative): void;
  open(optionsJson: string): Promise<NativeCoreBinding>;
}

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

function expectedPlatformPackage(): string {
  const { platform, arch } = process;
  if (platform === 'darwin' && arch === 'arm64') return '@42ch/nexus-native-darwin-arm64';
  if (platform === 'darwin' && arch === 'x64') return '@42ch/nexus-native-darwin-x64';
  if (platform === 'win32' && arch === 'x64') return '@42ch/nexus-native-win32-x64-msvc';
  if (platform === 'linux' && arch === 'x64') return '@42ch/nexus-native-linux-x64-gnu';
  throw new Error(`unsupported platform ${platform}/${arch}`);
}

function loadNodePath(): string {
  const pkg = expectedPlatformPackage();
  try {
    return require.resolve(`${pkg}/native/nexus_core_node.node`);
  } catch {
    const local = join(__dirname, '..', 'native', 'nexus_core_node.node');
    return local;
  }
}

export function loadNativeBinding(): NativeBinding {
  const nodePath = loadNodePath();
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  return require(nodePath) as NativeBinding;
}

export function readCompatibilityManifest(binding: NativeBinding): NativeCompatibility {
  return JSON.parse(binding.compatibility()) as NativeCompatibility;
}

export function assertCompatibility(manifest: NativeCompatibility, expectedHash?: string): void {
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
  if (expectedHash && manifest.contract_tree_sha256 !== expectedHash) {
    throw new Error(`contract_tree_sha256 mismatch`);
  }
}

export function readBundledCompatibility(): NativeCompatibility | undefined {
  try {
    const path = join(__dirname, '..', 'native', 'compatibility.json');
    return JSON.parse(readFileSync(path, 'utf8')) as NativeCompatibility;
  } catch {
    return undefined;
  }
}
