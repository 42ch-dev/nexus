import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { before, describe, test } from 'node:test';

const serviceRoot = join(dirname(fileURLToPath(import.meta.url)), '..');
const repoRoot = join(serviceRoot, '..', '..');
const CONTRACT_ROOT = join(
  repoRoot,
  'packages',
  'nexus-contracts',
  'src',
  'generated',
  'daemon-api',
);

/**
 * CAP-009 / I-000133 — contract drift guard.
 *
 * Enumerates the service route table and diffs the JSON payload each covered
 * handler actually produces against the key-set of the generated contract type
 * that names it. This is the guard that would have caught the scan
 * `{ entries }` vs `{ agents }` drift: the assertion is mechanical (payload keys
 * ⊆ contract keys, required contract keys ⊆ payload keys), never a snapshot.
 *
 * Coverage policy: tier1 + host-family routes and daemon status. Every other
 * route in those families is listed in `TODO_ROUTES` explicitly, so adding a
 * route without classifying it fails the enumeration test. The remaining
 * families (worlds/works/content/knowledge/actors/workspaces/memory/context/
 * presets/compute/workflow_observation/world_kb) are TODO for a later pass.
 */

const SESSION = {
  session_id: '9f9619ff-8b86-4d11-b42d-00cf4fc964ff',
  provider_id: 'mock-acp',
  state: 'Ready',
};

const SCAN_ENTRY = {
  name: 'Claude Code',
  registry_agent_id: 'claude-acp',
  launch_command: 'claude-agent-acp',
  installed: true,
  version: '1.0.0',
};

const CAPABILITY = {
  name: 'world.query',
  inputSchema: '{"type":"object"}',
  outputSchema: '{"type":"object"}',
  origin: 'builtin',
};

/**
 * Top-level property names of one generated `export interface`, with optional
 * flags. Nested object literals are tracked by brace depth so only the
 * interface's own fields are collected.
 */
function interfaceFields(source, name) {
  const start = source.indexOf(`export interface ${name} `);
  assert.ok(start >= 0, `interface ${name} not found`);
  const lines = source.slice(start).split('\n');
  const fields = new Map();
  let depth = 0;
  let started = false;
  for (const line of lines) {
    const trimmed = line.trim();
    if (!started) {
      if (!trimmed.includes('{')) continue;
      started = true;
      depth = (line.match(/{/g) ?? []).length - (line.match(/}/g) ?? []).length;
      continue;
    }
    if (trimmed.startsWith('*') || trimmed.startsWith('/*') || trimmed.startsWith('//')) continue;
    if (depth === 1) {
      const match = /^([A-Za-z_][A-Za-z0-9_]*)(\?)?:/.exec(trimmed);
      if (match) fields.set(match[1], match[2] === '?');
    }
    depth += (line.match(/{/g) ?? []).length - (line.match(/}/g) ?? []).length;
    if (depth <= 0) break;
  }
  assert.ok(fields.size > 0, `interface ${name} has no fields`);
  return fields;
}

function contractFields(file, name) {
  return interfaceFields(readFileSync(join(CONTRACT_ROOT, file), 'utf8'), name);
}

/** Payload keys must be declared; every required contract key must be present. */
function assertMatchesContract(label, payload, fields) {
  assert.equal(typeof payload, 'object', `${label}: payload is not an object`);
  const payloadKeys = Object.keys(payload);
  const undeclared = payloadKeys.filter((key) => !fields.has(key));
  const missing = [...fields.entries()]
    .filter(([, optional]) => !optional)
    .map(([key]) => key)
    .filter((key) => !(key in payload));
  assert.deepEqual(undeclared, [], `${label}: payload keys not in contract`);
  assert.deepEqual(missing, [], `${label}: required contract keys missing`);
}

let routes;

before(async () => {
  const routesDist = join(serviceRoot, 'dist', 'routes.js');
  assert.ok(
    existsSync(routesDist),
    'dist/routes.js is missing — run `pnpm --filter @42ch/nexus-service run test` (or `… run build`) first',
  );
  routes = await import(join(serviceRoot, 'dist', 'routes.js'));
});

function routeFor(method, pathname) {
  const route = routes.DOMAIN_ROUTES.find(
    (candidate) => candidate.method === method && candidate.pattern.test(pathname),
  );
  assert.ok(route, `${method} ${pathname} is not mounted`);
  return route;
}

const COVERED = [
  {
    method: 'POST',
    path: '/v1/daemon/agent-host/scan',
    contract: { file: 'agent-host/scan-response.ts', name: 'ScanResponse' },
    service: () => ({ core: { hostQuery: async () => ({ scan: { entries: [SCAN_ENTRY] } }) } }),
    items: (body) => body.agents,
    itemContract: { file: 'agent-host/scan-response.ts', name: 'NexusAgentScanEntry' },
  },
  {
    method: 'GET',
    path: '/v1/daemon/agent-host/sessions',
    contract: { file: 'agent-host/session-list-response.ts', name: 'SessionListResponse' },
    service: () => ({
      core: {
        hostQuery: async () => ({
          sessions: { items: [SESSION], pagination: { limit: 50, has_more: false } },
        }),
      },
    }),
    items: (body) => body.items,
    itemContract: {
      file: 'agent-host/session-list-response.ts',
      name: 'NexusAgentHostSessionResponse',
    },
  },
  {
    method: 'GET',
    path: `/v1/daemon/agent-host/sessions/${SESSION.session_id}`,
    contract: { file: 'agent-host/session-list-response.ts', name: 'NexusAgentHostSessionResponse' },
    service: () => ({ core: { hostQuery: async () => ({ session: SESSION }) } }),
  },
  {
    method: 'GET',
    path: '/v1/daemon/orchestration/capabilities',
    // The capabilities endpoint serves the hand-coded local DTO (camelCase),
    // not the schema-facing generated `ListCapabilitiesResponse` (snake_case).
    localContract: {
      source: join(repoRoot, 'apps', 'web', 'src', 'lib', 'nexus', 'capability-info.ts'),
      name: 'CapabilityListResponse',
      itemName: 'CapabilityInfo',
    },
    service: () => ({ core: { listCapabilities: async () => ({ items: [CAPABILITY] }) } }),
    items: (body) => body.items,
  },
];

const TODO_ROUTES = new Set([
  // execution family, not yet mechanically bound to a contract
  'POST ^\\/v1\\/daemon\\/orchestration\\/schedules$',
  'GET ^\\/v1\\/daemon\\/orchestration\\/schedules$',
  'GET ^\\/v1\\/daemon\\/orchestration\\/schedules\\/([^/]+)$',
  'PATCH ^\\/v1\\/daemon\\/orchestration\\/schedules\\/([^/]+)\\/core-context$',
  'POST ^\\/v1\\/daemon\\/orchestration\\/schedules\\/([^/]+)\\/signal$',
  'GET ^\\/v1\\/daemon\\/orchestration\\/sessions$',
  'GET ^\\/v1\\/daemon\\/orchestration\\/sessions\\/([^/]+)$',
  // host family, not yet mechanically bound to a contract
  'GET ^\\/v1\\/daemon\\/agent-host\\/health$',
  'GET ^\\/v1\\/daemon\\/agent-host\\/providers$',
  'GET ^\\/v1\\/daemon\\/agent-host\\/operations\\/([^/]+)$',
  'POST ^\\/v1\\/daemon\\/agent-host\\/sessions$',
  'DELETE ^\\/v1\\/daemon\\/agent-host\\/sessions\\/([^/]+)$',
  'POST ^\\/v1\\/daemon\\/agent-host\\/sessions\\/([^/]+)\\/operations$',
  'POST ^\\/v1\\/daemon\\/agent-host\\/operations\\/([^/]+)$',
  'GET ^\\/v1\\/daemon\\/agent-host\\/sessions\\/([^/]+)\\/events$',
  'GET ^\\/v1\\/daemon\\/agent-host\\/profile$',
  'PUT ^\\/v1\\/daemon\\/agent-host\\/profile$',
  'GET ^\\/v1\\/daemon\\/agent-host\\/config$',
  'PUT ^\\/v1\\/daemon\\/agent-host\\/config$',
  // runtime family
  'GET ^\\/v1\\/daemon\\/runtime\\/health$',
  'GET ^\\/v1\\/daemon\\/runtime\\/status$',
  'GET ^\\/v1\\/daemon\\/runtime\\/cert-fingerprint$',
  'GET ^\\/v1\\/daemon\\/daemon\\/status$',
]);

const FAMILIES = new Set(['host', 'execution', 'runtime']);

describe('service response shape vs generated contracts (CAP-009 / I-000133)', () => {
  test('every host/execution/runtime route is covered or explicitly TODO', () => {
    const coveredKeys = new Set(
      COVERED.map(({ method, path }) => {
        const route = routeFor(method, path);
        return `${method} ${route.pattern.source}`;
      }),
    );
    const unclassified = routes.DOMAIN_ROUTES.filter((route) => FAMILIES.has(route.family))
      .map((route) => `${route.method} ${route.pattern.source}`)
      .filter((key) => !coveredKeys.has(key) && !TODO_ROUTES.has(key));
    assert.deepEqual(unclassified, [], 'unclassified routes in covered families');
  });

  for (const covered of COVERED) {
    test(`${covered.method} ${covered.path} matches its contract key-set`, async () => {
      const route = routeFor(covered.method, covered.path);
      const params = covered.path.includes(SESSION.session_id) ? [SESSION.session_id] : [];
      const result = await route.handle(covered.service(), params, new URLSearchParams());
      const body = result.body;

      if ('localContract' in covered) {
        const source = readFileSync(covered.localContract.source, 'utf8');
        assertMatchesContract(
          covered.path,
          body,
          interfaceFields(source, covered.localContract.name),
        );
        const itemFields = interfaceFields(source, covered.localContract.itemName);
        for (const item of covered.items(body)) {
          assertMatchesContract(`${covered.path} item`, item, itemFields);
        }
        return;
      }

      assertMatchesContract(
        covered.path,
        body,
        contractFields(covered.contract.file, covered.contract.name),
      );
      if (covered.itemContract && covered.items) {
        const itemFields = contractFields(
          covered.itemContract.file,
          covered.itemContract.name,
        );
        for (const item of covered.items(body)) {
          assertMatchesContract(`${covered.path} item`, item, itemFields);
        }
      }
    });
  }

  test('daemon status keeps its published key set (no generated contract yet)', async () => {
    const route = routeFor('GET', '/v1/daemon/daemon/status');
    const result = await route.handle({
      workspaceInitialized: true,
      domainOnly: false,
      providerReady: true,
      startedAt: new Date().toISOString(),
      tlsFingerprint: null,
    });
    // TODO(CAP-009): bind this to a generated schema once daemon status has one.
    assert.deepEqual(Object.keys(result.body).sort(), [
      'degraded',
      'exit_code',
      'implementation_scope',
      'last_error',
      'lifecycle_state',
      'pid',
      'schema_version',
      'started_at',
      'subsystems',
      'uptime_ms',
      'version',
    ]);
    assert.deepEqual(Object.keys(result.body.degraded).sort(), ['reasons', 'subsystems']);
  });
});
