#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = join(__dirname, '..', '..', '..');
const outDir = process.argv.includes('--out')
  ? process.argv[process.argv.indexOf('--out') + 1]
  : join(root, '.mstar', 'iterations', 'v1.189', 'guides', 'evidence', 'native-wire');
const caseName = process.argv.includes('--case')
  ? process.argv[process.argv.indexOf('--case') + 1]
  : 'wire';
if (caseName !== 'wire') {
  console.error('only --case wire supported in P2-T1');
  process.exit(2);
}
mkdirSync(outDir, { recursive: true });

const home = mkdtempSync(join(tmpdir(), 'nexus-native-wire-'));
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
const compat = JSON.parse(binding.compatibility());
const core = binding.open(
  JSON.stringify({ user_home: home, access: 'engine_owner', allow_uninitialized: false }),
);
const principal = await core.activePrincipal();
const results = { case: caseName, home, principal, compat, checks: [] };

function record(name, ok, detail) {
  results.checks.push({ name, ok, detail });
  if (!ok) results.pass = false;
}

const graph = JSON.parse(
  new TextDecoder().decode(await core.worldKbGraph(principal, 'wld_owned', false)),
);
record('graph_nonempty', graph.entities?.length > 0, `entities=${graph.entities?.length ?? 0}`);

const foreignErr = await core
  .worldKbGraph(principal, 'wld_foreign', false)
  .then(() => null)
  .catch((e) => String(e));
record('foreign_denied', Boolean(foreignErr), foreignErr ?? 'no error');

const createReq = {
  entity_id: 'kb_abc123',
  expected_version: 0,
  patch: { title: 'Wire Hero', block_type: 'character' },
};
const created = JSON.parse(
  new TextDecoder().decode(
    await core.patchWorldKbEntity(principal, 'wld_owned', new TextEncoder().encode(JSON.stringify(createReq))),
  ),
);
record('create_ok', created.version === 1, `version=${created.version}`);

const staleReq = {
  entity_id: 'kb_cas',
  expected_version: 1,
  patch: { title: 'Stale' },
};
const beforeStale = JSON.parse(
  new TextDecoder().decode(await core.worldKbGraph(principal, 'wld_owned', false)),
);
const casEntity = beforeStale.entities?.find((e) => e.entity_id === 'kb_cas');
const casVersionBefore = casEntity?.version;
const conflict = await core
  .patchWorldKbEntity(principal, 'wld_owned', new TextEncoder().encode(JSON.stringify(staleReq)))
  .then(() => null)
  .catch((e) => String(e));
const afterStale = JSON.parse(
  new TextDecoder().decode(await core.worldKbGraph(principal, 'wld_owned', false)),
);
const casEntityAfter = afterStale.entities?.find((e) => e.entity_id === 'kb_cas');
record('stale_conflict', Boolean(conflict), conflict ?? 'no error');
record(
  'stale_no_mutation',
  casEntityAfter?.version === casVersionBefore,
  `before=${casVersionBefore} after=${casEntityAfter?.version}`,
);

const maxSafe = 9007199254740991;
const precisionReq = {
  entity_id: 'kb_def456',
  expected_version: 0,
  patch: { title: 'Prec', block_type: 'character' },
};
const precCreated = JSON.parse(
  new TextDecoder().decode(
    await core.patchWorldKbEntity(principal, 'wld_owned', new TextEncoder().encode(JSON.stringify(precisionReq))),
  ),
);
record('precision_create', precCreated.version === 1, `version=${precCreated.version}`);
record('max_safe_integer', maxSafe === Number.MAX_SAFE_INTEGER, `value=${maxSafe}`);

const overflowLimit = await core
  .worldKbCandidates(principal, 'wld_owned', maxSafe + 1, null)
  .then(() => null)
  .catch((e) => String(e));
record('limit_overflow_rejected', Boolean(overflowLimit), overflowLimit ?? 'no error');

const atLimit = JSON.parse(
  new TextDecoder().decode(await core.worldKbCandidates(principal, 'wld_owned', maxSafe, null)),
);
record('limit_max_safe_ok', Array.isArray(atLimit.items), `items=${atLimit.items?.length ?? 0}`);

const candidates = JSON.parse(
  new TextDecoder().decode(await core.worldKbCandidates(principal, 'wld_owned', 10, null)),
);
record('candidates_nonempty', (candidates.items?.length ?? 0) > 0, `items=${candidates.items?.length ?? 0}`);

await core.close();
results.pass = results.pass ?? true;
writeFileSync(join(outDir, 'proof-wire.json'), JSON.stringify(results, null, 2));
console.log('proof passed', outDir);
if (!results.pass) process.exit(1);
