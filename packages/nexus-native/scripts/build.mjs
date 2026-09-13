#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = join(__dirname, '..', '..', '..');
const nativeDir = join(__dirname, '..', 'native');
mkdirSync(nativeDir, { recursive: true });

function platformPackageName() {
  const { platform, arch } = process;
  if (platform === 'darwin' && arch === 'arm64') return '@42ch/nexus-native-darwin-arm64';
  if (platform === 'darwin' && arch === 'x64') return '@42ch/nexus-native-darwin-x64';
  if (platform === 'win32' && arch === 'x64') return '@42ch/nexus-native-win32-x64-msvc';
  if (platform === 'linux' && arch === 'x64') return '@42ch/nexus-native-linux-x64-gnu';
  throw new Error(`unsupported platform ${platform}/${arch}`);
}

const release = process.argv.includes('--release');
const build = spawnSync('cargo', ['build', '-p', 'nexus-core-node', ...(release ? ['--release'] : [])], {
  cwd: root,
  stdio: 'inherit',
});
if (build.status !== 0) process.exit(build.status ?? 1);

const ext = process.platform === 'win32' ? '.dll' : process.platform === 'darwin' ? '.dylib' : '.so';
const prefix = process.platform === 'win32' ? '' : 'lib';
const profile = release ? 'release' : 'debug';
const src = join(root, 'target', profile, `${prefix}nexus_core_node${ext}`);
const dest = join(nativeDir, 'nexus_core_node.node');
copyFileSync(src, dest);

const require = createRequire(import.meta.url);
let compat;
try {
  const binding = require(dest);
  compat = JSON.parse(binding.compatibility());
} catch (error) {
  const fallback = join(nativeDir, 'compatibility.json');
  if (!existsSync(fallback)) {
    console.error('failed to load native binding for compatibility manifest', error);
    process.exit(1);
  }
  compat = JSON.parse(readFileSync(fallback, 'utf8'));
}
const compatJson = JSON.stringify(compat, null, 2);
writeFileSync(join(nativeDir, 'compatibility.json'), compatJson);

const pkgName = platformPackageName();
const shortName = pkgName.replace('@42ch/', '');
const pkgRoot = join(root, 'packages', shortName);
if (!existsSync(join(pkgRoot, 'package.json'))) {
  throw new Error(`platform package manifest missing at ${pkgRoot}`);
}
const platformNativeDir = join(pkgRoot, 'native');
mkdirSync(platformNativeDir, { recursive: true });
copyFileSync(src, join(platformNativeDir, 'nexus_core_node.node'));
writeFileSync(join(platformNativeDir, 'compatibility.json'), compatJson);

console.log('built', dest);
console.log('installed', join(platformNativeDir, 'nexus_core_node.node'));
console.log('compatibility', compat.contract_tree_sha256);
