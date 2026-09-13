#!/usr/bin/env node
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, realpathSync, writeFileSync } from 'node:fs';
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

function resolveAdmittedPython() {
  const candidates = [process.env.PYTHON, process.env.PYTHON3, 'python3'].filter(Boolean);
  for (const candidate of candidates) {
    try {
      const resolved = execFileSync('which', [candidate], { encoding: 'utf8' }).trim();
      if (resolved.startsWith('/')) {
        return realpathSync(resolved);
      }
    } catch {
      // try next candidate
    }
  }
  throw new Error('no_absolute_python_executable');
}

function writeAgentHostConfig(home, fixturePath, workspace) {
  const python = resolveAdmittedPython();
  const toml = `[[providers]]
id = "mock-acp"
protocol = "acp"
command = "${python}"
args = ["${fixturePath.replaceAll('\\', '/')}"]
enabled = true

[providers.env]
ACP_FIXTURE_LOG = "${join(workspace, 'fixture.log').replaceAll('\\', '/')}"
`;
  const configDir = join(home, 'config');
  mkdirSync(configDir, { recursive: true });
  writeFileSync(join(configDir, 'agent-host.toml'), toml);
}

function hostOwner(home) {
  return {
    creator_id: 'proof-provider',
    workspace_root: home,
    orchestration_run_id: null,
  };
}

function rustProbePayload(home) {
  return {
    provider_id: 'mock-acp',
    timeout_ms: 30_000,
    cwd: home,
    owner: hostOwner(home),
  };
}

function rustLaunchPayload(home) {
  return {
    provider_id: 'mock-acp',
    cwd: home,
    mcp_servers: [],
    owner: hostOwner(home),
  };
}

function rustExecutePayload() {
  return {
    Prompt: {
      op_id: randomUUID(),
      content: [{ Text: { text: 'hello' } }],
      permission_scope: null,
    },
  };
}

function tsProbePayload() {
  return { provider_id: 'mock-acp' };
}

function tsLaunchPayload() {
  return { provider_id: 'mock-acp' };
}

function tsExecutePayload() {
  return { kind: 'prompt', content: 'hello' };
}

async function runAcpLifecycleSession(core, { adapter, sdk, admittedMeta = {}, home }) {
  const probePayload = adapter === 'rust-acp' ? rustProbePayload(home) : tsProbePayload();
  const launchPayload = adapter === 'rust-acp' ? rustLaunchPayload(home) : tsLaunchPayload();
  const executePayload = adapter === 'rust-acp' ? rustExecutePayload() : tsExecutePayload();

  const probeReply = decode(
    await core.providerCall(
      encode({
        request_id: 'probe',
        method: 'probe',
        deadline_ms: 30_000,
        payload: probePayload,
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
        payload: launchPayload,
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
        payload: executePayload,
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

  let shutdownReply = { ok: false, error: null };
  try {
    shutdownReply = decode(
      await core.providerCall(
        encode({
          request_id: 'shutdown',
          method: 'shutdown',
          session_id: sessionId,
          deadline_ms: 30_000,
          payload: {},
        }),
      ),
    );
  } catch (error) {
    shutdownReply = { ok: false, error: String(error) };
  }

  const closeStarted = Date.now();
  const closeReport = decode(await core.close());
  const closeMs = Date.now() - closeStarted;
  if (closeMs > 5_500) {
    console.error('close exceeded 5s budget', closeMs);
    process.exit(1);
  }
  if (!closeReport.cleanup_confirmed || closeReport.state !== 'closed') {
    console.error('close did not confirm cleanup', closeReport);
    process.exit(1);
  }

  const evidence = {
    adapter,
    case: caseName,
    probe_latency_ms: probeReply.health?.latency_ms ?? null,
    session_id: sessionId,
    operation_id: operationId,
    saw_delta: sawDelta,
    terminal,
    session_shutdown_ok: shutdownReply.ok,
    session_shutdown_error: shutdownReply.error ?? null,
    close_ms: closeMs,
    close_state: closeReport.state,
    cleanup_confirmed: closeReport.cleanup_confirmed,
    pending_operations: closeReport.pending_operations ?? [],
    sdk,
    ...admittedMeta,
  };
  mkdirSync(outDir, { recursive: true });
  writeFileSync(join(outDir, 'lifecycle.json'), JSON.stringify(evidence, null, 2));
}

async function runRustAcpLifecycleProof() {
  const fixture = resolve(root, 'crates/nexus-agent-host/tests/fixtures/mock_acp_workflow.py');
  const workspace = mkdtempSync(join(tmpdir(), 'nexus-acp-ws-'));
  writeAgentHostConfig(home, fixture, workspace);
  const core = binding.open(
    JSON.stringify({ user_home: home, access: 'engine_owner', allow_uninitialized: false }),
  );
  await runAcpLifecycleSession(core, {
    adapter: 'rust-acp',
    home,
    sdk: 'agent-client-protocol=2.1.0',
    admittedMeta: {
      fixture,
      localset_bridge: 'nexus-acp-host',
      rust_admission_boundary: 'host_catalog_only',
    },
  });
}


async function runTsAcpLifecycleProof() {
  const build = spawnSync('pnpm', ['--filter', '@42ch/nexus-provider-acp', 'build'], {
    cwd: root,
    stdio: 'inherit',
  });
  if (build.status !== 0) process.exit(build.status ?? 1);

  const { createAcpProvider } = await import('../../nexus-provider-acp/dist/index.js');
  const fixture = resolve(root, 'crates/nexus-agent-host/tests/fixtures/mock_acp_workflow.py');
  const workspace = mkdtempSync(join(tmpdir(), 'nexus-acp-ws-'));
  writeAgentHostConfig(home, fixture, workspace);

  let capturedAdmittedRecipe = null;
  const providers = createAcpProvider();
  const core = binding.open(
    JSON.stringify({ user_home: home, access: 'engine_owner', allow_uninitialized: false }),
    {
      call: async (...args) => {
        const req = unpackCallbackPayload(...args);
        if (req.payload?.recipe) capturedAdmittedRecipe = req.payload.recipe;
        return JSON.stringify(await providers.call(req));
      },
      next: async (...args) => {
        const req = unpackCallbackPayload(...args);
        return JSON.stringify(
          await providers.next(req.operation_id, req.max_events, req.max_bytes),
        );
      },
    },
  );

  const fakeRecipe = {
    provider_id: 'mock-acp',
    recipe_generation: '999999',
    executable: '/tmp/evil',
    args: [],
    env: {},
    cwd: '/tmp',
  };
  const fakeProbeErr = await core
    .providerCall(
      encode({
        request_id: 'fake-recipe',
        method: 'probe',
        deadline_ms: 30_000,
        payload: { provider_id: 'mock-acp', recipe: fakeRecipe },
      }),
    )
    .then(() => null)
    .catch((error) => String(error));
  if (!fakeProbeErr || !/recipe rejected|invalid_input|policy/i.test(fakeProbeErr)) {
    console.error('expected rejection for caller-supplied recipe', fakeProbeErr);
    process.exit(1);
  }

  const admitProbeReply = decode(
    await core.providerCall(
      encode({
        request_id: 'admit-probe',
        method: 'probe',
        deadline_ms: 30_000,
        payload: tsProbePayload(),
      }),
    ),
  );
  if (!admitProbeReply.ok) {
    console.error('admitting probe failed', admitProbeReply);
    process.exit(1);
  }

  if (!capturedAdmittedRecipe) {
    console.error('callback never received Rust-admitted recipe');
    process.exit(1);
  }
  if (capturedAdmittedRecipe.recipe_generation === '999999') {
    console.error('callback used caller fake recipe generation');
    process.exit(1);
  }
  if (!capturedAdmittedRecipe.executable?.startsWith('/')) {
    console.error('admitted executable not canonical absolute', capturedAdmittedRecipe);
    process.exit(1);
  }

  await runAcpLifecycleSession(core, {
    adapter: 'ts-acp',
    home,
    sdk: '@agentclientprotocol/sdk@1.4.0',
    admittedMeta: {
      fixture,
      rust_admission_boundary: 'exercised_via_admitting_provider_port',
      admitted_provider_id: capturedAdmittedRecipe.provider_id,
      admitted_generation: capturedAdmittedRecipe.recipe_generation,
      admitted_executable: capturedAdmittedRecipe.executable,
      admitted_env_keys: Object.keys(capturedAdmittedRecipe.env ?? {}).sort(),
    },
  });
}

if (adapter === 'rust-acp' && caseName === 'lifecycle') {
  await runRustAcpLifecycleProof();
  console.log('proof-provider rust-acp lifecycle passed');
  process.exit(0);
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
