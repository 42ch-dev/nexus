#!/usr/bin/env node
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { nativeCompatibility, openCore } from '@42ch/nexus-native';

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const home = mkdtempSync(join(tmpdir(), 'nexus-service-smoke-'));
const seed = spawnSync(
  'cargo',
  ['run', '-q', '-p', 'nexus-core-node', '--bin', 'native-wire-fixture-seed', '--', home],
  { cwd: root, stdio: 'inherit' },
);
if (seed.status !== 0) process.exit(seed.status ?? 1);

const compat = nativeCompatibility();
const core = openCore({ user_home: home, access: 'engine_owner', allow_uninitialized: false });
const principal = await core.activePrincipal();
const graph = await core.worldKbGraph(principal, 'wld_owned', false);
console.log(JSON.stringify({ compat: compat.contract_tree_sha256, entities: graph.entities?.length ?? 0 }));
await core.close();
