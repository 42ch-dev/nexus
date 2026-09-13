#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = join(__dirname, '..', '..', '..');

const home = mkdtempSync(join(tmpdir(), 'nexus-provider-proof-'));
const seed = spawnSync(
  'cargo',
  ['run', '-q', '-p', 'nexus-core-node', '--bin', 'native-wire-fixture-seed', '--', home],
  { cwd: root },
);
if (seed.status !== 0) {
  console.error(seed.stderr?.toString());
  process.exit(seed.status ?? 1);
}

const require = createRequire(import.meta.url);
const { loadNodePath } = await import('../dist/loader.js');
const nodePath = loadNodePath();
const binding = require(nodePath);
const core = binding.open(
  JSON.stringify({ user_home: home, access: 'engine_owner', allow_uninitialized: false }),
);
const principal = await core.activePrincipal();

const health = JSON.parse(
  new TextDecoder().decode(await core.hostQuery(new TextEncoder().encode(JSON.stringify({ query: 'health' })))),
);
if (!health.health?.running) {
  console.error('host health missing', health);
  process.exit(1);
}

const probeReq = {
  request_id: 'probe-1',
  method: 'probe',
  deadline_ms: 30_000,
  payload: { provider_id: 'missing-provider' },
};
const probeErr = await core
  .providerCall(new TextEncoder().encode(JSON.stringify(probeReq)))
  .then(() => null)
  .catch((e) => String(e));
if (!probeErr) {
  console.error('expected not-found for missing provider');
  process.exit(1);
}

await core.close();
console.log('proof-provider passed');
