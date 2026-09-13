#!/usr/bin/env node
/**
 * P3-T2 — package the private Electron feasibility shell with pinned Electron 44.3.0 /
 * @electron/packager 20.3.0. Includes unchanged apps/web/dist and the current compatible
 * native payload only. Never triggers Tauri or Rust builds.
 */
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  cpSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import packager from '@electron/packager';

const __dirname = dirname(fileURLToPath(import.meta.url));
const appRoot = join(__dirname, '..');
const repoRoot = resolve(appRoot, '..', '..');
const webDist = join(repoRoot, 'apps', 'web', 'dist');
const electronVersion = '44.3.0';
const bundleId = 'com.nexus42.rft-electron-proof';
const productName = 'Nexus RFT Feasibility';

function sha256File(path) {
  return createHash('sha256').update(readFileSync(path)).digest('hex');
}

function parseArgs(argv) {
  const out = { arch: null, signIdentity: process.env.APPLE_SIGNING_IDENTITY ?? null };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--arch') out.arch = argv[++i];
    else if (arg === '--sign-identity') out.signIdentity = argv[++i];
    else if (arg === '--out') out.out = argv[++i];
    else if (arg === '--help' || arg === '-h') out.help = true;
  }
  return out;
}

function usage() {
  console.error(`Usage: node scripts/package.mjs --arch arm64|x64 [--out <dir>] [--sign-identity <id>]

Signing identity precedence: --sign-identity > APPLE_SIGNING_IDENTITY > unsigned (development only).

Prerequisites:
  pnpm --filter web build
  pnpm --filter @42ch/nexus-native run build
  pnpm --filter nexus-desktop-electron-proof run build
  Native platform payload installed for host arch (see packages/nexus-native/scripts/package.mjs).`);
}

function assertHostArch(arch) {
  const host = process.arch;
  if (arch === 'arm64' && host !== 'arm64') {
    throw new Error(`refusing to package darwin/arm64 on ${host}; use a compatible runner`);
  }
  if (arch === 'x64' && host !== 'x64') {
    throw new Error(`refusing to package darwin/x64 on ${host}; use a compatible runner`);
  }
  if (process.platform !== 'darwin') {
    throw new Error('Electron GUI proof packaging is macOS-only in P3 (no Windows/Linux GUI claim)');
  }
}

function assertArtifacts() {
  const index = join(webDist, 'index.html');
  if (!existsSync(index)) {
    throw new Error(`missing ${index}. Build unchanged web dist first: pnpm --filter web build`);
  }
  const mainJs = join(appRoot, 'dist', 'main.js');
  if (!existsSync(mainJs)) {
    throw new Error(`missing ${mainJs}. Run: pnpm --filter nexus-desktop-electron-proof run build`);
  }
  const probe = spawnSync(
    process.execPath,
    ['-e', "import('@42ch/nexus-native').then(m=>m.nativeCompatibility())"],
    { cwd: appRoot, encoding: 'utf8' },
  );
  if (probe.status !== 0) {
    throw new Error(
      `native compatibility check failed: ${probe.stderr || probe.stdout}\n` +
        'Refresh P2/P3 native artifacts: node packages/nexus-native/scripts/package.mjs --target <triple> --out <dir> ' +
        'then install packed tarballs before packaging Electron.',
    );
  }
}

function readdirSafe(dir) {
  try {
    return readdirSync(dir);
  } catch {
    return [];
  }
}

function findNativeNodeFiles(root) {
  const hits = [];
  const queue = [root];
  while (queue.length) {
    const dir = queue.pop();
    for (const entry of readdirSafe(dir)) {
      const full = join(dir, entry);
      const st = statSync(full);
      if (st.isDirectory()) queue.push(full);
      else if (entry.endsWith('.node')) hits.push(full);
    }
  }
  return hits;
}

function stageApp(stagingDir) {
  rmSync(stagingDir, { recursive: true, force: true });
  mkdirSync(stagingDir, { recursive: true });

  const pkg = JSON.parse(readFileSync(join(appRoot, 'package.json'), 'utf8'));
  const stagedPkg = {
    name: pkg.name,
    version: pkg.version,
    private: true,
    type: 'module',
    main: 'dist/main.js',
    description: pkg.description,
    dependencies: {
      '@42ch/nexus-contracts': pkg.dependencies['@42ch/nexus-contracts'],
      '@42ch/nexus-native': pkg.dependencies['@42ch/nexus-native'],
      '@42ch/nexus-provider-acp': pkg.dependencies['@42ch/nexus-provider-acp'],
    },
  };
  writeFileSync(join(stagingDir, 'package.json'), `${JSON.stringify(stagedPkg, null, 2)}\n`);

  cpSync(join(appRoot, 'dist'), join(stagingDir, 'dist'), { recursive: true });
  cpSync(webDist, join(stagingDir, 'web-dist'), { recursive: true });

  const install = spawnSync('pnpm', ['install', '--prod', '--ignore-scripts'], {
    cwd: stagingDir,
    stdio: 'inherit',
    env: { ...process.env, npm_config_engine_strict: 'false' },
  });
  if (install.status !== 0) {
    throw new Error(
      'staging install failed — PM may need to serialize lockfile refresh for workspace deps',
    );
  }

  const nativeNodes = findNativeNodeFiles(join(stagingDir, 'node_modules'));
  if (nativeNodes.length === 0) {
    throw new Error(
      'no .node payload found in staged node_modules. Install the platform native package for this host, then retry.',
    );
  }
  return nativeNodes;
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.help || !args.arch) {
    usage();
    process.exit(args.help ? 0 : 1);
  }
  if (args.arch !== 'arm64' && args.arch !== 'x64') {
    throw new Error('--arch must be arm64 or x64');
  }
  assertHostArch(args.arch);
  assertArtifacts();

  const outDir = resolve(
    args.out ?? join(repoRoot, '.mstar/iterations/v1.189/guides/evidence/electron-packages', args.arch),
  );
  mkdirSync(outDir, { recursive: true });
  const stagingDir = join(outDir, '.staging-app');
  const nativeNodes = stageApp(stagingDir);

  const options = {
    dir: stagingDir,
    out: outDir,
    platform: 'darwin',
    arch: args.arch,
    electronVersion,
    appBundleId: bundleId,
    name: productName,
    overwrite: true,
    prune: true,
    asar: {
      unpack: '{**/*.node,**/utility-host.js}',
    },
    extraResource: [join(stagingDir, 'web-dist')],
    osxSign: args.signIdentity
      ? {
          identity: args.signIdentity,
          entitlements,
          'entitlements-inherit': entitlementsChild,
          hardenedRuntime: true,
        }
      : undefined,
  };

  const artifacts = await packager(options);
  if (!artifacts || artifacts.length === 0) {
    throw new Error('packager returned no artifacts');
  }

  const receipt = {
    status: 'pass',
    electron_version: electronVersion,
    packager_version: '20.3.0',
    bundle_id: bundleId,
    product_name: productName,
    arch: args.arch,
    signed: Boolean(args.signIdentity),
    signing_identity: args.signIdentity ? 'redacted' : null,
    output_paths: artifacts,
    web_dist_index_sha256: sha256File(join(webDist, 'index.html')),
    native_node_paths: nativeNodes.map((p) => p.replace(stagingDir, '<staging>')),
    publish: 'not authorized — local proof packaging only',
    note: args.signIdentity
      ? 'Signed with explicit identity input.'
      : 'Unsigned development package — does not satisfy SEC-1 (P3-T3 requires --signed-required).',
  };
  writeFileSync(join(outDir, 'package-receipt.json'), `${JSON.stringify(receipt, null, 2)}\n`);
  console.log(JSON.stringify(receipt, null, 2));
}

main().catch((err) => {
  console.error(err instanceof Error ? err.message : String(err));
  process.exit(1);
});
