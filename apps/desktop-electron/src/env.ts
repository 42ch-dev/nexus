import { existsSync, realpathSync } from 'node:fs';
import { isAbsolute, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import type { UtilityConfig } from './ipc.js';

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
