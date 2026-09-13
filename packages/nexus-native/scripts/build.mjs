#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import { copyFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = join(__dirname, '..', '..', '..');
const nativeDir = join(__dirname, '..', 'native');
mkdirSync(nativeDir, { recursive: true });

const build = spawnSync('cargo', ['build', '-p', 'nexus-core-node'], {
  cwd: root,
  stdio: 'inherit',
});
if (build.status !== 0) process.exit(build.status ?? 1);

const ext = process.platform === 'win32' ? '.dll' : process.platform === 'darwin' ? '.dylib' : '.so';
const prefix = process.platform === 'win32' ? '' : 'lib';
const src = join(root, 'target', 'debug', `${prefix}nexus_core_node${ext}`);
const dest = join(nativeDir, 'nexus_core_node.node');
copyFileSync(src, dest);

const require = createRequire(import.meta.url);
const binding = require(dest);
const compat = JSON.parse(binding.compatibility());
writeFileSync(join(nativeDir, 'compatibility.json'), JSON.stringify(compat, null, 2));
console.log('built', dest);
console.log('compatibility', compat.contract_tree_sha256);
