import assert from 'node:assert/strict';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
import { execFileSync, spawnSync } from 'node:child_process';
import { before, describe, test } from 'node:test';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = join(__dirname, '..', '..', '..');
const serviceRoot = join(__dirname, '..');
const require = createRequire(import.meta.url);
const acpFixture = resolve(root, 'crates/nexus-agent-host/tests/fixtures/mock_acp_workflow.py');
const dshFixture = resolve(
  root,
  'crates/nexus-agent-host/tests/fixtures/native_protocol/mock_dsh_agent.py',
);

const CREATOR = 'ctr_testcreator';
const SLUG = 'default';
const OWNED_WORLD = 'wld_owned';

/**
 * P0-T5 bounded real-native target: the production native boot and the
 * readiness it publishes.
 *
 * Every observation below comes from the real service over the real native
 * addon against a real seeded store: a blank home stays the uninitialized
 * shell, an initialized home publishes the ACTUAL hosted-owner epoch, a
 * selected provider whose owner-bound probe failed is NOT advertised ready
 * (while a selected provider that did probe is), a duplicate owner boot is
 * refused, and domain-only access keeps reading/writing allowed non-engine
 * state. No model request and no mock forwards anything.
 *
 * The native addon admits ONE open core per JS environment, so each case opens
 * its service and closes it before the next case opens one.
 */

function resolvePython() {
  const which = execFileSync('which', ['python3'], { encoding: 'utf8' }).trim();
  return spawnSync('python3', ['-c', `import os,sys;print(os.path.realpath(sys.argv[1]))`, which], {
    encoding: 'utf8',
  }).stdout.trim();
}

/**
 * The selected provider this product dispatches to, pointed at a runtime that
 * cannot start: its ordinary+sealed probe fails deterministically, and no
 * process is spawned to do it.
 */
const BROKEN_SELECTED_PROVIDER = `
[[providers]]
id = "dsh-native"
protocol = "native_cli"
command = "/nonexistent/nexus-t5-dsh"
enabled = true
`;

/**
 * The selected ACP provider (the deterministic test-protocol fixture peer).
 *
 * `fixtureLog` points the fixture's own request log at an absolute path inside
 * the case's disposable home, so a case can read back the prompt a real engine
 * boundary actually dispatched. It is omitted by default: every existing
 * boot/readiness case keeps the exact provider document it had.
 *
 * `env` adds further fixture knobs to the SAME `[providers.env]` table (e.g.
 * the prompt gate, which holds a boundary's prompt until the case releases it).
 */
function acpProviderConfig({ fixtureLog, env = {} } = {}) {
  const python = resolvePython();
  const envVars = {
    ...(fixtureLog === undefined ? {} : { ACP_FIXTURE_LOG: fixtureLog }),
    ...env,
  };
  const envTable =
    Object.keys(envVars).length === 0
      ? ''
      : `[providers.env]\n${Object.entries(envVars)
          .map(([key, value]) => `${key} = ${JSON.stringify(value)}`)
          .join('\n')}\n`;
  return `
[[providers]]
id = "mock-acp"
protocol = "acp"
command = ${JSON.stringify(python)}
args = [${JSON.stringify(acpFixture)}]
enabled = true
${envTable}`;
}

/**
 * Seed one disposable home: the shared native wire fixture (creator, workspace,
 * durable store, worlds/KB) plus the two things an initialized profile needs —
 * the agent-host provider selection, and the selected workspace's registered
 * creative root.
 *
 * `meta.json`'s `local_root` is the exact document `nexus42 creator workspace
 * create --creative-root <abs>` writes and the only workspace-root writer; it
 * is not a private DB seed.
 *
 * `providerConfig` is the config document, or a factory of the seeded home when
 * the document must name an absolute path inside it (a fixture request log).
 */
function seededHome(providerConfig) {
  const home = mkdtempSync(join(tmpdir(), 'nexus-t5-'));
  const seed = spawnSync(
    'cargo',
    ['run', '-q', '-p', 'nexus-core-node', '--bin', 'native-wire-fixture-seed', '--', home],
    { cwd: root, stdio: 'inherit' },
  );
  assert.equal(seed.status, 0, seed.stderr?.toString());
  const agentHostDir = join(home, '.nexus42', 'agent-host');
  mkdirSync(agentHostDir, { recursive: true });
  writeFileSync(
    join(agentHostDir, 'config.toml'),
    typeof providerConfig === 'function' ? providerConfig(home) : providerConfig,
  );
  const creativeRoot = join(home, 'creative-root');
  mkdirSync(creativeRoot, { recursive: true });
  writeFileSync(
    join(home, '.nexus42', 'creators', CREATOR, 'workspaces', SLUG, 'meta.json'),
    JSON.stringify({ local_root: creativeRoot }),
  );
  return home;
}

function blankHome() {
  return mkdtempSync(join(tmpdir(), 'nexus-t5-blank-'));
}

async function jsonFetch(url, { method = 'GET', body } = {}) {
  const response = await fetch(url, {
    method,
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  const text = await response.text();
  const payload = text.length > 0 ? JSON.parse(text) : null;
  return { status: response.status, payload, text };
}

async function startServiceOn(home, extra = {}) {
  const { startService } = await import(join(serviceRoot, 'dist/index.js'));
  return startService({ home, host: '127.0.0.1', port: 0, allowRemote: false, ...extra });
}

/**
 * Boot one real service process over `home` and return the discovery record it
 * published, then let it close. A separate process is what the product's
 * restart is, so this observes the record exactly as a restarted service
 * publishes it.
 */
function bootInChildProcess(home) {
  const script = `
const { startService } = await import(${JSON.stringify(join(serviceRoot, 'dist/index.js'))});
const service = await startService({
  home: process.env.NEXUS_T5_HOME,
  host: '127.0.0.1',
  port: 0,
  allowRemote: false,
});
process.stdout.write('__DISCOVERY__' + JSON.stringify(service.discovery));
await service.close();
`;
  const child = spawnSync(process.execPath, ['--input-type=module', '-e', script], {
    cwd: root,
    env: { ...process.env, NEXUS_T5_HOME: home },
    encoding: 'utf8',
  });
  assert.equal(child.status, 0, child.stderr);
  const record = /__DISCOVERY__(\{.*\})/s.exec(child.stdout ?? '');
  assert.ok(record, `the service child published no discovery record: ${child.stdout}`);
  return JSON.parse(record[1]);
}

/** Runtime status plus the discovery record this open service published. */
async function observe(service) {
  const response = await jsonFetch(`${service.url}/v1/daemon/runtime/status`);
  assert.equal(response.status, 200, response.text);
  return { status: response.payload, discovery: service.discovery };
}

describe('workflow-control-http (v1.195 P0-T5 native boot and truthful readiness)', () => {
  before(() => {
    assert.equal(
      spawnSync('node', ['packages/nexus-native/scripts/build.mjs'], { cwd: root, stdio: 'inherit' })
        .status,
      0,
    );
    assert.equal(
      spawnSync('npx', ['tsc', '-p', 'tsconfig.json'], { cwd: serviceRoot, stdio: 'inherit' }).status,
      0,
    );
  });

  test('hosted readiness and owner: blank home stays uninitialized', async () => {
    const service = await startServiceOn(blankHome());
    try {
      const { status, discovery } = await observe(service);
      assert.equal(status.workspace_initialized, false, JSON.stringify(status));
      assert.equal(status.runtime_mode, 'uninitialized');
      assert.equal(status.acp.tool_execution_enabled, false);
      assert.equal(discovery.readiness, 'uninitialized');
      assert.equal(discovery.engine_epoch, null);
      assert.equal(discovery.creator_id, null);
    } finally {
      await service.close();
    }
  });

  test('hosted readiness and owner: an unprobed selected provider is not ready', async () => {
    // The selected provider's ordinary+sealed probe cannot succeed: the
    // configured runtime does not exist. Catalog presence is a candidate, so
    // the profile must be published degraded rather than ready.
    const service = await startServiceOn(seededHome(BROKEN_SELECTED_PROVIDER));
    try {
      const { status, discovery } = await observe(service);
      assert.equal(status.workspace_initialized, true, JSON.stringify(status));
      assert.equal(status.runtime_mode, 'provider_degraded', JSON.stringify(status));
      assert.equal(status.acp.tool_execution_enabled, false);
      assert.equal(service.service.providerReady, false);
      // The owner itself DID boot: the profile is initialized, and only the
      // provider lane is not ready.
      assert.equal(discovery.readiness, 'ready');
    } finally {
      await service.close();
    }
  });

  test('hosted readiness and owner: a probed selected provider is ready', async () => {
    // Positive control for the same rule: with the selection probed available
    // the profile is published ready. An implementation that always reports
    // not-ready cannot pass both cases.
    const service = await startServiceOn(seededHome(acpProviderConfig()));
    try {
      const { status } = await observe(service);
      assert.equal(status.workspace_initialized, true, JSON.stringify(status));
      assert.equal(status.runtime_mode, 'provider_enabled', JSON.stringify(status));
      assert.equal(status.acp.tool_execution_enabled, true);
    } finally {
      await service.close();
    }
  });

  test('hosted readiness and owner: publishes the actual owner epoch', async () => {
    const home = seededHome(BROKEN_SELECTED_PROVIDER);
    // Each boot is its own service process (the product's real restart), so the
    // published epoch is observed exactly as a restarted service reports it.
    const first = bootInChildProcess(home);
    assert.equal(first.readiness, 'ready', JSON.stringify(first));
    assert.ok(
      Number.isInteger(first.engine_epoch) && first.engine_epoch > 0,
      `an established hosted owner must publish its real epoch, got ${first.engine_epoch}`,
    );

    // Every owner admission advances the writer protocol's engine epoch, so a
    // literal/constant epoch cannot satisfy both observations.
    const second = bootInChildProcess(home);
    assert.ok(
      Number.isInteger(second.engine_epoch) && second.engine_epoch > first.engine_epoch,
      `the published epoch must track the native owner admission (${first.engine_epoch} → ${second.engine_epoch})`,
    );
  });

  test('hosted readiness and owner: a duplicate owner boot is refused', async () => {
    const home = seededHome(acpProviderConfig());
    const { openCore } = await import('@42ch/nexus-native');
    const core = await openCore(
      { user_home: home, access: 'engine_owner', allow_uninitialized: false },
      undefined,
    );
    try {
      const owner = await core.startExecutionOwner();
      assert.ok(
        Number.isInteger(owner.engine_epoch) && owner.engine_epoch > 0,
        `the native boot must report the actual epoch, got ${JSON.stringify(owner)}`,
      );
      assert.equal(owner.provider_ready, true, JSON.stringify(owner));
      await assert.rejects(
        () => core.startExecutionOwner(),
        (error) => {
          assert.ok(
            /already established|busy/i.test(String(error?.message)),
            `a duplicate owner boot must be refused truthfully, got: ${error?.message}`,
          );
          return true;
        },
      );
    } finally {
      await core.close();
    }
  });

  test('hosted readiness and owner: a confirmed close releases the workspace authority', async () => {
    const home = seededHome(BROKEN_SELECTED_PROVIDER);
    const { openCore } = await import('@42ch/nexus-native');
    const coreA = await openCore(
      { user_home: home, access: 'engine_owner', allow_uninitialized: false },
      undefined,
    );
    const ownerA = await coreA.startExecutionOwner();
    assert.ok(
      Number.isInteger(ownerA.engine_epoch) && ownerA.engine_epoch > 0,
      `owner A must be a real owner, got ${JSON.stringify(ownerA)}`,
    );

    const reportA = await coreA.close();
    assert.equal(reportA.state, 'closed', JSON.stringify(reportA));
    assert.equal(reportA.cleanup_confirmed, true, JSON.stringify(reportA));

    // `coreA`/`ownerA` stay referenced for the rest of this case (and the
    // epoch below is read from `ownerA`): the release must not depend on the
    // closed owner's references being dropped.

    // The next owner over the SAME home, in the SAME process: the confirmed
    // close released the workspace authority, so this composes and admits a
    // NEW engine generation instead of refusing `busy`.
    const coreB = await openCore(
      { user_home: home, access: 'engine_owner', allow_uninitialized: false },
      undefined,
    );
    try {
      const ownerB = await coreB.startExecutionOwner();
      assert.ok(
        Number.isInteger(ownerB.engine_epoch) && ownerB.engine_epoch > ownerA.engine_epoch,
        `the next owner is a new admission (${ownerA.engine_epoch} -> ${ownerB.engine_epoch})`,
      );
      // A live duplicate is still refused.
      await assert.rejects(
        () => coreB.startExecutionOwner(),
        (error) => {
          assert.ok(
            /already established|busy/i.test(String(error?.message)),
            `a duplicate owner boot must still be refused, got: ${error?.message}`,
          );
          return true;
        },
      );
    } finally {
      await coreB.close();
    }
  });

  test('hosted readiness and owner: an unconfirmed cleanup cannot publish a new owner', async () => {
    const home = seededHome(BROKEN_SELECTED_PROVIDER);
    const { openCore } = await import('@42ch/nexus-native');
    // The forced-unconfirmed seam is a test-only native export, reached the
    // same way the other focused suites reach it.
    const { loadNodePath } = await import(
      join(root, 'packages', 'nexus-native', 'dist', 'loader.js')
    );
    const binding = require(loadNodePath());
    const coreA = await openCore(
      { user_home: home, access: 'engine_owner', allow_uninitialized: false },
      undefined,
    );
    const ownerA = await coreA.startExecutionOwner();

    binding.forceUnconfirmedCleanup(true);
    try {
      // An unconfirmed cleanup reports what it is: never a confirmed close.
      const reportA = await coreA.close();
      assert.equal(reportA.state, 'interrupted', JSON.stringify(reportA));
      assert.equal(reportA.cleanup_confirmed, false, JSON.stringify(reportA));

      // …and it keeps the home fenced: the SAME process cannot publish a new
      // owner over it while the retained cleanup is unsettled.
      await assert.rejects(
        () =>
          openCore(
            { user_home: home, access: 'engine_owner', allow_uninitialized: false },
            undefined,
          ),
        (error) => {
          // The native open rejection for a retained cleanup is a plain reason
          // ("interrupted: …"), which the binding surfaces as its generic
          // `open_failed` envelope; what matters is that no owner is published.
          assert.ok(
            /open failed|interrupted|busy/i.test(String(error?.message)),
            `an unconfirmed cleanup must not publish a new owner, got: ${error?.message}`,
          );
          return true;
        },
      );
    } finally {
      binding.forceUnconfirmedCleanup(false);
    }

    // With the forced failure gone, the retained owner settles on the retried
    // close and only THEN does the home admit a real new owner — the
    // unconfirmed attempt never published one.
    const coreB = await openCore(
      { user_home: home, access: 'engine_owner', allow_uninitialized: false },
      undefined,
    );
    try {
      const ownerB = await coreB.startExecutionOwner();
      assert.ok(
        Number.isInteger(ownerB.engine_epoch) && ownerB.engine_epoch > ownerA.engine_epoch,
        `the settled home admits a new generation (${ownerA.engine_epoch} -> ${ownerB.engine_epoch})`,
      );
    } finally {
      await coreB.close();
    }
  });

  test('hosted readiness and owner: no canonical creative root probes nothing and publishes no owner', async () => {
    // The same probe-able ACP selection as the positive control, seeded with
    // its registered creative root first: the valid-root control must report
    // the REAL owner epoch and a probed-ready provider.
    const home = seededHome(acpProviderConfig());
    const control = await startServiceOn(home);
    try {
      const { status, discovery } = await observe(control);
      assert.equal(status.runtime_mode, 'provider_enabled', JSON.stringify(status));
      assert.equal(control.service.providerReady, true, JSON.stringify(status));
      assert.ok(
        Number.isInteger(discovery.engine_epoch) && discovery.engine_epoch > 0,
        `the valid-root control must publish its real owner epoch, got ${discovery.engine_epoch}`,
      );
    } finally {
      await control.close();
    }

    // Then invalidate ONLY the selected `meta.json.local_root`. The provider
    // selection is unchanged and would still probe if it were probed, so a
    // ready lane here would prove a probe ran outside the selected creative
    // root. Without a canonical root there must be NO owner-bound probe, NO
    // published owner epoch, and NO selected-provider ready claim — the exact
    // `{ engine_epoch: null, provider_ready: true }` shape is the bug.
    writeFileSync(
      join(home, '.nexus42', 'creators', CREATOR, 'workspaces', SLUG, 'meta.json'),
      JSON.stringify({}),
    );

    const rootless = await startServiceOn(home);
    try {
      const { status, discovery } = await observe(rootless);
      assert.equal(status.workspace_initialized, true, JSON.stringify(status));
      assert.equal(status.runtime_mode, 'provider_degraded', JSON.stringify(status));
      assert.equal(rootless.service.providerReady, false, JSON.stringify(status));
      // No owner was established, so no owner epoch is published: the service
      // sentinel for "no engine epoch", never a fabricated or null-epoch
      // ready owner.
      assert.equal(discovery.engine_epoch, 0, JSON.stringify(discovery));
    } finally {
      await rootless.close();
    }
  });

  /**
   * The admitted root is PINNED. A supported `meta.json.local_root` write —
   * the exact document `nexus42 creator workspace create --creative-root`
   * writes — that lands after `openCore` bound its Host probe to the earlier
   * root must never be joined to that Host: the stale admission is refused
   * with a typed `auth_required` refusal and publishes no owner/readiness. The
   * moved root is only admitted by a FRESH open (a new epoch) whose Host and
   * workspace authority both bind it.
   */
  test('hosted readiness and owner: a root that moves after open is refused, never mixed', async () => {
    const home = seededHome(acpProviderConfig());
    const metaPath = join(home, '.nexus42', 'creators', CREATOR, 'workspaces', SLUG, 'meta.json');
    const movedRoot = join(home, 'moved-creative-root');
    mkdirSync(movedRoot, { recursive: true });
    const { openCore, isNativeCoreErrorCode } = await import('@42ch/nexus-native');
    const open = () =>
      openCore({ user_home: home, access: 'engine_owner', allow_uninitialized: false }, undefined);

    const coreA = await open();
    try {
      writeFileSync(metaPath, JSON.stringify({ local_root: movedRoot }));

      // Whatever this admission answers, it must not be a published owner:
      // the Host probe lane was bound to the root that was selected at open.
      let established = null;
      let refusal = null;
      try {
        established = await coreA.startExecutionOwner();
      } catch (error) {
        refusal = error;
      }
      assert.equal(
        established,
        null,
        `a selected root that moved after open must publish no owner, got ${JSON.stringify(established)}`,
      );
      assert.ok(
        isNativeCoreErrorCode(refusal, 'auth_required'),
        `the stale admission must be refused as a typed failure, got: ${refusal?.message}`,
      );
    } finally {
      const report = await coreA.close();
      assert.equal(report.state, 'closed', JSON.stringify(report));
      assert.equal(report.cleanup_confirmed, true, JSON.stringify(report));
    }

    // Nothing was published and no authority was retained: a FRESH open that
    // selects the moved root admits a real owner over it.
    const coreB = await open();
    try {
      const ownerB = await coreB.startExecutionOwner();
      assert.ok(
        Number.isInteger(ownerB.engine_epoch) && ownerB.engine_epoch > 0,
        `the fresh open over the moved root must admit a real owner, got ${JSON.stringify(ownerB)}`,
      );
      assert.equal(ownerB.provider_ready, true, JSON.stringify(ownerB));
    } finally {
      await coreB.close();
    }
  });

  test('hosted readiness and owner: domain-only access still serves domain state', async () => {
    const service = await startServiceOn(seededHome(BROKEN_SELECTED_PROVIDER), {
      domainOnly: true,
    });
    try {
      const { status, discovery } = await observe(service);
      assert.equal(status.workspace_initialized, true, JSON.stringify(status));
      assert.equal(status.runtime_mode, 'domain_only');
      // A domain-only profile owns no execution engine, so it publishes the
      // "no engine epoch" sentinel rather than a synthesized owner epoch.
      assert.equal(discovery.engine_epoch, 0);

      const write = await jsonFetch(
        `${service.url}/v1/daemon/worlds/${OWNED_WORLD}/kb/patch-entity`,
        {
          method: 'POST',
          body: {
            entity_id: 'kb_a11ce5',
            expected_version: 0,
            patch: { title: 'T5 domain write', block_type: 'character' },
          },
        },
      );
      assert.equal(write.status, 200, write.text);
      assert.equal(write.payload.version, 1, write.text);

      const graph = await jsonFetch(`${service.url}/v1/daemon/worlds/${OWNED_WORLD}/kb/graph`);
      assert.equal(graph.status, 200, graph.text);
      assert.ok(
        (graph.payload.entities ?? []).some(
          (entity) => entity.key_block_id === 'kb_a11ce5' && entity.canonical_name === 'T5 domain write',
        ),
        `the domain write must be durable: ${graph.text}`,
      );
    } finally {
      await service.close();
    }
  });
});

// ── P0-T6: native/HTTP control closure (W1–W7 / S0-1–S0-7 boundary) ─────────

const FOREIGN_CREATOR = 'other_creator';
const FOREIGN_SCHEDULE_ID = 'SCH_foreign_control';
const FOREIGN_SESSION_ID = 'sess_foreign_control';

/** Resolve after `ms` milliseconds. */
function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/**
 * Poll `read` until `done` accepts its value; fail the case if the bounded
 * window closes first. Admission is asynchronous by contract (§3.3): the row
 * is durable before `POST /schedules` answers, and the owned run appears on
 * the SAME durable row only after the hosted admission clock admits it — so
 * the run identity is observed by polling the public read, never assumed.
 */
async function waitFor(read, done, { label, timeout = 30_000 } = {}) {
  const deadline = Date.now() + timeout;
  for (;;) {
    const value = await read();
    if (done(value)) return value;
    if (Date.now() > deadline) {
      assert.fail(`${label ?? 'waitFor'} did not settle within ${timeout}ms`);
    }
    await delay(100);
  }
}

/**
 * The W6 success-path graph: `branch_a` routes to the `join` converge gate
 * whose second upstream (`branch_b`) is never walked, so the admitted run parks
 * at that gate durably — no human wait token, no deadline, and (unlike a manual
 * wait) a legal plain resume. The manifest is written through the public preset
 * PATCH, which validates it structurally before replacing the bundle's YAML.
 */
const RESUME_PARK_PRESET = 'p0t6-resume-park';

function resumeParkPresetYaml() {
  return `preset:
  id: ${RESUME_PARK_PRESET}
  version: 1
  kind: creator
  description: "P0-T6 W6 resume fixture — an unreachable branch parks the join, no deadline"
  requires_capabilities: []
  initial: start
  terminal: done
states:
  - id: start
    next: branch_a
  - id: branch_a
    next:
      branches: []
      default: join
  - id: branch_b
    description: "Hanging upstream edge — never walked, never arrives"
    next: join
  - id: join
    converge: { strategy: wait_for_all }
    next: done
  - id: done
    terminal: true
`;
}

/**
 * The W5 boundary graph (S0-3): one state whose `enter` action is the real
 * `acp.prompt` capability, with the frozen `core_context.text` rendered into the
 * prompt payload. The engine renders capability args against the run's context
 * snapshot at the state boundary, and the fixture peer logs the exact prompt it
 * received — so the fixture's request log IS the boundary's own evidence that
 * it consumed the committed version, not an inspect increment or a UI mock.
 *
 * `tool_policy: deny_all` is the narrowest scope; the ACP adapter serves it
 * without a model request.
 */
const STEER_BOUNDARY_PRESET = 'p0t6-steer-boundary';
const STEER_BOUNDARY_MARKER = 'P0T6-BOUNDARY';
const STEER_IDEA = 'P0-T6 steer idea: the next boundary must read this';

function steerBoundaryPresetYaml() {
  return `preset:
  id: ${STEER_BOUNDARY_PRESET}
  version: 1
  kind: creator
  description: "P0-T6 W5 fixture — the first execution boundary renders the frozen core context"
  requires_capabilities: [acp.prompt]
  initial: start
  terminal: done
states:
  - id: start
    enter:
      - kind: capability
        name: acp.prompt
        args:
          prompt: "${STEER_BOUNDARY_MARKER}|{{core_context.text}}|END"
          tool_policy: deny_all
    next: done
  - id: done
    terminal: true
`;
}

/** The capacity holder for the W5 case: the scaffold preset, which parks. */
const STEER_HOLDER_PRESET = 'p0t6-steer-holder';

/**
 * The W5 ACTIVE-RUN graph (S0-3): three state boundaries in a row, each of
 * which renders the run's frozen `core_context.text` through the real
 * `acp.prompt` capability.
 *
 * It is what makes "the NEXT state transition reads the committed version"
 * observable on an ALREADY ADMITTED run: state `first` is held in flight by the
 * fixture's prompt gate while the HTTP append commits version 1, so state
 * `second` is the next execution boundary after that commit, and state `third`
 * is the next boundary after the second edit. `tool_policy: deny_all` is the
 * narrowest scope; the ACP adapter serves it without a model request.
 */
const ACTIVE_CONTEXT_PRESET = 'p0t6-active-context';
const ACTIVE_CONTEXT_MARKER = 'P0T6-ACTIVE';
const ACTIVE_CONTEXT_IDEA = 'P0-T6 active idea: the next boundary must read this';
const ACTIVE_CONTEXT_IDEA_2 = 'P0-T6 second edit: it must land one boundary later';

function activeContextPresetYaml() {
  return `preset:
  id: ${ACTIVE_CONTEXT_PRESET}
  version: 1
  kind: creator
  description: "P0-T6 W5 active-run fixture — every state boundary prompts from the run's frozen core context"
  requires_capabilities: [acp.prompt]
  initial: first
  terminal: done
states:
  - id: first
    enter:
      - kind: capability
        name: acp.prompt
        args:
          prompt: "${ACTIVE_CONTEXT_MARKER}:first|{{core_context.text}}|END"
          tool_policy: deny_all
    next: second
  - id: second
    enter:
      - kind: capability
        name: acp.prompt
        args:
          prompt: "${ACTIVE_CONTEXT_MARKER}:second|{{core_context.text}}|END"
          tool_policy: deny_all
    next: third
  - id: third
    enter:
      - kind: capability
        name: acp.prompt
        args:
          prompt: "${ACTIVE_CONTEXT_MARKER}:third|{{core_context.text}}|END"
          tool_policy: deny_all
    next: done
  - id: done
    terminal: true
`;
}

/**
 * The dsh native provider, backed by the repository's scripted SDK-wire mock
 * runtime (the existing `native_protocol/mock_dsh_agent.py` fixture).
 *
 * `env` carries the fixture's own documented knobs: `REQ_LOG` (its request
 * log), `DSH_HOME`, and `SHUTDOWN_DELAY_MS` — the fixture's
 * unconfirmed-close arm, which delays the `shutdown` reply so the provider's
 * close-waiter deadline fires while the retained cleanup owner is still
 * running. `shutdown_ms` is the per-close waiter budget; keeping it below the
 * delay is what makes an owned-session close genuinely unconfirmable inside
 * the budget.
 */
const DSH_SHUTDOWN_BUDGET_MS = 1000;
const DSH_SHUTDOWN_DELAY_MS = 8000;

function dshNativeProviderConfig(env) {
  const envTable = Object.entries(env)
    .map(([key, value]) => `${key} = ${JSON.stringify(value)}`)
    .join('\n');
  return `
[timeouts]
shutdown_ms = ${DSH_SHUTDOWN_BUDGET_MS}

[[providers]]
id = "dsh-native"
protocol = "native_cli"
command = ${JSON.stringify(dshFixture)}
enabled = true
[providers.env]
${envTable}
`;
}

/**
 * The W7 preset (S0-6): one prompt state, then a manual wait so the run is
 * non-terminal while the boundary that owns the session is still alive.
 * `deny_all` is the only narrowed scope the dsh native provider can serve for
 * an orchestration prompt (it selects the sealed deny-all recipe); the mock
 * runtime never reaches the wire, so no model request is involved.
 */
const UNCONFIRMED_CANCEL_PRESET = 'p0t6-unconfirmed-cancel';

function unconfirmedCancelPresetYaml() {
  return `preset:
  id: ${UNCONFIRMED_CANCEL_PRESET}
  version: 1
  kind: creator
  description: "P0-T6 W7 fixture — a prompt boundary whose owned session cannot be confirmed closed"
  requires_capabilities: [acp.prompt]
  initial: start
  terminal: done
states:
  - id: start
    enter:
      - kind: capability
        name: acp.prompt
        args:
          prompt: "P0-T6 unconfirmed cancel boundary"
          tool_policy: deny_all
    exit_when:
      kind: manual
    next: done
  - id: done
    terminal: true
`;
}

/**
 * Seed the durable rows only ANOTHER creator could have written: one terminal
 * root run and one terminal schedule owned by `other_creator`, inside the same
 * workspace state DB the restored owner serves.
 *
 * Both are terminal and `legacy_inert`, so neither boot recovery nor the
 * hosted admission clock touches them. What they prove is the OWNERSHIP
 * boundary: a surface that read stored rows without the stored owner would
 * serve them, and an absent-id-only check could not tell the difference.
 *
 * The writer protocol validates every store write through connection-local
 * scalar functions, so this seed declares the engine writer identity the
 * fixture's own `init_engine_pool` committed (the same convention
 * `execution-http.test.mjs` uses to inject a journal orphan).
 */
function seedForeignControlRows(home) {
  const script = `
import sqlite3, glob, sys
path = glob.glob(sys.argv[1] + "/.nexus42/creators/*/workspaces/*/state.db")[0]
conn = sqlite3.connect(path)
gate = conn.execute("SELECT migration_epoch, engine_epoch FROM core_workspace_gate WHERE pk=1").fetchone()
wid = conn.execute("SELECT writer_id FROM core_writer_registration WHERE mode='engine' AND engine_epoch=?", (gate[1],)).fetchone()[0]
conn.create_function("nexus_writer_protocol", 0, lambda: 1)
conn.create_function("nexus_writer_mode", 0, lambda: "engine")
conn.create_function("nexus_writer_id", 0, lambda: wid)
conn.create_function("nexus_migration_epoch", 0, lambda: gate[0])
conn.create_function("nexus_engine_epoch", 0, lambda: gate[1])
now = 1
conn.execute("INSERT INTO orchestration_sessions (session_id, creator_id, preset_id, preset_version, parent_session_id, current_task_id, status, context_json, created_at, updated_at) VALUES (?, ?, 'foreign-preset', 1, NULL, NULL, 'completed', ?, ?, ?)", (${JSON.stringify(
    FOREIGN_SESSION_ID,
  )}, ${JSON.stringify(FOREIGN_CREATOR)}, b"{}", now, now))
conn.execute("INSERT INTO creator_schedules (schedule_id, creator_id, preset_id, preset_version, status, concurrency_kind, concurrency_whitelist, current_core_context_version, current_session_id, scheduled_at, label, created_at, updated_at, terminated_at) VALUES (?, ?, 'foreign-preset', 1, 'completed', 'serial', NULL, 0, ?, NULL, NULL, ?, ?, NULL)", (${JSON.stringify(
    FOREIGN_SCHEDULE_ID,
  )}, ${JSON.stringify(FOREIGN_CREATOR)}, ${JSON.stringify(FOREIGN_SESSION_ID)}, now, now))
conn.commit()
`;
  const seeded = spawnSync('python3', ['-c', script, home], { encoding: 'utf8' });
  assert.equal(seeded.status, 0, seeded.stderr || seeded.stdout);
}

describe('workflow-control-http (v1.195 P0-T6 native/HTTP control closure)', () => {
  before(() => {
    // The Rust addon carries the new boundary, and the TS facade re-declares
    // it; the service compiles against the facade's declaration output, so all
    // three are built here (each is incremental after the first run).
    assert.equal(
      spawnSync('node', ['packages/nexus-native/scripts/build.mjs'], { cwd: root, stdio: 'inherit' })
        .status,
      0,
    );
    assert.equal(
      spawnSync('npx', ['tsc', '-p', 'packages/nexus-native/tsconfig.json'], {
        cwd: root,
        stdio: 'inherit',
      }).status,
      0,
    );
    assert.equal(
      spawnSync('npx', ['tsc', '-p', 'tsconfig.json'], { cwd: serviceRoot, stdio: 'inherit' }).status,
      0,
    );
  });

  test('control round trip: create, list, inspect, session, append, resume and cancel act on one durable run', async () => {
    const home = seededHome(acpProviderConfig());
    const service = await startServiceOn(home);
    try {
      const base = service.url;
      const scheduleUrl = (id) => `${base}/v1/daemon/orchestration/schedules/${id}`;
      const inspectSchedule = async (id = scheduleId) => {
        const response = await jsonFetch(scheduleUrl(id));
        assert.equal(response.status, 200, response.text);
        return response.payload;
      };
      const inspectSession = async (id) => {
        const response = await jsonFetch(`${base}/v1/daemon/orchestration/sessions/${id}`);
        assert.equal(response.status, 200, response.text);
        return response.payload;
      };

      // W1: a real user preset through the public authoring surface, then the
      // schedule that runs it. Its scaffold graph opens the workspace and then
      // waits for manual input, so the admitted run stays non-terminal for the
      // whole control journey — no model request is involved anywhere.
      const scaffolded = await jsonFetch(`${base}/v1/daemon/presets`, {
        method: 'POST',
        body: { name: 'p0t6-control' },
      });
      assert.equal(scaffolded.status, 201, scaffolded.text);
      const presetId = scaffolded.payload.id;
      assert.equal(presetId, 'p0t6-control', scaffolded.text);

      const created = await jsonFetch(`${base}/v1/daemon/orchestration/schedules`, {
        method: 'POST',
        body: { creator_id: CREATOR, preset_id: presetId },
      });
      assert.equal(created.status, 201, created.text);
      assert.equal(created.payload.status, 'pending', created.text);
      assert.equal(created.payload.core_context_version, 0, created.text);
      const scheduleId = created.payload.schedule_id;
      assert.ok(scheduleId, created.text);

      // W2: the durable row is listable for that creator/preset.
      const listed = await jsonFetch(`${base}/v1/daemon/orchestration/schedules`);
      assert.equal(listed.status, 200, listed.text);
      const listedRow = listed.payload.items.find((row) => row.schedule_id === scheduleId);
      assert.ok(listedRow, `the created schedule must be listed: ${listed.text}`);
      assert.equal(listedRow.creator_id, CREATOR, listed.text);
      assert.equal(listedRow.preset_id, presetId, listed.text);
      assert.ok(listed.payload.pagination, listed.text);

      // W4 + S0-2: admission is asynchronous, and the owned run identity lands
      // on that durable row; the run then parks at the preset's manual wait.
      const admitted = await waitFor(
        () => inspectSchedule(),
        (payload) => Boolean(payload.schedule.current_session_id),
        { label: `schedule ${scheduleId} owned run identity` },
      );
      const runId = admitted.schedule.current_session_id;
      const parked = await waitFor(() => inspectSession(runId), (payload) => payload.session.status === 'waiting_for_input', {
        label: `run ${runId} manual wait`,
      });
      assert.equal(parked.session.session_id, runId, parked && JSON.stringify(parked));
      assert.equal(parked.session.creator_id, CREATOR, JSON.stringify(parked));
      assert.equal(parked.session.preset_id, presetId, JSON.stringify(parked));

      // W3: the overlay list is the DURABLE run set of this creator.
      const sessions = await jsonFetch(`${base}/v1/daemon/orchestration/sessions`);
      assert.equal(sessions.status, 200, sessions.text);
      const sessionRow = sessions.payload.items.find((row) => row.session_id === runId);
      assert.ok(sessionRow, `the admitted run must be listed: ${sessions.text}`);
      assert.equal(sessionRow.creator_id, CREATOR, sessions.text);
      assert.equal(sessionRow.preset_id, presetId, sessions.text);
      assert.equal(sessionRow.status, 'waiting_for_input', sessions.text);

      // W4: inspect reports the durable status, the owned run and the context
      // version of THAT schedule.
      const inspected = await inspectSchedule();
      assert.equal(inspected.schedule.schedule_id, scheduleId, JSON.stringify(inspected));
      assert.equal(inspected.schedule.status, 'running', JSON.stringify(inspected));
      assert.equal(inspected.schedule.current_session_id, runId, JSON.stringify(inspected));
      assert.equal(inspected.schedule.current_core_context_version, 0, JSON.stringify(inspected));
      assert.deepEqual(inspected.depends_on, [], JSON.stringify(inspected));
      assert.equal(inspected.concurrency_kind, 'serial', JSON.stringify(inspected));

      // W5: every refused append is a typed client refusal that leaves the
      // durable version and status untouched — a failed append is a failed
      // Steer, never a half-applied version and never a resume.
      for (const body of [{}, { op: 'bogus', body: 'x' }, { op: 'struct_merge' }]) {
        const refused = await jsonFetch(`${scheduleUrl(scheduleId)}/core-context`, {
          method: 'PATCH',
          body,
        });
        assert.equal(refused.status, 400, `${JSON.stringify(body)}: ${refused.text}`);
        assert.equal(refused.payload.error.code, 'invalid_input', refused.text);
      }
      const refusedAppend = await jsonFetch(`${scheduleUrl(scheduleId)}/core-context`, {
        method: 'PATCH',
        body: { op: 'replace', body: 'a user edit never overwrites system context' },
      });
      assert.equal(refusedAppend.status, 400, refusedAppend.text);
      const afterRefusal = await inspectSchedule();
      assert.equal(
        afterRefusal.schedule.current_core_context_version,
        0,
        `a refused append must not write a version: ${JSON.stringify(afterRefusal)}`,
      );
      assert.equal(afterRefusal.schedule.status, 'running', JSON.stringify(afterRefusal));

      // W5: the real Steer append is durable BEFORE any resume counts.
      const appended = await jsonFetch(`${scheduleUrl(scheduleId)}/core-context`, {
        method: 'PATCH',
        body: { op: 'append', body: 'P0-T6 steer idea' },
      });
      assert.equal(appended.status, 200, appended.text);
      assert.equal(appended.payload.new_version, 1, appended.text);
      const afterAppend = await inspectSchedule();
      assert.equal(
        afterAppend.schedule.current_core_context_version,
        1,
        `the appended version must be the durable pointer: ${JSON.stringify(afterAppend)}`,
      );

      // W6: a plain resume must not bypass the run's manual wait. The exact
      // durable conflict comes back (409 + coded detail), the durable append
      // stays, and NO second workflow is minted for the schedule.
      const resumed = await jsonFetch(`${scheduleUrl(scheduleId)}/signal`, {
        method: 'POST',
        body: { signal: 'resume' },
      });
      assert.equal(resumed.status, 409, resumed.text);
      assert.equal(
        resumed.payload.error.details?.wire_code,
        'workflow_state_conflict',
        resumed.text,
      );
      const afterResume = await inspectSchedule();
      assert.equal(afterResume.schedule.current_session_id, runId, JSON.stringify(afterResume));
      assert.equal(afterResume.schedule.current_core_context_version, 1, JSON.stringify(afterResume));
      assert.equal(afterResume.schedule.status, 'running', JSON.stringify(afterResume));
      const sessionsAfterResume = await jsonFetch(`${base}/v1/daemon/orchestration/sessions`);
      assert.equal(
        sessionsAfterResume.payload.items.filter((row) => row.preset_id === presetId).length,
        1,
        `resume must never create a second workflow: ${sessionsAfterResume.text}`,
      );

      // W7: cancel settles the SAME run (never a provider acknowledgement),
      // and the durable identity survives the terminal settlement.
      const cancelled = await jsonFetch(`${scheduleUrl(scheduleId)}/signal`, {
        method: 'POST',
        body: { signal: 'cancel' },
      });
      assert.equal(cancelled.status, 200, cancelled.text);
      assert.equal(cancelled.payload.status, 'cancelled', cancelled.text);
      const settled = await waitFor(
        () => inspectSchedule(),
        (payload) => payload.schedule.status === 'cancelled',
        { label: `schedule ${scheduleId} durable cancel settlement` },
      );
      assert.equal(settled.schedule.current_session_id, runId, JSON.stringify(settled));
      const settledSession = await inspectSession(runId);
      assert.equal(settledSession.session.status, 'cancelled', JSON.stringify(settledSession));

      // The append on a terminal schedule is the state conflict, and the
      // cancelled row stays inspectable (never a fabricated empty list).
      const terminalAppend = await jsonFetch(`${scheduleUrl(scheduleId)}/core-context`, {
        method: 'PATCH',
        body: { op: 'append', body: 'too late' },
      });
      assert.equal(terminalAppend.status, 409, terminalAppend.text);
      assert.equal(
        terminalAppend.payload.error.details?.wire_code,
        'workflow_state_conflict',
        terminalAppend.text,
      );
      const finalList = await jsonFetch(`${base}/v1/daemon/orchestration/schedules`);
      assert.ok(
        finalList.payload.items.some(
          (row) => row.schedule_id === scheduleId && row.status === 'cancelled',
        ),
        `the cancelled schedule stays durable: ${finalList.text}`,
      );
    } finally {
      await service.close();
    }
  });

  test('control ownership: foreign and absent ids close, foreign filters refuse, malformed input is typed', async () => {
    const home = seededHome(acpProviderConfig());
    seedForeignControlRows(home);
    const service = await startServiceOn(home);
    try {
      const base = service.url;

      // Both foreign rows are inside this workspace store, so a read that
      // ignored the stored owner would serve them.
      const schedules = await jsonFetch(`${base}/v1/daemon/orchestration/schedules`);
      assert.equal(schedules.status, 200, schedules.text);
      assert.ok(
        !schedules.payload.items.some((row) => row.schedule_id === FOREIGN_SCHEDULE_ID),
        `a foreign schedule must never be listed: ${schedules.text}`,
      );
      assert.deepEqual(
        [...new Set(schedules.payload.items.map((row) => row.creator_id))].filter(
          (creator) => creator !== CREATOR,
        ),
        [],
        `list stays scoped to the admitted creator: ${schedules.text}`,
      );

      const sessions = await jsonFetch(`${base}/v1/daemon/orchestration/sessions`);
      assert.equal(sessions.status, 200, sessions.text);
      assert.ok(
        !sessions.payload.items.some((row) => row.session_id === FOREIGN_SESSION_ID),
        `a foreign run must never be listed: ${sessions.text}`,
      );

      // A foreign id closes EXACTLY like an absent one: same status, same
      // code, no payload and no existence signal.
      for (const id of [FOREIGN_SCHEDULE_ID, 'SCH_absent_control']) {
        const response = await jsonFetch(`${base}/v1/daemon/orchestration/schedules/${id}`);
        assert.equal(response.status, 404, response.text);
        assert.equal(response.payload.error.code, 'not_found', response.text);
      }
      for (const id of [FOREIGN_SESSION_ID, 'sess_absent_control']) {
        const response = await jsonFetch(`${base}/v1/daemon/orchestration/sessions/${id}`);
        assert.equal(response.status, 404, response.text);
        assert.equal(response.payload.error.code, 'not_found', response.text);
      }

      // An explicit foreign creator filter is refused BEFORE any query runs —
      // never answered with a silently empty page.
      for (const family of ['schedules', 'sessions']) {
        const response = await jsonFetch(
          `${base}/v1/daemon/orchestration/${family}?creator_id=${FOREIGN_CREATOR}`,
        );
        assert.equal(response.status, 403, response.text);
        assert.equal(response.payload.error.code, 'forbidden', response.text);
      }

      // Malformed query input is the typed client refusal.
      const badLimit = await jsonFetch(`${base}/v1/daemon/orchestration/schedules?limit=abc`);
      assert.equal(badLimit.status, 400, badLimit.text);
      assert.equal(badLimit.payload.error.code, 'invalid_input', badLimit.text);
      const badSort = await jsonFetch(
        `${base}/v1/daemon/orchestration/schedules?sort=not_a_sort_key`,
      );
      assert.equal(badSort.status, 400, badSort.text);
      assert.equal(badSort.payload.error.details?.field, 'sort', badSort.text);
      const badSessionSort = await jsonFetch(
        `${base}/v1/daemon/orchestration/sessions?sort=not_a_sort_key`,
      );
      assert.equal(badSessionSort.status, 400, badSessionSort.text);
      assert.equal(badSessionSort.payload.error.details?.field, 'sort', badSessionSort.text);

      // An UNSUPPORTED or misspelled query key is NOT silently discarded. The
      // generated query DTOs are `additionalProperties: false`, so a key this
      // surface does not forward can never be answered as a broader
      // unfiltered page — that would make a caller believe a filter it asked
      // for was applied. It closes exactly like any other malformed input:
      // the typed client refusal, on BOTH list identities.
      for (const family of ['schedules', 'sessions']) {
        for (const query of ['preset_id=p0t6-control', 'limt=1', 'staus=running']) {
          const response = await jsonFetch(
            `${base}/v1/daemon/orchestration/${family}?${query}`,
          );
          assert.equal(response.status, 400, `${family}?${query}: ${response.text}`);
          assert.equal(response.payload.error.code, 'invalid_input', response.text);
        }
      }

      // …while every SUPPORTED key still reaches the core owner: the explicit
      // own-creator filter is accepted (the foreign-creator refusal above
      // proves it is forwarded, not dropped), and the page size the owner
      // echoes is the one that was asked for rather than the default.
      for (const family of ['schedules', 'sessions']) {
        const own = await jsonFetch(
          `${base}/v1/daemon/orchestration/${family}?creator_id=${CREATOR}&limit=1`,
        );
        assert.equal(own.status, 200, own.text);
        assert.equal(own.payload.pagination.limit, 1, own.text);
      }

      // Control on an unknown schedule is the not-found refusal, with no
      // mutation of anything else.
      const unknownSignal = await jsonFetch(`${base}/v1/daemon/orchestration/schedules/SCH_absent_control/signal`, {
        method: 'POST',
        body: { signal: 'cancel' },
      });
      assert.equal(unknownSignal.status, 404, unknownSignal.text);
      assert.equal(unknownSignal.payload.error.code, 'not_found', unknownSignal.text);
    } finally {
      await service.close();
    }
  });

  /**
   * W6 (S0-4): the public SUCCESS branch of the resume journey.
   *
   * The round-trip case above pins the refusal half — a plain resume must not
   * bypass a run parked at a MANUAL human wait. This case pins the other half:
   * a run durably parked at a converge gate carries no human wait, so the
   * schedule's own run is genuinely resumable. The row is `pause`d first (the
   * row flips while the run it owns keeps its identity), the resume signals
   * that SAME run over HTTP, the response carries the durable RUN status, and
   * both public projections read the reconciled row — with no second run
   * minted.
   *
   * The graph is authored through the PUBLIC preset surface (scaffold →
   * validated PATCH of its YAML), so the preset, the schedule, its admission
   * and the run are all producer-made; nothing here seeds the store privately
   * and no mock acknowledges anything.
   */
  test('control resume: a paused schedule row resumes the same parked run over HTTP', async () => {
    const home = seededHome(acpProviderConfig());
    const service = await startServiceOn(home);
    try {
      const base = service.url;
      const schedulesUrl = `${base}/v1/daemon/orchestration/schedules`;
      const scheduleUrl = (id) => `${schedulesUrl}/${id}`;
      const sessionUrl = (id) => `${base}/v1/daemon/orchestration/sessions/${id}`;
      const inspectSchedule = async (id) => {
        const response = await jsonFetch(scheduleUrl(id));
        assert.equal(response.status, 200, response.text);
        return response.payload;
      };
      const inspectSession = async (id) => {
        const response = await jsonFetch(sessionUrl(id));
        assert.equal(response.status, 200, response.text);
        return response.payload;
      };

      // W1: author the graph through the public preset surface. The scaffold
      // creates the user bundle; the validated PATCH replaces its YAML with a
      // converge gate whose second upstream branch is never walked, so the
      // admitted run parks durably at that gate (no human wait, no deadline).
      const scaffolded = await jsonFetch(`${base}/v1/daemon/presets`, {
        method: 'POST',
        body: { name: RESUME_PARK_PRESET },
      });
      assert.equal(scaffolded.status, 201, scaffolded.text);
      const patched = await jsonFetch(`${base}/v1/daemon/presets/${RESUME_PARK_PRESET}`, {
        method: 'PATCH',
        body: { yaml: resumeParkPresetYaml() },
      });
      assert.equal(patched.status, 200, patched.text);
      assert.equal(patched.payload.updated, true, patched.text);

      const created = await jsonFetch(schedulesUrl, {
        method: 'POST',
        body: { creator_id: CREATOR, preset_id: RESUME_PARK_PRESET },
      });
      assert.equal(created.status, 201, created.text);
      assert.equal(created.payload.status, 'pending', created.text);
      const scheduleId = created.payload.schedule_id;
      assert.ok(scheduleId, created.text);

      // Admission is asynchronous by contract: the run identity lands on the
      // durable row, then the run parks at the gate.
      const admitted = await waitFor(
        () => inspectSchedule(scheduleId),
        (payload) => Boolean(payload.schedule.current_session_id),
        { label: `schedule ${scheduleId} owned run identity` },
      );
      const runId = admitted.schedule.current_session_id;
      const parked = await waitFor(
        () => inspectSession(runId),
        (payload) => payload.session.status === 'paused',
        { label: `run ${runId} converge-gate park` },
      );
      assert.equal(parked.session.session_id, runId, JSON.stringify(parked));
      assert.equal(parked.session.creator_id, CREATOR, JSON.stringify(parked));
      assert.equal(parked.session.preset_id, RESUME_PARK_PRESET, JSON.stringify(parked));

      // W6 state: `pause` flips the durable schedule ROW while the run it owns
      // keeps its identity, so the row claims `paused` while the run is still
      // the parked one this schedule owns.
      const paused = await jsonFetch(`${scheduleUrl(scheduleId)}/signal`, {
        method: 'POST',
        body: { signal: 'pause' },
      });
      assert.equal(paused.status, 200, paused.text);
      assert.equal(paused.payload.status, 'paused', paused.text);
      const afterPause = await inspectSchedule(scheduleId);
      assert.equal(afterPause.schedule.status, 'paused', JSON.stringify(afterPause));
      assert.equal(afterPause.schedule.current_session_id, runId, JSON.stringify(afterPause));

      // W6: the resume reaches that SAME run, and the response carries the
      // durable RUN status — never a synthesized success.
      const resumed = await jsonFetch(`${scheduleUrl(scheduleId)}/signal`, {
        method: 'POST',
        body: { signal: 'resume' },
      });
      assert.equal(resumed.status, 200, resumed.text);
      assert.equal(resumed.payload.schedule_id, scheduleId, resumed.text);
      assert.equal(resumed.payload.status, 'running', resumed.text);

      // The durable row follows the run it owns: both public projections read
      // the reconciled state, the identity is unchanged, and no second
      // workflow was minted for the schedule.
      const afterResume = await inspectSchedule(scheduleId);
      assert.equal(afterResume.schedule.status, 'running', JSON.stringify(afterResume));
      assert.equal(afterResume.schedule.current_session_id, runId, JSON.stringify(afterResume));
      const resumedSession = await inspectSession(runId);
      assert.equal(resumedSession.session.status, 'running', JSON.stringify(resumedSession));

      const listed = await jsonFetch(schedulesUrl);
      assert.equal(listed.status, 200, listed.text);
      const listedRow = listed.payload.items.find((row) => row.schedule_id === scheduleId);
      assert.ok(listedRow, `the resumed schedule stays listed: ${listed.text}`);
      assert.equal(listedRow.status, 'running', listed.text);
      assert.equal(listedRow.current_session_id, runId, listed.text);

      const sessions = await jsonFetch(`${base}/v1/daemon/orchestration/sessions`);
      assert.equal(sessions.status, 200, sessions.text);
      assert.equal(
        sessions.payload.items.filter((row) => row.preset_id === RESUME_PARK_PRESET).length,
        1,
        `a resume must never mint a second workflow: ${sessions.text}`,
      );
    } finally {
      await service.close();
    }
  });

  /**
   * W5 (S0-3): the next execution boundary reads the HTTP-appended version.
   *
   * The append is made over the public HTTP surface against a schedule parked
   * at a LEGAL boundary (the row is `paused` — it owns no run yet), and the
   * resume is what mints/drives that schedule's one run. The next execution
   * boundary is therefore the admission claim, which re-reads the committed
   * pointer and freezes the version's payload into the run — the exact boundary
   * the contract names. The observation is the boundary's OWN output: the real
   * `acp.prompt` capability renders `{{core_context.text}}` from the run's
   * frozen context and the fixture peer logs the prompt it received, so the
   * fixture log distinguishes "consumed version 1" from "kept the version-0
   * seed" (an empty body) rather than re-reading the version counter.
   *
   * The capacity holder is what makes the ordering deterministic: a running
   * serial predecessor holds the creator's capacity, so the boundary schedule
   * cannot be admitted early and the append is unambiguously before its first
   * boundary.
   */
  test('control steer: the next execution boundary consumes the HTTP-appended context version', async () => {
    const fixtureLogName = 'acp-boundary.jsonl';
    const home = seededHome((dir) =>
      acpProviderConfig({ fixtureLog: join(dir, fixtureLogName) }),
    );
    const fixtureLog = join(home, fixtureLogName);
    const readBoundaryPrompts = () =>
      existsSync(fixtureLog)
        ? readFileSync(fixtureLog, 'utf8')
            .split('\n')
            .filter((line) => line.trim().length > 0)
            .map((line) => JSON.parse(line))
            .filter((entry) => entry.event === 'prompt')
            .map((entry) => entry.prompt)
        : [];

    const service = await startServiceOn(home);
    try {
      const base = service.url;
      const schedulesUrl = `${base}/v1/daemon/orchestration/schedules`;
      const scheduleUrl = (id) => `${schedulesUrl}/${id}`;
      const inspectSchedule = async (id) => {
        const response = await jsonFetch(scheduleUrl(id));
        assert.equal(response.status, 200, response.text);
        return response.payload;
      };

      // The capacity holder: the scaffold graph opens the workspace and parks
      // at its manual wait, so it owns the creator's serial capacity without
      // any model request.
      const holderScaffold = await jsonFetch(`${base}/v1/daemon/presets`, {
        method: 'POST',
        body: { name: STEER_HOLDER_PRESET },
      });
      assert.equal(holderScaffold.status, 201, holderScaffold.text);
      const holderCreated = await jsonFetch(schedulesUrl, {
        method: 'POST',
        body: { creator_id: CREATOR, preset_id: STEER_HOLDER_PRESET },
      });
      assert.equal(holderCreated.status, 201, holderCreated.text);
      const holderId = holderCreated.payload.schedule_id;
      await waitFor(
        () => inspectSchedule(holderId),
        (payload) => Boolean(payload.schedule.current_session_id),
        { label: `holder schedule ${holderId} admission` },
      );

      // W1: author the boundary graph through the public preset surface.
      const scaffolded = await jsonFetch(`${base}/v1/daemon/presets`, {
        method: 'POST',
        body: { name: STEER_BOUNDARY_PRESET },
      });
      assert.equal(scaffolded.status, 201, scaffolded.text);
      const patched = await jsonFetch(`${base}/v1/daemon/presets/${STEER_BOUNDARY_PRESET}`, {
        method: 'PATCH',
        body: { yaml: steerBoundaryPresetYaml() },
      });
      assert.equal(patched.status, 200, patched.text);
      assert.equal(patched.payload.updated, true, patched.text);

      const created = await jsonFetch(schedulesUrl, {
        method: 'POST',
        body: {
          creator_id: CREATOR,
          preset_id: STEER_BOUNDARY_PRESET,
          concurrency: 'serial',
          agent_bindings: { default: { provider_id: 'mock-acp' } },
        },
      });
      assert.equal(created.status, 201, created.text);
      assert.equal(created.payload.status, 'pending', created.text);
      assert.equal(created.payload.core_context_version, 0, created.text);
      const scheduleId = created.payload.schedule_id;

      // The serial gate holds the boundary row unadmitted, so the parked row
      // below is the row's REAL state rather than a winning race: it owns no
      // run, and no boundary prompt has run yet.
      await delay(1_500);
      const blocked = await inspectSchedule(scheduleId);
      assert.equal(blocked.schedule.status, 'pending', JSON.stringify(blocked));
      assert.equal(
        blocked.schedule.current_session_id,
        undefined,
        `the serial gate must hold the row unadmitted: ${JSON.stringify(blocked)}`,
      );
      assert.deepEqual(
        readBoundaryPrompts(),
        [],
        'no execution boundary may run before the append',
      );

      // W5 boundary: the row is parked at a legal boundary (paused, no run).
      const paused = await jsonFetch(`${scheduleUrl(scheduleId)}/signal`, {
        method: 'POST',
        body: { signal: 'pause' },
      });
      assert.equal(paused.status, 200, paused.text);
      assert.equal(paused.payload.status, 'paused', paused.text);

      // W5: the HTTP append commits the next immutable version.
      const appended = await jsonFetch(`${scheduleUrl(scheduleId)}/core-context`, {
        method: 'PATCH',
        body: { op: 'append', body: STEER_IDEA },
      });
      assert.equal(appended.status, 200, appended.text);
      assert.equal(appended.payload.new_version, 1, appended.text);
      const afterAppend = await inspectSchedule(scheduleId);
      assert.equal(afterAppend.schedule.current_core_context_version, 1, JSON.stringify(afterAppend));
      assert.equal(afterAppend.schedule.current_session_id, undefined, JSON.stringify(afterAppend));

      // Free the capacity, then resume: the resume crosses the next execution
      // boundary — the admission claim of this schedule's ONE run.
      const holderCancelled = await jsonFetch(`${scheduleUrl(holderId)}/signal`, {
        method: 'POST',
        body: { signal: 'cancel' },
      });
      assert.equal(holderCancelled.status, 200, holderCancelled.text);
      const resumed = await jsonFetch(`${scheduleUrl(scheduleId)}/signal`, {
        method: 'POST',
        body: { signal: 'resume' },
      });
      assert.equal(resumed.status, 200, resumed.text);
      assert.equal(resumed.payload.status, 'running', resumed.text);

      // THE OBSERVATION: the boundary's own rendered prompt. A boundary that
      // did not read the committed version renders the empty version-0 seed
      // (`<marker>||END`), so this equality is the discriminating assertion.
      const prompts = await waitFor(readBoundaryPrompts, (entries) => entries.length > 0, {
        label: 'the resumed schedule boundary prompt',
      });
      assert.deepEqual(
        prompts,
        [`${STEER_BOUNDARY_MARKER}|${STEER_IDEA}|END`],
        `the next execution boundary must render the committed version: ${JSON.stringify(prompts)}`,
      );

      // The run identity is the schedule's own and stays single: the resume
      // minted no second workflow, and inspect still reads the committed
      // version.
      const settled = await waitFor(
        () => inspectSchedule(scheduleId),
        (payload) => payload.schedule.status === 'completed',
        { label: `schedule ${scheduleId} completion` },
      );
      const runId = settled.schedule.current_session_id;
      assert.ok(runId, JSON.stringify(settled));
      assert.equal(settled.schedule.current_core_context_version, 1, JSON.stringify(settled));

      const sessions = await jsonFetch(`${base}/v1/daemon/orchestration/sessions`);
      assert.equal(sessions.status, 200, sessions.text);
      const boundaryRuns = sessions.payload.items.filter(
        (row) => row.preset_id === STEER_BOUNDARY_PRESET,
      );
      assert.equal(
        boundaryRuns.length,
        1,
        `the resumed schedule owns exactly one run: ${sessions.text}`,
      );
      assert.equal(boundaryRuns[0].session_id, runId, sessions.text);
    } finally {
      await service.close();
    }
  });

  /**
   * W5 (S0-3) ACTIVE RUN: a version committed over HTTP while a run is already
   * ADMITTED and mid-execution is consumed at that SAME run's next outer state
   * transition.
   *
   * This is the distinct boundary the admission case above cannot reach: there
   * the append precedes the schedule's first admission, so the run is frozen
   * WITH version 1. Here the run is admitted at version 0, its first state
   * boundary renders the version-0 seed and is held IN FLIGHT by the fixture's
   * prompt gate (`ACP_FIXTURE_PROMPT_GATE_DIR`), and the append commits version
   * 1 while that state is still executing. The fixture records the prompt it
   * received BEFORE it blocks, so the ordering is evidenced rather than slept
   * on: the append provably lands after admission and after state `first`
   * started, and before state `second` starts.
   *
   * The observation is each boundary's OWN output — the peer's request log
   * records the prompt it actually received (the snapshot's
   * `{{core_context.text}}` body) — so a boundary that did not read the
   * committed pointer renders the empty version-0 seed (`||END`) and fails the
   * assertion. The case also holds the second edit while state `second` is in
   * flight, proving a state's snapshot is stable for its own duration while the
   * NEXT boundary picks the edit up; and it asserts one run, one identity, and
   * no re-append (the pointer advances exactly once per committed edit).
   */
  test('control steer: an append made during an admitted run lands at its next state boundary', async () => {
    const fixtureLogName = 'acp-active-context.jsonl';
    const gateDirName = 'acp-active-gate';
    const home = seededHome((dir) => {
      mkdirSync(join(dir, gateDirName), { recursive: true });
      return acpProviderConfig({
        fixtureLog: join(dir, fixtureLogName),
        env: { ACP_FIXTURE_PROMPT_GATE_DIR: join(dir, gateDirName) },
      });
    });
    const fixtureLog = join(home, fixtureLogName);
    const gateDir = join(home, gateDirName);
    const readPrompts = () =>
      existsSync(fixtureLog)
        ? readFileSync(fixtureLog, 'utf8')
            .split('\n')
            .filter((line) => line.trim().length > 0)
            .map((line) => JSON.parse(line))
            .filter((entry) => entry.event === 'prompt')
            .map((entry) => entry.prompt)
        : [];
    // Release the n-th recorded prompt; the peer resumes that exact step.
    const releasePrompt = (n) => writeFileSync(join(gateDir, `release-${n}`), 'release');

    const service = await startServiceOn(home);
    try {
      const base = service.url;
      const schedulesUrl = `${base}/v1/daemon/orchestration/schedules`;
      const scheduleUrl = (id) => `${schedulesUrl}/${id}`;
      const inspectSchedule = async (id) => {
        const response = await jsonFetch(scheduleUrl(id));
        assert.equal(response.status, 200, response.text);
        return response.payload;
      };
      const inspectSession = async (id) => {
        const response = await jsonFetch(`${base}/v1/daemon/orchestration/sessions/${id}`);
        assert.equal(response.status, 200, response.text);
        return response.payload;
      };
      const appendContext = async (id, body) =>
        jsonFetch(`${scheduleUrl(id)}/core-context`, {
          method: 'PATCH',
          body: { op: 'append', body },
        });

      // W1: author the three-boundary graph through the public preset surface.
      const scaffolded = await jsonFetch(`${base}/v1/daemon/presets`, {
        method: 'POST',
        body: { name: ACTIVE_CONTEXT_PRESET },
      });
      assert.equal(scaffolded.status, 201, scaffolded.text);
      const patched = await jsonFetch(`${base}/v1/daemon/presets/${ACTIVE_CONTEXT_PRESET}`, {
        method: 'PATCH',
        body: { yaml: activeContextPresetYaml() },
      });
      assert.equal(patched.status, 200, patched.text);
      assert.equal(patched.payload.updated, true, patched.text);

      const created = await jsonFetch(schedulesUrl, {
        method: 'POST',
        body: {
          creator_id: CREATOR,
          preset_id: ACTIVE_CONTEXT_PRESET,
          agent_bindings: { default: { provider_id: 'mock-acp' } },
        },
      });
      assert.equal(created.status, 201, created.text);
      assert.equal(created.payload.core_context_version, 0, created.text);
      const scheduleId = created.payload.schedule_id;

      // Admission: the run is ADMITTED (its identity lands on the durable row)
      // and its first state boundary renders the frozen version-0 seed.
      const admitted = await waitFor(
        () => inspectSchedule(scheduleId),
        (payload) => Boolean(payload.schedule.current_session_id),
        { label: `schedule ${scheduleId} owned run identity` },
      );
      const runId = admitted.schedule.current_session_id;
      const firstPrompt = `${ACTIVE_CONTEXT_MARKER}:first||END`;
      const firstPrompts = await waitFor(readPrompts, (entries) => entries.length >= 1, {
        label: `run ${runId} first state boundary prompt`,
      });
      assert.deepEqual(
        firstPrompts,
        [firstPrompt],
        `state 'first' renders the frozen version-0 seed: ${JSON.stringify(firstPrompts)}`,
      );

      // W5: the append commits version 1 while state 'first' is IN FLIGHT. It
      // must not disturb the admitted run: same identity, same durable
      // projection, same single prompt (the in-flight state is untouched).
      const inFlight = await inspectSession(runId);
      assert.equal(inFlight.session.status, 'running', JSON.stringify(inFlight));
      const appended = await appendContext(scheduleId, ACTIVE_CONTEXT_IDEA);
      assert.equal(appended.status, 200, appended.text);
      assert.equal(appended.payload.new_version, 1, appended.text);
      const afterAppend = await inspectSchedule(scheduleId);
      assert.equal(afterAppend.schedule.current_core_context_version, 1, JSON.stringify(afterAppend));
      assert.equal(afterAppend.schedule.current_session_id, runId, JSON.stringify(afterAppend));
      assert.equal(afterAppend.schedule.status, 'running', JSON.stringify(afterAppend));
      const stillInFlight = await inspectSession(runId);
      assert.deepEqual(
        stillInFlight,
        inFlight,
        'the committed edit must not mutate the in-flight run',
      );
      assert.deepEqual(readPrompts(), [firstPrompt], 'the in-flight state is not re-rendered');

      // THE OBSERVATION: release the in-flight state; the SAME run's NEXT
      // boundary renders the version the append committed.
      releasePrompt(1);
      const secondPrompt = `${ACTIVE_CONTEXT_MARKER}:second|${ACTIVE_CONTEXT_IDEA}|END`;
      const secondPrompts = await waitFor(readPrompts, (entries) => entries.length >= 2, {
        label: `run ${runId} second state boundary prompt`,
      });
      assert.deepEqual(
        secondPrompts,
        [firstPrompt, secondPrompt],
        `the next state boundary reads the committed version: ${JSON.stringify(secondPrompts)}`,
      );

      // W5: a second edit committed while state 'second' is in flight must not
      // alter THAT state's snapshot — it lands at the boundary after it.
      const secondEdit = await appendContext(scheduleId, ACTIVE_CONTEXT_IDEA_2);
      assert.equal(secondEdit.status, 200, secondEdit.text);
      assert.equal(secondEdit.payload.new_version, 2, secondEdit.text);
      const midSecond = await inspectSchedule(scheduleId);
      assert.equal(midSecond.schedule.current_core_context_version, 2, JSON.stringify(midSecond));
      assert.equal(midSecond.schedule.current_session_id, runId, JSON.stringify(midSecond));

      releasePrompt(2);
      const thirdPrompt = `${ACTIVE_CONTEXT_MARKER}:third|${ACTIVE_CONTEXT_IDEA}${ACTIVE_CONTEXT_IDEA_2}|END`;
      const thirdPrompts = await waitFor(readPrompts, (entries) => entries.length >= 3, {
        label: `run ${runId} third state boundary prompt`,
      });
      assert.deepEqual(
        thirdPrompts,
        [firstPrompt, secondPrompt, thirdPrompt],
        `state 'second' keeps its start-of-state snapshot and the newest edit lands at 'third': ${JSON.stringify(thirdPrompts)}`,
      );

      // The run settles on the SAME identity: one run, no re-append, no extra
      // boundary render, and the committed pointer is still the last edit.
      releasePrompt(3);
      const settled = await waitFor(
        () => inspectSchedule(scheduleId),
        (payload) => payload.schedule.status === 'completed',
        { label: `schedule ${scheduleId} completion` },
      );
      assert.equal(settled.schedule.current_session_id, runId, JSON.stringify(settled));
      assert.equal(settled.schedule.current_core_context_version, 2, JSON.stringify(settled));
      assert.deepEqual(
        readPrompts(),
        [firstPrompt, secondPrompt, thirdPrompt],
        'no boundary rendered twice and no version was re-appended',
      );

      const sessions = await jsonFetch(`${base}/v1/daemon/orchestration/sessions`);
      assert.equal(sessions.status, 200, sessions.text);
      const runs = sessions.payload.items.filter((row) => row.preset_id === ACTIVE_CONTEXT_PRESET);
      assert.equal(runs.length, 1, `the schedule owns exactly one run: ${sessions.text}`);
      assert.equal(runs[0].session_id, runId, sessions.text);
      const finalRun = await inspectSession(runId);
      assert.equal(finalRun.session.status, 'completed', JSON.stringify(finalRun));
    } finally {
      await service.close();
    }
  });

  /**
   * W7 (S0-6): an unconfirmed stop is durable `interrupted`, never a cancel
   * success.
   *
   * The run's owned Host session is made genuinely unconfirmable INSIDE the
   * close budget: the mock runtime delays its `shutdown` reply past the
   * configured per-close waiter deadline (`SHUTDOWN_DELAY_MS` above
   * `shutdown_ms`), which is the fixture's documented unconfirmed-close arm.
   * The prompt boundary still runs for real — the sealed deny-all recipe is the
   * only narrowed scope the dsh native provider serves for an orchestration
   * prompt, and its ordinary-harness close is the first close the budget
   * cannot confirm, so the boundary really does own a Host session whose
   * cleanup stays unconfirmable.
   *
   * The case then drives the ACTUAL public cancel path and asserts the durable
   * winner and the wire disposition: the response must carry the truthful
   * `interrupted` disposition — never `cancelled` — the run must be durably
   * `interrupted` with the actionable cleanup reason, and the schedule row must
   * NOT be promoted to `cancelled`.
   */
  test('control cancel: an unconfirmable owned cleanup stays durable interrupted, never cancelled', async () => {
    const home = seededHome((dir) =>
      dshNativeProviderConfig({
        REQ_LOG: join(dir, 'dsh-requests.jsonl'),
        DSH_HOME: join(dir, 'dsh-home'),
        SHUTDOWN_DELAY_MS: String(DSH_SHUTDOWN_DELAY_MS),
      }),
    );
    mkdirSync(join(home, 'dsh-home'), { recursive: true });

    const service = await startServiceOn(home);
    try {
      const base = service.url;
      const schedulesUrl = `${base}/v1/daemon/orchestration/schedules`;
      const scheduleUrl = (id) => `${schedulesUrl}/${id}`;
      const inspectSchedule = async (id) => {
        const response = await jsonFetch(scheduleUrl(id));
        assert.equal(response.status, 200, response.text);
        return response.payload;
      };
      const inspectSession = async (id) => {
        const response = await jsonFetch(`${base}/v1/daemon/orchestration/sessions/${id}`);
        assert.equal(response.status, 200, response.text);
        return response.payload;
      };

      const scaffolded = await jsonFetch(`${base}/v1/daemon/presets`, {
        method: 'POST',
        body: { name: UNCONFIRMED_CANCEL_PRESET },
      });
      assert.equal(scaffolded.status, 201, scaffolded.text);
      const patched = await jsonFetch(`${base}/v1/daemon/presets/${UNCONFIRMED_CANCEL_PRESET}`, {
        method: 'PATCH',
        body: { yaml: unconfirmedCancelPresetYaml() },
      });
      assert.equal(patched.status, 200, patched.text);

      const created = await jsonFetch(schedulesUrl, {
        method: 'POST',
        body: {
          creator_id: CREATOR,
          preset_id: UNCONFIRMED_CANCEL_PRESET,
          agent_bindings: { default: { provider_id: 'dsh-native' } },
        },
      });
      assert.equal(created.status, 201, created.text);
      const scheduleId = created.payload.schedule_id;

      // The boundary really ran and its owned session is retained: the run
      // parks at this preset's manual wait, which is only reached after the
      // prompt attempt settled. Cancelling from that state is the public
      // cancel of a non-terminal admitted workflow.
      const admitted = await waitFor(
        () => inspectSchedule(scheduleId),
        (payload) => Boolean(payload.schedule.current_session_id),
        { label: `schedule ${scheduleId} admission` },
      );
      const runId = admitted.schedule.current_session_id;
      const parked = await waitFor(
        () => inspectSession(runId),
        (payload) => payload.session.status === 'waiting_for_input',
        { label: `run ${runId} manual-wait park` },
      );
      assert.equal(parked.session.session_id, runId, JSON.stringify(parked));

      const cancelled = await jsonFetch(`${scheduleUrl(scheduleId)}/signal`, {
        method: 'POST',
        body: { signal: 'cancel' },
      });
      assert.equal(cancelled.status, 200, cancelled.text);
      // Never a success: the unconfirmed stop is reported as `interrupted`,
      // and specifically NOT as `cancelled`.
      assert.equal(cancelled.payload.status, 'interrupted', cancelled.text);
      assert.notEqual(cancelled.payload.status, 'cancelled', cancelled.text);

      // The durable winner is the interrupted run with its actionable cleanup
      // reason — not a fabricated cancelled row.
      const settledSession = await inspectSession(runId);
      assert.equal(settledSession.session.status, 'interrupted', JSON.stringify(settledSession));
      assert.match(
        String(settledSession.session.failure_reason),
        /unconfirmed/i,
        `the unconfirmed cleanup must stay actionable: ${JSON.stringify(settledSession)}`,
      );

      // The owning schedule row is NOT promoted to `cancelled` (the settlement
      // is a deliberate no-op for an unconfirmed cleanup) and keeps its run.
      const afterCancel = await inspectSchedule(scheduleId);
      assert.notEqual(afterCancel.schedule.status, 'cancelled', JSON.stringify(afterCancel));
      assert.equal(
        afterCancel.schedule.current_session_id,
        runId,
        JSON.stringify(afterCancel),
      );

      const listed = await jsonFetch(schedulesUrl);
      assert.equal(listed.status, 200, listed.text);
      const listedRow = listed.payload.items.find((row) => row.schedule_id === scheduleId);
      assert.ok(listedRow, `the interrupted schedule stays listed: ${listed.text}`);
      assert.notEqual(listedRow.status, 'cancelled', listed.text);

      // The unconfirmed outcome is ACTIONABLE, not a dead end (§3.4): the
      // retained cleanup owner is retried by a further cancel. Once the
      // provider's delayed close finally completes, the same public signal
      // confirms the stop and settles `cancelled` — which also releases the
      // owned Host session (and its process) instead of leaving a retained
      // cleanup running after the case ends.
      await waitFor(
        async () => {
          const retry = await jsonFetch(`${scheduleUrl(scheduleId)}/signal`, {
            method: 'POST',
            body: { signal: 'cancel' },
          });
          assert.equal(retry.status, 200, retry.text);
          return retry.payload.status;
        },
        (status) => status === 'cancelled',
        { label: `run ${runId} confirmed cancel retry`, timeout: 30_000 },
      );
      const confirmedSession = await inspectSession(runId);
      assert.equal(confirmedSession.session.status, 'cancelled', JSON.stringify(confirmedSession));
    } finally {
      await service.close();
    }
  });
});
