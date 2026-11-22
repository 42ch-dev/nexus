/**
 * note: A3 — the native authoring boundary.
 *
 * The real TS/native service, the repository's Rust seed and its local protocol fixture
 * carry the whole public authoring chain: generate → simulate → approve. Every observation
 * below is the persisted store's, read back through the product routes — a committed
 * revision, the hash the approval froze, and the rows a refused approval leaves untouched.
 *
 * Not claimed here: any model or provider turn. The Generate leg runs with no executor
 * attached (the domain-only shape a deterministic request needs), and the selected native
 * provider is never prompted, so the fixture is a boot-time selection only and no call
 * leaves loopback. The wire behaviour of the existing mock-route suites is unchanged and
 * this target does not restate it.
 */
import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { after, before, describe, test } from 'node:test';

const serviceRoot = join(dirname(fileURLToPath(import.meta.url)), '..');
const repoRoot = join(serviceRoot, '..', '..');

/** note: the repository's scripted SDK-wire runtime — the ONLY provider fixture this target uses. */
const dshFixture = join(
  repoRoot,
  'crates/nexus-agent-host/tests/fixtures/native_protocol/mock_dsh_agent.py',
);

const CREATOR = 'ctr_testcreator';
const SLUG = 'default';
const STRATEGY = 'native-authoring-fixture';

const generatePath = `/v1/daemon/strategies/${STRATEGY}/generate`;
const simulatePath = `/v1/daemon/strategies/${STRATEGY}/simulate`;
const approvePath = `/v1/daemon/strategies/${STRATEGY}/approve`;
const candidatePath = `/v1/daemon/strategies/${STRATEGY}/candidate`;
const approvalsPath = `/v1/daemon/strategies/${STRATEGY}/approvals`;
const appliedPath = `/v1/daemon/strategies/${STRATEGY}/applied`;

function node(id, kind) {
  return { id, kind, effect_class: 'pure' };
}

/** note: a request every lint layer accepts — one outer state wired to its terminal, one run intent. */
function legalRequest() {
  return {
    entry: 'start',
    terminal: ['done'],
    nodes: [node('start', 'outer_state'), node('done', 'terminal')],
    edges: [{ id: 'start->done', from: 'start', to: 'done', kind: 'linear' }],
    nests: [],
    meta: {
      description: 'native authoring fixture',
      kind: 'creator',
      run_intents: ['work_init'],
      requires_capabilities: [],
    },
  };
}

/** note: the user bundle the strategy id addresses — the layout the core resolves as a preset. */
function writePreset(nexusHome, id) {
  const bundle = join(nexusHome, 'presets', id);
  mkdirSync(bundle, { recursive: true });
  writeFileSync(
    join(bundle, 'preset.yaml'),
    `preset:
  id: ${id}
  version: 1
  kind: creator
  description: "native authoring fixture"
  requires_capabilities: []
  run_intents: [work_init]
  initial: start
  terminal: done
states:
  - id: start
    enter: []
    exit_when: { kind: rule }
    next: done
  - id: done
    terminal: true
`,
  );
}

/**
 * note: seed one disposable home whose directory the CALLER already allocated.
 *
 * Every step here is fallible, so the name is chosen (and therefore owned) before
 * this runs: `before` assigns the `mkdtempSync` result to the teardown field first,
 * so a seed refusal, a spawn abort or a failed write still leaves `after` holding
 * the path. This function never creates, renames or removes the root itself.
 *
 * One seeded home: the existing Rust seed (creator, workspace, durable store, worlds/KB),
 * the strategy bundle this target authors, the selected workspace's registered creative root
 * (the hosted execution owner's precondition) and the local protocol fixture as the selected
 * native provider.
 *
 * The developer's own `~/.nexus42` is never read or written.
 */
function seedHome(home) {
  // why: `--locked --offline` forbid lockfile changes and any network access; they do
  // NOT override the caller's Cargo configuration, so the prepared build
  // prerequisites are what make the local cache sufficient. The explicit timeout
  // + SIGKILL keeps a wedged cargo from blocking this synchronous call past the
  // suite's own budget, and both pipes are captured so a failure reports the
  // child's status and output.
  const seed = spawnSync(
    'cargo',
    [
      'run',
      '-q',
      '--locked',
      '--offline',
      '-p',
      'nexus-core-node',
      '--bin',
      'native-wire-fixture-seed',
      '--',
      home,
    ],
    { cwd: repoRoot, encoding: 'utf8', timeout: 180_000, killSignal: 'SIGKILL' },
  );
  if (seed.error || seed.signal) {
    throw new Error(
      `native seed aborted: error=${seed.error ? (seed.error.code ?? seed.error.message) : 'none'} signal=${seed.signal ?? 'none'}\n${seed.stdout}\n${seed.stderr}`,
    );
  }
  assert.equal(seed.status, 0, `native seed failed: ${seed.stdout}\n${seed.stderr}`);

  const nexusHome = join(home, '.nexus42');
  writePreset(nexusHome, STRATEGY);

  const creativeRoot = join(home, 'creative-root');
  mkdirSync(creativeRoot, { recursive: true });
  writeFileSync(
    join(nexusHome, 'creators', CREATOR, 'workspaces', SLUG, 'meta.json'),
    JSON.stringify({ local_root: creativeRoot }),
  );

  mkdirSync(join(nexusHome, 'agent-host'), { recursive: true });
  writeFileSync(
    join(nexusHome, 'agent-host', 'config.toml'),
    `[[providers]]
id = "dsh-native"
protocol = "native_cli"
command = ${JSON.stringify(dshFixture)}
enabled = true
`,
  );
}

describe('native authoring boundary', () => {
  let home;
  let service;

  before(async () => {
    // why: clear inherited service authentication — the service resolves
    // NEXUS42_DAEMON_API_KEY even with `allowRemote: false`, and a nonempty
    // value selects `keyed_all`. Every fetch below is keyless loopback, so a
    // developer/CI key would 401 before authoring was ever reached.
    delete process.env.NEXUS42_DAEMON_API_KEY;
    // invariant: allocate the name FIRST and hand it to the teardown field before any
    // fallible step: ownership starts when the name is chosen, so a seed refusal
    // (or an abort, or a failed write) still leaves `after` with the path to
    // remove. Nothing is pre-deleted and the name is unique, so a run killed
    // outright can only leave its own inert tree behind for no later run to touch.
    home = mkdtempSync(join(tmpdir(), 'nexus-authoring-native-'));
    seedHome(home);
    const { startService } = await import(join(serviceRoot, 'dist', 'index.js'));
    service = await startService({ home, host: '127.0.0.1', port: 0, allowRemote: false });
  });

  after(async () => {
    if (service) await service.close().catch(() => undefined);
    if (home) rmSync(home, { recursive: true, force: true });
  });

  async function call(method, path, body) {
    const response = await fetch(`${service.url}${path}`, {
      method,
      ...(body === undefined
        ? {}
        : { headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) }),
    });
    const text = await response.text();
    return { status: response.status, payload: text.length > 0 ? JSON.parse(text) : null, text };
  }

  test('generate → simulate → approve persists revision and hash, and stale evidence writes nothing', async () => {
    // why: the native boundary itself — the staged addon opened the core and the hosted owner
    // established an engine epoch. Without this the rest would be a mock-route assertion.
    // A positive epoch is the owner proof: `null` is "not projected yet" and `0` is the
    // no-owner/domain-only sentinel the service publishes, so both would pass a null check
    // while every deterministic authoring call still succeeded.
    assert.ok(
      Number.isInteger(service.service.engineEpoch) && service.service.engineEpoch > 0,
      `the native addon must report an established execution owner, got ${service.service.engineEpoch}`,
    );

    const generated = await call('POST', generatePath, {
      base_revision: 0,
      request: legalRequest(),
    });
    assert.equal(generated.status, 200, `generate must commit: ${generated.text.slice(0, 600)}`);
    assert.equal(generated.payload.new_revision, 1, generated.text.slice(0, 600));
    const document = generated.payload.document;
    assert.equal(document.id, STRATEGY, generated.text.slice(0, 600));
    assert.equal(document.revision, 1, generated.text.slice(0, 600));
    assert.equal(document.state, 'candidate', generated.text.slice(0, 600));
    assert.equal(document.provenance.source, 'generate', 'only the Generate kernel writes this');
    assert.ok(Array.isArray(generated.payload.warnings), 'warnings ride the committed candidate');

    const simulated = await call('POST', simulatePath, { document });
    assert.equal(simulated.status, 200, `simulate must walk: ${simulated.text.slice(0, 600)}`);
    const trajectory = simulated.payload;
    assert.match(
      trajectory.ir_content_hash,
      /^[0-9a-f]{64}$/,
      `the trajectory is bound to the document bytes: ${simulated.text.slice(0, 600)}`,
    );
    assert.ok(
      Array.isArray(trajectory.nodes_visited) && trajectory.nodes_visited.length > 0,
      'the walk visits the document, so the trajectory is not an empty shell',
    );

    // invariant: stale evidence runs FIRST, while the candidate hash is UNAPPROVED. Persistence
    // keys the approval on the current candidate hash with `ON CONFLICT DO
    // NOTHING`, so a premature freeze replayed after a good approval is a no-op
    // and cannot be observed. Before any approval exists it has nowhere to hide:
    // it would insert an approval row or move the candidate, and the read-back
    // below compares the COMPLETE payloads the refusal must leave untouched.
    const candidateBefore = await call('GET', candidatePath);
    assert.equal(candidateBefore.status, 200, candidateBefore.text.slice(0, 600));
    assert.equal(
      candidateBefore.payload.revision,
      document.revision,
      'the candidate row is the CAS revision',
    );
    const approvalsBefore = await call('GET', approvalsPath);
    assert.equal(approvalsBefore.status, 200, approvalsBefore.text.slice(0, 600));
    assert.equal(
      approvalsBefore.payload.items.length,
      0,
      'the stale case must run before any approval exists',
    );

    const stale = await call('POST', approvePath, {
      base_revision: document.revision,
      trajectory: { ...trajectory, ir_content_hash: '0'.repeat(64) },
    });
    assert.equal(
      stale.status,
      422,
      `stale evidence is a typed refusal, never a success: ${stale.text.slice(0, 600)}`,
    );
    assert.equal(stale.payload.error.code, 'invalid_input');
    assert.equal(stale.payload.error.details?.reason, 'simulation_stale');
    assert.equal(stale.payload.error.details?.recorded_content_hash, '0'.repeat(64));
    assert.equal(stale.payload.error.details?.current_content_hash, trajectory.ir_content_hash);

    const candidateAfterRefusal = await call('GET', candidatePath);
    assert.equal(candidateAfterRefusal.status, 200, candidateAfterRefusal.text.slice(0, 600));
    assert.deepEqual(
      candidateAfterRefusal.payload,
      candidateBefore.payload,
      'a refused approve leaves the complete candidate payload identical',
    );
    const approvalsAfterRefusal = await call('GET', approvalsPath);
    assert.equal(approvalsAfterRefusal.status, 200, approvalsAfterRefusal.text.slice(0, 600));
    assert.deepEqual(
      approvalsAfterRefusal.payload.items,
      approvalsBefore.payload.items,
      'a refused approve writes no approval row and consumes no candidate revision',
    );

    const approved = await call('POST', approvePath, {
      base_revision: document.revision,
      trajectory,
    });
    assert.equal(approved.status, 200, `approve must freeze: ${approved.text.slice(0, 600)}`);
    assert.equal(approved.payload.strategy_id, STRATEGY);
    assert.equal(approved.payload.candidate_revision, document.revision);
    assert.equal(
      approved.payload.content_hash,
      trajectory.ir_content_hash,
      'the approval freezes exactly the bytes the simulation walked',
    );
    assert.match(approved.payload.approval_id, /^apr_/, approved.text.slice(0, 600));

    // why: re-read what was PERSISTED, not what the write returned.
    const candidate = await call('GET', candidatePath);
    assert.equal(candidate.status, 200, candidate.text.slice(0, 600));
    assert.equal(candidate.payload.revision, document.revision, 'the candidate row is the CAS revision');

    const approvals = await call('GET', approvalsPath);
    assert.equal(approvals.status, 200, approvals.text.slice(0, 600));
    assert.equal(approvals.payload.items.length, 1, 'one approval froze one snapshot');
    assert.equal(approvals.payload.items[0].candidate_revision, document.revision);
    assert.equal(approvals.payload.items[0].content_hash, trajectory.ir_content_hash);

    const applied = await call('GET', appliedPath);
    assert.equal(applied.status, 200, applied.text.slice(0, 600));
    assert.equal(
      applied.payload.newest_approval?.content_hash,
      trajectory.ir_content_hash,
      'the status read reports the same persisted hash',
    );
  });
});
