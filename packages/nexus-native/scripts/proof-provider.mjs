#!/usr/bin/env node
// P2-T1 provider/host-query proof against the real .node binding.
import { spawnSync } from 'node:child_process';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
import { randomUUID } from 'node:crypto';

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

const encode = (value) => new TextEncoder().encode(JSON.stringify(value));
const decode = (buffer) => JSON.parse(new TextDecoder().decode(buffer));
const query = (request) => core.hostQuery(encode(request)).then(decode);

const health = await query({ query: 'health' });
if (health.health?.running !== true) {
  console.error('host health not running', health);
  process.exit(1);
}

// Catalog protocol_kind is the contract's snake_case vocabulary.
const catalog = await query({ query: 'catalog' });
for (const provider of catalog.catalog?.providers ?? []) {
  if (provider.protocol_kind !== 'acp' && provider.protocol_kind !== 'native_cli') {
    console.error('protocol_kind is not contract snake_case', provider);
    process.exit(1);
  }
}

// Session snapshots paginate deterministically (sorted by session id).
const listed = await query({ query: 'list_sessions' });
const ids = (listed.sessions?.items ?? []).map((item) => item.session_id);
for (let index = 1; index < ids.length; index += 1) {
  if (ids[index - 1] > ids[index]) {
    console.error('session snapshot is not sorted', ids);
    process.exit(1);
  }
}
if ((listed.sessions?.pagination?.limit ?? 0) < 1) {
  console.error('pagination limit missing', listed.sessions?.pagination);
  process.exit(1);
}

// An operation the registry does not track is reported, never fabricated.
const activeOp = await query({ query: 'get_operation', operation_id: randomUUID() })
  .then(() => null)
  .catch((error) => String(error));
if (!activeOp) {
  console.error('unknown operation must not report a fabricated status');
  process.exit(1);
}
if (activeOp.includes('started')) {
  console.error('unknown operation must not report the constant "started"', activeOp);
  process.exit(1);
}

const probeReq = {
  request_id: 'probe-1',
  method: 'probe',
  deadline_ms: 30_000,
  payload: { provider_id: 'missing-provider' },
};
const probeErr = await core
  .providerCall(encode(probeReq))
  .then(() => null)
  .catch((error) => String(error));
if (!probeErr) {
  console.error('expected not-found for missing provider');
  process.exit(1);
}

await core.close();
console.log('proof-provider passed');
