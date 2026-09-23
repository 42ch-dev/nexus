import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, writeFileSync } from 'node:fs';
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

function acpProviderConfig() {
  const python = resolvePython();
  return `
[[providers]]
id = "mock-acp"
protocol = "acp"
command = ${JSON.stringify(python)}
args = [${JSON.stringify(acpFixture)}]
enabled = true
`;
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
  writeFileSync(join(agentHostDir, 'config.toml'), providerConfig);
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
