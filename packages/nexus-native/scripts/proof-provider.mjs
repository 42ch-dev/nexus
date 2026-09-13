#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
import { randomUUID } from 'node:crypto';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = join(__dirname, '..', '..', '..');

const adapter = process.argv.includes('--adapter')
  ? process.argv[process.argv.indexOf('--adapter') + 1]
  : 'wire';
const caseName = process.argv.includes('--case')
  ? process.argv[process.argv.indexOf('--case') + 1]
  : 'wire';
const outDir = process.argv.includes('--out')
  ? process.argv[process.argv.indexOf('--out') + 1]
  : join(root, '.mstar', 'iterations', 'v1.189', 'guides', 'evidence', 'native-wire');

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

const encode = (value) => new TextEncoder().encode(JSON.stringify(value));
const decode = (buffer) => JSON.parse(new TextDecoder().decode(buffer));

async function runWireProof(core) {
  const query = (request) => core.hostQuery(encode(request)).then(decode);
  const health = await query({ query: 'health' });
  if (health.health?.running !== true) {
    console.error('host health not running', health);
    process.exit(1);
  }
  const catalog = await query({ query: 'catalog' });
  for (const provider of catalog.catalog?.providers ?? []) {
    if (provider.protocol_kind !== 'acp' && provider.protocol_kind !== 'native_cli') {
      console.error('protocol_kind is not contract snake_case', provider);
      process.exit(1);
    }
  }
  const listed = await query({ query: 'list_sessions' });
  const ids = (listed.sessions?.items ?? []).map((item) => item.session_id);
  for (let index = 1; index < ids.length; index += 1) {
    if (ids[index - 1] > ids[index]) {
      console.error('session snapshot is not sorted', ids);
      process.exit(1);
    }
  }
  const probeErr = await core
    .providerCall(encode({ request_id: 'probe-1', method: 'probe', deadline_ms: 30_000, payload: { provider_id: 'missing-provider' } }))
    .then(() => null)
    .catch((error) => String(error));
  if (!probeErr) {
    console.error('expected not-found for missing provider');
    process.exit(1);
  }
}
function unpackCallbackPayload(...args) {
  const payload = args.length > 1 ? args[1] : args[0];
  return typeof payload === 'string' ? JSON.parse(payload) : payload;
}

async function runTsAcpLifecycleProof() {
  const build = spawnSync('pnpm', ['--filter', '@42ch/nexus-provider-acp', 'build'], {
    cwd: root,
    stdio: 'inherit',
  });
  if (build.status !== 0) process.exit(build.status ?? 1);

  const { createAcpProvider } = await import('../../nexus-provider-acp/dist/index.js');
  const providers = createAcpProvider();
  const core = binding.open(
    JSON.stringify({ user_home: home, access: 'engine_owner', allow_uninitialized: false }),
    {
      call: async (...args) =>
        JSON.stringify(await providers.call(unpackCallbackPayload(...args))),
      next: async (...args) => {
        const req = unpackCallbackPayload(...args);
        return JSON.stringify(await providers.next(req.operation_id, req.max_events, req.max_bytes));
      },
    },
  );

  const fixture = resolve(root, 'crates/nexus-agent-host/tests/fixtures/mock_acp_workflow.py');
  const workspace = mkdtempSync(join(tmpdir(), 'nexus-acp-ws-'));
  const recipe = {
    provider_id: 'mock-acp',
    recipe_generation: '1',
    executable: process.env.PYTHON ?? 'python3',
    args: [fixture],
    env: {},
    cwd: workspace,
  };

  const probeReply = decode(
    await core.providerCall(
      encode({
        request_id: 'probe',
        method: 'probe',
        deadline_ms: 30_000,
        payload: { provider_id: 'mock-acp', recipe },
      }),
    ),
  );
  if (!probeReply.ok || !probeReply.health?.available) {
    console.error('probe failed', probeReply);
    process.exit(1);
  }

  const launchReply = decode(
    await core.providerCall(
      encode({
        request_id: 'launch',
        method: 'launch',
        deadline_ms: 30_000,
        payload: { provider_id: 'mock-acp', cwd: workspace, recipe },
      }),
    ),
  );
  const sessionId = launchReply.session_id;
  if (!launchReply.ok || !sessionId) {
    console.error('launch failed', launchReply);
    process.exit(1);
  }

  const executeReply = decode(
    await core.providerCall(
      encode({
        request_id: 'execute',
        method: 'execute',
        session_id: sessionId,
        deadline_ms: 30_000,
        payload: { kind: 'prompt', content: 'hello' },
      }),
    ),
  );
  const operationId = executeReply.operation_id;
  if (!executeReply.ok || !operationId) {
    console.error('execute failed', executeReply);
    process.exit(1);
  }

  let sawDelta = false;
  let terminal = false;
  for (let i = 0; i < 20 && !terminal; i += 1) {
    const batch = decode(await core.nextProviderEvents(operationId, 16, 256 * 1024));
    for (const event of batch.events ?? []) {
      if (event.MessageDelta) sawDelta = true;
      if (event.OpFinished || event.OpFailed) terminal = true;
    }
    if (!batch.has_more && terminal) break;
    await new Promise((r) => setTimeout(r, 50));
  }
  if (!sawDelta || !terminal) {
    console.error('missing prompt stream evidence', { sawDelta, terminal });
    process.exit(1);
  }

  const shutdownReply = decode(
    await core.providerCall(
      encode({ request_id: 'shutdown', method: 'shutdown', session_id: sessionId, deadline_ms: 30_000, payload: {} }),
    ),
  );
  if (!shutdownReply.ok) {
    console.error('shutdown failed', shutdownReply);
    process.exit(1);
  }

  const evidence = {
    adapter: 'ts-acp',
    case: caseName,
    probe_latency_ms: probeReply.health?.latency_ms ?? null,
    session_id: sessionId,
    operation_id: operationId,
    saw_delta: sawDelta,
    terminal,
    fixture,
    sdk: '@agentclientprotocol/sdk@1.4.0',
  };
  mkdirSync(outDir, { recursive: true });
  writeFileSync(join(outDir, 'lifecycle.json'), JSON.stringify(evidence, null, 2));
  await core.close();
}

if (adapter === 'ts-acp' && caseName === 'lifecycle') {
  await runTsAcpLifecycleProof();
  console.log('proof-provider ts-acp lifecycle passed');
  process.exit(0);
}

if (caseName !== 'wire' || adapter !== 'wire') {
  console.error(`unsupported proof profile adapter=${adapter} case=${caseName}`);
  process.exit(2);
}

const core = binding.open(
  JSON.stringify({ user_home: home, access: 'engine_owner', allow_uninitialized: false }),
);
await runWireProof(core);
await core.close();
console.log('proof-provider passed');
