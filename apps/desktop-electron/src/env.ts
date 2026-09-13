import { existsSync, readFileSync, realpathSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, isAbsolute, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import type { UtilityConfig } from './ipc.js';

const require = createRequire(import.meta.url);

const ALLOWED_ENV_KEYS = new Set([
  'PATH',
  'HOME',
  'USER',
  'LOGNAME',
  'TMPDIR',
  'TEMP',
  'TMP',
  'LANG',
  'LC_ALL',
  'LC_CTYPE',
  'SystemRoot',
  'ComSpec',
  'PATHEXT',
]);

const PATH_DENY_SEGMENTS = ['rustup', '.cargo/bin', 'node-gyp'];

export function repoRootFromMeta(metaUrl: string): string {
  return resolve(fileURLToPath(new URL('../../..', metaUrl)));
}

export function resolveProofHome(raw: string | undefined, repoRoot: string): string {
  if (!raw || raw.trim() === '') {
    throw new Error(
      'NEXUS_PROOF_HOME is required (absolute path to a disposable seeded home). ' +
        'The renderer cannot supply home/principal claims.',
    );
  }
  const candidate = isAbsolute(raw) ? raw : resolve(repoRoot, raw);
  if (!existsSync(candidate)) {
    throw new Error(`NEXUS_PROOF_HOME does not exist: ${candidate}`);
  }
  return realpathSync(candidate);
}

export function buildUtilityConfig(home: string): UtilityConfig {
  return {
    user_home: home,
    access: 'engine_owner',
    allow_uninitialized: false,
  };
}

export function sanitizeInheritedEnv(source: NodeJS.ProcessEnv): NodeJS.ProcessEnv {
  const next: NodeJS.ProcessEnv = {};
  for (const key of ALLOWED_ENV_KEYS) {
    const value = source[key];
    if (typeof value === 'string' && value.length > 0) {
      next[key] = value;
    }
  }
  if (typeof next.PATH === 'string') {
    const sep = process.platform === 'win32' ? ';' : ':';
    next.PATH = next.PATH.split(sep)
      .filter((segment) => !PATH_DENY_SEGMENTS.some((deny) => segment.includes(deny)))
      .join(sep);
  }
  next.NODE_ENV = 'production';
  next.ELECTRON_RUN_AS_NODE = '1';
  return next;
}

export function nativeRefreshHint(): string {
  return (
    'Refresh native compatibility artifacts before packaging: ' +
    '`pnpm --filter @42ch/nexus-native run build`, then ' +
    '`node packages/nexus-native/scripts/package.mjs --target <triple> --out <dir>` ' +
    'for the host architecture, install the packed tarballs into the workspace, ' +
    'and rebuild @42ch/nexus-native if the contract hash changed.'
  );
}

function expectedPlatformPackageName(): string {
  const { platform, arch } = process;
  if (platform === 'darwin' && arch === 'arm64') return '@42ch/nexus-native-darwin-arm64';
  if (platform === 'darwin' && arch === 'x64') return '@42ch/nexus-native-darwin-x64';
  if (platform === 'win32' && arch === 'x64') return '@42ch/nexus-native-win32-x64-msvc';
  if (platform === 'linux' && arch === 'x64') return '@42ch/nexus-native-linux-x64-gnu';
  throw new Error(`unsupported platform ${platform}/${arch}`);
}

/** Filesystem-only native payload probe — never loads the `.node` binding in main. */
export function assertNativePayloadPresent(): void {
  const pkgName = expectedPlatformPackageName();
  let pkgRoot: string;
  try {
    pkgRoot = dirname(require.resolve(`${pkgName}/package.json`));
  } catch {
    throw new Error(
      `platform native package ${pkgName} is not installed. ${nativeRefreshHint()}`,
    );
  }
  const nodePath = join(pkgRoot, 'native', 'nexus_core_node.node');
  if (!existsSync(nodePath)) {
    throw new Error(`missing native artifact ${nodePath}. ${nativeRefreshHint()}`);
  }
  const compatPath = join(dirname(nodePath), 'compatibility.json');
  if (!existsSync(compatPath)) {
    throw new Error(`missing compatibility manifest ${compatPath}. ${nativeRefreshHint()}`);
  }
  const manifest = JSON.parse(readFileSync(compatPath, 'utf8')) as Record<string, unknown>;
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
    if (manifest[field] === undefined) {
      throw new Error(`compatibility manifest missing "${field}" at ${compatPath}`);
    }
  }
  realpathSync(nodePath);
}
