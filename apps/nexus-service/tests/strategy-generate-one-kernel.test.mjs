/**
 * P6-T10: one kernel, three surfaces — the drift guard.
 *
 * `CoreService::generate_strategy_candidate` has three transports: `nexus42 preset generate`, the
 * `POST /v1/daemon/strategies/{strategy_id}/generate` route, and the `nexus.strategy.generate`
 * agent tool. This test drives all three against ONE home and asserts each leg's committed
 * candidate — revision 1, the requested document id, and `provenance.source == "generate"`, which
 * only the kernel writes. Rewiring any surface to another entry point (an import, a compose, a
 * second generator) breaks the revision or the provenance, so the guard fails rather than passing
 * three unrelated smoke checks.
 *
 * It also pins the exhaustion rendering on the HTTP surface: lint exhaustion must arrive as the
 * D20 payload under the daemon's `invalid_input` details — `code: lint_exhausted`,
 * `rounds_attempted`, `persisted: false`, and a violation whose `correction` names the change. The
 * domain-only profile is the leg that proves it, because that profile attaches no prompt executor:
 * round 1 is the exhaustion, never a corrector call.
 *
 * Prerequisites, both already used by `execution-http.test.mjs`:
 *   - the staged native addon (`node packages/nexus-native/scripts/build.mjs`; CI stages it before
 *     running this suite),
 *   - `python3` and the deterministic mock ACP fixture, for the hosted owner the agent tool needs.
 *
 * Nothing here reads or writes the developer's own `~/.nexus42`.
 */
import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { homedir, tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync, spawnSync } from 'node:child_process';
import { before, after, describe, test } from 'node:test';

const serviceRoot = join(dirname(fileURLToPath(import.meta.url)), '..');
const repoRoot = join(serviceRoot, '..', '..');
const acpFixture = resolve(repoRoot, 'crates/nexus-agent-host/tests/fixtures/mock_acp_workflow.py');

/** One strategy per surface, so three revisions of one id can never be confused for one kernel. */
const CLI_STRATEGY = 'gen-cli-fixture';
const ROUTE_STRATEGY = 'gen-route-fixture';
const TOOL_STRATEGY = 'gen-tool-fixture';
const EXHAUST_STRATEGY = 'gen-exhaust-fixture';

const TOOL = 'nexus.strategy.generate';
const generatePath = (strategyId) => `/v1/daemon/strategies/${strategyId}/generate`;

function node(id, kind) {
  return { id, kind, effect_class: 'pure' };
}

/** A request every lint layer accepts: one outer state wired to its terminal, one run intent. */
function legalRequest() {
  return {
    entry: 'start',
    terminal: ['done'],
    nodes: [node('start', 'outer_state'), node('done', 'terminal')],
    edges: [{ id: 'start->done', from: 'start', to: 'done', kind: 'linear' }],
    nests: [],
    meta: {
      description: 'generate one-kernel fixture',
      kind: 'creator',
      run_intents: ['work_init'],
      requires_capabilities: [],
    },
  };
}

/** The same graph with one `depends_on` entry naming a node the document does not declare. */
function danglingRequest() {
  const request = legalRequest();
  request.nodes[0] = { ...request.nodes[0], depends_on: ['missing'] };
  return request;
}

/** A minimal user preset bundle: the layout the core resolves as a strategy, i.e. an address. */
function writePreset(nexusHome, id) {
  const bundle = join(nexusHome, 'presets', id);
  mkdirSync(bundle, { recursive: true });
  writeFileSync(
    join(bundle, 'preset.yaml'),
    `preset:
  id: ${id}
  version: 1
  kind: creator
  description: "generate one-kernel fixture"
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
 * A disposable home: the seeded wire fixture (creator + workspace + store), the four strategy
 * bundles this test addresses, and one mock ACP provider so the hosted profile can establish an
 * execution owner — the agent tool refuses without one.
 */
function seedHome() {
  const home = mkdtempSync(join(tmpdir(), 'nexus-service-generate-'));
  const seed = spawnSync(
    'cargo',
    ['run', '-q', '-p', 'nexus-core-node', '--bin', 'native-wire-fixture-seed', '--', home],
    { stdio: 'inherit' },
  );
  assert.equal(seed.status, 0, seed.stderr?.toString());

  const nexusHome = join(home, '.nexus42');
  for (const id of [CLI_STRATEGY, ROUTE_STRATEGY, TOOL_STRATEGY, EXHAUST_STRATEGY]) {
    writePreset(nexusHome, id);
  }

  // The selected workspace's registered creative root — the one document
  // `nexus42 creator workspace create --creative-root <abs>` writes. Without it
  // the workspace composition refuses the hosted execution owner with
  // `uninitialized`, `startExecutionOwner` reports `engine_epoch: null`, and the
  // agent-tool leg has no owner to dispatch through.
  const creativeRoot = join(home, 'creative-root');
  mkdirSync(creativeRoot, { recursive: true });
  writeFileSync(
    join(nexusHome, 'creators', 'ctr_testcreator', 'workspaces', 'default', 'meta.json'),
    JSON.stringify({ local_root: creativeRoot }),
  );

  const agentHostDir = join(nexusHome, 'agent-host');
  mkdirSync(agentHostDir, { recursive: true });
  const python = execFileSync('which', ['python3'], { encoding: 'utf8' }).trim();
  writeFileSync(
    join(agentHostDir, 'config.toml'),
    `[[providers]]
id = "mock-acp"
protocol = "acp"
command = ${JSON.stringify(python)}
args = [${JSON.stringify(acpFixture)}]
enabled = true

[providers.env]
`,
  );
  return home;
}

/** One committed candidate, read the same way from every surface's payload. */
function assertCandidate(payload, strategyId, surface) {
  const detail = `${surface}: ${JSON.stringify(payload).slice(0, 600)}`;
  assert.equal(payload.new_revision, 1, `a first candidate commits at revision 1 — ${detail}`);
  assert.equal(payload.document.id, strategyId, `the document is the requested strategy — ${detail}`);
  assert.equal(payload.document.revision, 1, `the document revision is the CAS revision — ${detail}`);
  assert.equal(
    payload.document.state,
    'candidate',
    `generate returns a candidate, never a live strategy — ${detail}`,
  );
  assert.equal(
    payload.document.provenance.source,
    'generate',
    `only the generate kernel writes this provenance — ${detail}`,
  );
  assert.ok(Array.isArray(payload.warnings), `warnings ride the output — ${detail}`);
}

describe('generate one-kernel drift guard (P6-T10)', () => {
  let home;
  let service;

  before(async () => {
    home = seedHome();
    const build = spawnSync('npx', ['tsc', '-p', 'tsconfig.json'], {
      cwd: serviceRoot,
      stdio: 'inherit',
    });
    assert.equal(build.status, 0);
  });

  after(async () => {
    if (service) await service.close().catch(() => undefined);
    if (home) rmSync(home, { recursive: true, force: true });
  });

  async function startService(domainOnly) {
    const { startService: start } = await import(join(serviceRoot, 'dist', 'index.js'));
    return start({ home, host: '127.0.0.1', port: 0, allowRemote: false, domainOnly });
  }

  async function post(target, path, body) {
    const response = await fetch(`${target.url}${path}`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(body),
    });
    const text = await response.text();
    return { status: response.status, payload: text.length > 0 ? JSON.parse(text) : null };
  }

  test('the CLI, the route and the agent tool reach one generate kernel', async () => {
    // ── Leg 1: the CLI. Run before any service holds the home, so the child's direct writer opens
    // uncontended; the strategy is one of the seeded user bundles.
    const requestPath = join(home, 'legal-request.json');
    writeFileSync(requestPath, JSON.stringify(legalRequest(), null, 2));
    // `HOME` must be the fixture's, so the CLI opens the fixture store; `RUSTUP_HOME`/`CARGO_HOME`
    // must stay the developer's, or rustup cannot resolve a toolchain under the overridden HOME.
    const cliEnv = {
      ...process.env,
      HOME: home,
      RUST_LOG: 'off',
      RUSTUP_HOME: process.env.RUSTUP_HOME ?? join(homedir(), '.rustup'),
      CARGO_HOME: process.env.CARGO_HOME ?? join(homedir(), '.cargo'),
    };
    const cli = spawnSync(
      'cargo',
      [
        'run',
        '-q',
        '-p',
        'nexus42',
        '--bin',
        'nexus42',
        '--',
        'preset',
        'generate',
        CLI_STRATEGY,
        '--request',
        requestPath,
      ],
      { env: cliEnv, encoding: 'utf8' },
    );
    assert.equal(cli.status, 0, `preset generate must commit: ${cli.stderr}`);
    assertCandidate(JSON.parse(cli.stdout), CLI_STRATEGY, 'nexus42 preset generate');

    // ── Leg 2: the daemon route, on the domain-only profile — no executor, so a lint-failing
    // request is exhaustion at round 1 and the D20 payload is observable as `details`.
    service = await startService(true);
    const exhausted = await post(service, generatePath(EXHAUST_STRATEGY), {
      base_revision: 0,
      request: danglingRequest(),
    });
    assert.equal(
      exhausted.status,
      422,
      `lint exhaustion is the 422 invalid_input envelope: ${JSON.stringify(exhausted.payload)}`,
    );
    assert.equal(exhausted.payload.error.code, 'invalid_input');
    const details = exhausted.payload.error.details;
    assert.equal(details?.code, 'lint_exhausted');
    assert.equal(details.rounds_attempted, 1, 'no corrector was attached, so round 1 is the ceiling');
    assert.equal(details.persisted, false);
    const violation = details.violations[0];
    assert.equal(violation.kind, 'dangling_dependency');
    assert.match(violation.observed, /missing/, 'the violation names what was actually seen');
    assert.ok(
      typeof violation.location === 'string' && violation.location.length > 0,
      `the violation is addressed: ${JSON.stringify(violation)}`,
    );
    assert.match(
      violation.correction,
      /missing/,
      `the correction names what to change: ${JSON.stringify(violation)}`,
    );
    assert.match(details.message, /fresh generate request/);
    await service.close();
    service = null;

    // ── Legs 2 and 3: the route and the agent tool, on the hosted profile the tool needs.
    service = await startService(false);
    const routed = await post(service, generatePath(ROUTE_STRATEGY), {
      base_revision: 0,
      request: legalRequest(),
    });
    assert.equal(
      routed.status,
      200,
      `the route must commit: ${JSON.stringify(routed.payload).slice(0, 600)}`,
    );
    assertCandidate(routed.payload, ROUTE_STRATEGY, 'POST …/generate');

    const principal = await service.service.core.activePrincipal();
    // The tool dispatches through the execution owner's `ToolContext`. The
    // fixture registers a creative root so the hosted owner is established —
    // the same seam `compute-http.test.mjs` uses — and there is no skip here: a
    // green run has to include this leg.
    assert.notEqual(
      service.service.engineEpoch,
      null,
      'the hosted profile must report an engine epoch (an execution owner was established)',
    );
    const tool = await service.service.core.executeTool(principal, {
      tool_name: TOOL,
      parameters: {
        strategy_id: TOOL_STRATEGY,
        base_revision: 0,
        request: legalRequest(),
      },
    });
    assert.equal(tool.success, true, `the tool must commit: ${JSON.stringify(tool).slice(0, 600)}`);
    assertCandidate(tool.result, TOOL_STRATEGY, `tool ${TOOL}`);
  });
});
