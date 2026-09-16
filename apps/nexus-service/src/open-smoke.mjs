#!/usr/bin/env node
// P2-T1 binding smoke: opens the native core against a seeded fixture home.
// Not a product entrypoint — see package.json `dev`/`start` for the
// downstream-owned `src/main.mjs`.
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

const _compat = nativeCompatibility();
const core = await openCore({ user_home: home, access: 'engine_owner', allow_uninitialized: false });
const principal = await core.activePrincipal();
const _graph = await core.worldKbGraph(principal, 'wld_owned', false);

await core.close();
