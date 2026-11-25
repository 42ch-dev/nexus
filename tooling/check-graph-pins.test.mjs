//
// Every case below runs the REAL script against a REAL disposable Cargo
// workspace: no stubbed `cargo`, no canned tree output, no source-shape
// assertions — except the census temp-file case, which must read the script's
// `mktemp` call sites because a bare `mktemp` on BSD ignores TMPDIR, hides the
// census file from the guard dir and makes the cleanup case vacuous.
// The fixtures use path-only dependencies, so the suite resolves offline and
// never touches the shared checkout's manifests or Cargo.lock.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const GUARD = path.join(path.dirname(fileURLToPath(import.meta.url)), 'check-graph-pins.sh');
const REPO_ROOT = path.resolve(path.dirname(GUARD), '..');

const DOMAIN_FORBIDDEN = [
  'nexus-orchestration',
  'nexus-execution',
  'nexus-session-storage',
  'graph-flow',
  'nexus-agent-host',
  'nexus-acp-host',
  'nexus-wasm-host',
  'nexus-connect',
  'spoke-connect',
  'libp2p',
  'rmcp',
];

const CATALOG_FORBIDDEN = [
  'nexus-orchestration',
  'nexus-execution',
  'nexus-session-storage',
  'graph-flow',
  'nexus-agent-host',
  'nexus-acp-host',
  'nexus-wasm-host',
  'nexus-connect',
  'spoke-connect',
  'libp2p',
  'rmcp',
];

const scratchRoot = mkdtempSync(path.join(tmpdir(), 'nexus-graph-pins-scratch-'));
const targetDir = path.join(scratchRoot, 'target');
const fixtures = [];

after(() => {
  rmSync(scratchRoot, { recursive: true, force: true });
  for (const dir of fixtures) rmSync(dir, { recursive: true, force: true });
});

function cargoEnv() {
  return {
    ...process.env,
    CARGO_NET_OFFLINE: 'true',
    CARGO_TARGET_DIR: targetDir,
  };
}

function manifestFor(name, { deps = {}, devDeps = {}, features = ['default = []'] } = {}) {
  const lines = [
    '[package]',
    `name = "${name}"`,
    'version = "0.1.0"',
    'edition = "2021"',
    '',
    '[lib]',
    'path = "src/lib.rs"',
    '',
    '[features]',
    ...features,
    '',
  ];
  /** @type {Array<[string, Record<string, string | null>]>} */
  const sections = [
    ['dependencies', deps],
    ['dev-dependencies', devDeps],
  ];
  for (const [section, entries] of sections) {
    const names = Object.entries(entries);
    if (names.length === 0) continue;
    lines.push(`[${section}]`);
    for (const [dep, extra] of names) lines.push(`${dep} = { path = "../${dep}"${extra ?? ''} }`);
    lines.push('');
  }
  return lines.join('\n');
}

// `afterLock` rewrites a manifest that must be valid at lock time but invalid
// for the probe (Cargo rejects feature errors during resolution, which would
// otherwise also block `cargo generate-lockfile`).
function createWorkspace(crates, { lockfile = true, afterLock } = {}) {
  const root = mkdtempSync(path.join(tmpdir(), 'nexus-graph-pins-fixture-'));
  fixtures.push(root);
  writeFileSync(
    path.join(root, 'Cargo.toml'),
    `[workspace]\nresolver = "2"\nmembers = [\n${Object.keys(crates)
      .map((dir) => `  "${dir}",`)
      .join('\n')}\n]\n`,
  );
  for (const [dir, spec] of Object.entries(crates)) {
    mkdirSync(path.join(root, dir, 'src'), { recursive: true });
    writeFileSync(path.join(root, dir, 'Cargo.toml'), spec.manifest ?? manifestFor(dir, spec));
    writeFileSync(path.join(root, dir, 'src', 'lib.rs'), '');
  }
  const manifestPath = path.join(root, 'Cargo.toml');
  if (lockfile) {
    const lock = spawnSync('cargo', ['generate-lockfile', '--offline', '--manifest-path', manifestPath], {
      encoding: 'utf8',
      env: cargoEnv(),
      timeout: 120_000,
      killSignal: 'SIGKILL',
    });
    assert.equal(lock.status, 0, `fixture lockfile generation failed: ${lock.stderr}`);
  }
  afterLock?.(root);
  return root;
}

function domainCrates({ coreDeps = {}, coreDevDeps = {}, extra = {} } = {}) {
  return {
    'nexus-core': {
      deps: { 'nexus-memory-provider': null, 'spoke-operations': null, ...coreDeps },
      devDeps: coreDevDeps,
    },
    'nexus-memory-provider': { deps: { 'spoke-operations': null } },
    'spoke-operations': { deps: {} },
    ...extra,
  };
}

function runGuard(args, { env: envExtra } = {}) {
  const run = spawnSync('bash', [GUARD, ...args], {
    cwd: REPO_ROOT,
    encoding: 'utf8',
    env: { ...cargoEnv(), ...envExtra },
    timeout: 180_000,
    killSignal: 'SIGKILL',
  });
  if (run.error || run.signal) {
    throw new Error(
      `guard run aborted: error=${run.error ? (run.error.code ?? run.error.message) : 'none'} signal=${run.signal ?? 'none'}`,
    );
  }
  return run;
}

function domainRun(root) {
  return runGuard(['--cohort', 'domain', '--manifest-path', path.join(root, 'Cargo.toml')]);
}

function catalogCrates({ coreDeps = {}, extra = {} } = {}) {
  return domainCrates({
    coreDeps: { 'nexus-spoke-adapter': ', default-features = false', ...coreDeps },
    extra: {
      'nexus-spoke-adapter': {
        deps: {},
        features: ['default = ["compute"]', 'compute = []'],
      },
      ...extra,
    },
  });
}

function catalogRun(root) {
  return runGuard(['--cohort', 'catalog', '--manifest-path', path.join(root, 'Cargo.toml')]);
}

test('domain cohort passes on a domain-shaped workspace', () => {
  const root = createWorkspace(domainCrates());
  const run = domainRun(root);

  assert.equal(run.status, 0, `${run.stdout}\n${run.stderr}`);
  assert.match(run.stdout, /ok: nexus-core --no-default-features resolves 2 distinct dependency package\(s\)/);
  assert.match(run.stdout, /ok: nexus-memory-provider v0\.1\.0 feats=\[default\] for nexus-core/);
  assert.match(run.stdout, /ok: spoke-operations single version .* for nexus-core/);
  assert.match(run.stdout, /ok: spoke-operations single version .* for nexus-memory-provider/);
  assert.match(run.stdout, /ok: nexus-orchestration absent from nexus-core/);
  assert.match(run.stdout, /graph pins OK \(cohort: domain\)$/m);
});

test('a missing provider forwarding edge is refused', () => {
  const root = createWorkspace({
    'nexus-core': { deps: { 'nexus-memory-provider': null, 'spoke-operations': null } },
    'nexus-memory-provider': { deps: {} },
    'spoke-operations': { deps: {} },
  });
  const run = domainRun(root);

  assert.notEqual(run.status, 0, run.stdout);
  assert.match(run.stdout, /ok: spoke-operations single version .* for nexus-core/);
  assert.match(
    run.stdout,
    /spoke-operations expected in nexus-memory-provider--no-default-features graph, but absent/,
  );
});

test('each forbidden domain edge fails the cohort', async (t) => {
  for (const pkg of DOMAIN_FORBIDDEN) {
    await t.test(pkg, () => {
      const root = createWorkspace(
        domainCrates({ coreDeps: { [pkg]: null }, extra: { [pkg]: { deps: {} } } }),
      );
      const run = domainRun(root);

      assert.notEqual(run.status, 0, `a real core -> ${pkg} edge was accepted`);
      assert.match(run.stdout, /graph pin failed/);
      assert.match(run.stdout, new RegExp(`${pkg} must be MISSING`));
    });
  }
});

test('dev-only forbidden edge stays outside the pinned normal/build graph', () => {
  const root = createWorkspace(
    domainCrates({
      coreDevDeps: { 'nexus-orchestration': null },
      extra: { 'nexus-orchestration': { deps: {} } },
    }),
  );
  const run = domainRun(root);

  assert.equal(run.status, 0, `${run.stdout}\n${run.stderr}`);
  assert.match(run.stdout, /ok: nexus-orchestration absent from nexus-core/);
});

test('an empty positive control fails instead of vacuously greening', () => {
  const root = createWorkspace({ 'nexus-core': { deps: {}, devDeps: {} } });
  const run = domainRun(root);

  assert.notEqual(run.status, 0);
  assert.match(run.stdout, /absence pins would be vacuous/);
});

test('catalog cohort passes on a catalog-shaped workspace', () => {
  const root = createWorkspace(catalogCrates());
  const run = catalogRun(root);

  assert.equal(run.status, 0, `${run.stdout}\n${run.stderr}`);
  assert.match(run.stdout, /ok: nexus-core <default> resolves 3 distinct dependency package\(s\)/);
  assert.match(run.stdout, /ok: nexus-core v0\.1\.0 feats=\[default\] for nexus-core <default>/);
  assert.match(run.stdout, /ok: nexus-memory-provider v0\.1\.0 feats=\[default\] for nexus-core/);
  assert.match(run.stdout, /ok: nexus-spoke-adapter v0\.1\.0 feats=\[\] for nexus-core/);
  assert.match(run.stdout, /ok: spoke-operations single version .* for nexus-core/);
  assert.match(run.stdout, /ok: nexus-wasm-host absent from nexus-core/);
  assert.match(run.stdout, /ok: nexus-connect absent from nexus-core/);
  assert.match(run.stdout, /graph pins OK \(cohort: catalog\)$/m);
});

test('each forbidden catalog edge fails the cohort', async (t) => {
  for (const pkg of CATALOG_FORBIDDEN) {
    await t.test(pkg, () => {
      const root = createWorkspace(
        catalogCrates({ coreDeps: { [pkg]: null }, extra: { [pkg]: { deps: {} } } }),
      );
      const run = catalogRun(root);

      assert.notEqual(run.status, 0, `a real core -> ${pkg} edge was accepted`);
      assert.match(run.stdout, /graph pin failed/);
      assert.match(run.stdout, new RegExp(`${pkg} must be MISSING`));
    });
  }
});

test('catalog cohort fails when its graph is empty', () => {
  const root = createWorkspace({ 'nexus-core': { deps: {}, devDeps: {} } });
  const run = catalogRun(root);

  assert.notEqual(run.status, 0);
  assert.match(run.stdout, /absence pins would be vacuous/);
});

test('catalog cohort fails when a positive pin resolves no row', () => {
  const root = createWorkspace({
    'nexus-core': { deps: { 'spoke-operations': null, 'nexus-spoke-adapter': null } },
    'nexus-memory-provider': { deps: { 'spoke-operations': null } },
    'nexus-spoke-adapter': { deps: {} },
    'spoke-operations': { deps: {} },
  });
  const run = catalogRun(root);

  assert.notEqual(run.status, 0);
  assert.match(run.stdout, /graph pin failed/);
  assert.match(run.stdout, /nexus-memory-provider/);
  assert.doesNotMatch(run.stdout, /ok: nexus-memory-provider/);
});


// `plainDep` / `computeLeaf` exist so a case can make ONE negative root
// vacuous: the compute row needs a dep of its own, the no-default row needs a
// non-optional one, and each control must be provably able to fail.
function authoringCrates({
  nodeForwards = true,
  nodeIndependentSimulator = false,
  computeLinksSimulator = false,
  connectForwards = false,
  connectSimulatorDep = false,
  plainDep = true,
  computeLeaf = true,
} = {}) {
  const coreDeps = [
    ...(plainDep ? ['core-store = { path = "../core-store" }'] : []),
    'nexus-workflow-simulate = { path = "../nexus-workflow-simulate", optional = true }',
    ...(computeLeaf ? ['compute-leaf = { path = "../compute-leaf", optional = true }'] : []),
  ];
  const compute = computeLinksSimulator
    ? '["dep:nexus-workflow-simulate"]'
    : `[${computeLeaf ? '"dep:compute-leaf"' : ''}]`;
  const core = `[package]
name = "nexus-core"
version = "0.1.0"
edition = "2021"

[features]
default = []
execution = []
compute = ${compute}
strategy-authoring = ["execution", "dep:nexus-workflow-simulate"]

[dependencies]
${coreDeps.join('\n')}
`;
  const selector = (name, feature, forwards, extraDeps = '', connectHost = '[]') => `[package]
name = "${name}"
version = "0.1.0"
edition = "2021"

[features]
default = ["${feature}"]
${feature} = [${forwards ? '"nexus-core/strategy-authoring"' : ''}]
connect-host = ${connectHost}

[dependencies]
nexus-core = { path = "../nexus-core" }
${extraDeps}`;
  return {
    'nexus-core': { manifest: core },
    nexus42: {
      manifest: selector(
        'nexus42',
        'cli',
        true,
        connectSimulatorDep
          ? 'nexus-workflow-simulate = { path = "../nexus-workflow-simulate" }'
          : '',
        connectForwards ? '["nexus-core/strategy-authoring"]' : '[]',
      ),
    },
    'nexus-core-node': {
      manifest: selector(
        'nexus-core-node',
        'host',
        nodeForwards,
        nodeIndependentSimulator
          ? 'nexus-workflow-simulate = { path = "../nexus-workflow-simulate" }'
          : '',
      ),
    },
    'nexus-workflow-simulate': { deps: {} },
    ...(plainDep ? { 'core-store': { deps: {} } } : {}),
    ...(computeLeaf ? { 'compute-leaf': { deps: {} } } : {}),
  };
}

function authoringRun(root) {
  return runGuard(['--cohort', 'authoring', '--manifest-path', path.join(root, 'Cargo.toml')]);
}

test('authoring cohort passes and reads the checked-in consumer', () => {
  const root = createWorkspace(authoringCrates());
  const run = authoringRun(root);

  assert.equal(run.status, 0, `${run.stdout}\n${run.stderr}`);
  assert.match(run.stdout, /ok: nexus-core selects feature strategy-authoring for nexus42 <default>/);
  assert.match(run.stdout, /ok: nexus-core selects feature strategy-authoring for nexus-core-node <default>/);
  assert.match(run.stdout, /ok: nexus-workflow-simulate single version \(0\.1\.0\) for nexus42/);
  assert.match(run.stdout, /ok: nexus-workflow-simulate single version \(0\.1\.0\) for nexus-core-node/);
  assert.match(run.stdout, /ok: nexus-core --features compute resolves \d+ distinct dependency package\(s\)/);
  assert.match(run.stdout, /ok: nexus-workflow-simulate absent from nexus-core --features compute/);
  assert.match(run.stdout, /ok: nexus-core --no-default-features resolves \d+ distinct dependency package\(s\)/);
  assert.match(run.stdout, /ok: nexus-workflow-simulate absent from nexus-core --no-default-features/);
  assert.match(
    run.stdout,
    /ok: nexus-core v\S+ feats=\[execution,nexus-connect,operation-id,test-hooks\] for authoring-cohorts <default>/,
  );
  assert.match(
    run.stdout,
    /ok: nexus-core v\S+ feats=\[execution,nexus-connect,operation-id,strategy-authoring,test-hooks\] for authoring-cohorts --features authoring/,
  );
  assert.match(run.stdout, /ok: nexus-agent-host absent from authoring-cohorts --features authoring/);
  assert.match(run.stdout, /ok: nexus-wasm-host absent from authoring-cohorts --features authoring/);
  assert.match(run.stdout, /graph pins OK \(cohort: authoring\)$/m);
});

test('the compute negative row fails when its root graph is vacuous', () => {
  const root = createWorkspace(authoringCrates({ plainDep: false, computeLeaf: false }));
  const run = authoringRun(root);

  assert.notEqual(run.status, 0, run.stdout);
  assert.match(run.stdout, /graph pin failed/);
  assert.match(
    run.stdout,
    /nexus-core --features compute: resolved graph names only 1 package\(s\); the absence pins would be vacuous/,
  );
  assert.doesNotMatch(run.stdout, /ok: nexus-workflow-simulate absent from nexus-core --features compute/);
});

test('the no-default negative row fails when its root graph is vacuous', () => {
  const root = createWorkspace(authoringCrates({ plainDep: false }));
  const run = authoringRun(root);

  assert.notEqual(run.status, 0, run.stdout);
  assert.match(run.stdout, /ok: nexus-core --features compute resolves 1 distinct dependency package\(s\)/);
  assert.match(
    run.stdout,
    /nexus-core --no-default-features: resolved graph names only 1 package\(s\); the absence pins would be vacuous/,
  );
  assert.doesNotMatch(
    run.stdout,
    /ok: nexus-workflow-simulate absent from nexus-core --no-default-features/,
  );
});

test('dropping the native selector authoring edge fails the cohort', () => {
  const root = createWorkspace(authoringCrates({ nodeForwards: false }));
  const run = authoringRun(root);

  assert.notEqual(run.status, 0, run.stdout);
  assert.match(run.stdout, /graph pin failed/);
  assert.match(
    run.stdout,
    /nexus-workflow-simulate expected in nexus-core-node graph, but absent/,
  );
  assert.match(run.stdout, /ok: nexus-core selects feature strategy-authoring for nexus42/);
});

test('a native selector with the simulator but without the authoring feature fails on containment', () => {
  const root = createWorkspace(
    authoringCrates({ nodeForwards: false, nodeIndependentSimulator: true }),
  );
  const run = authoringRun(root);

  assert.notEqual(run.status, 0, run.stdout);
  assert.match(run.stdout, /ok: nexus-workflow-simulate single version \(0\.1\.0\) for nexus-core-node/);
  assert.match(
    run.stdout,
    /graph pin failed: nexus-core for nexus-core-node <default>: expected feature 'strategy-authoring'/,
  );
  assert.doesNotMatch(run.stdout, /ok: nexus-core selects feature strategy-authoring for nexus-core-node/);
});

test('a compute selection that links the simulator fails the cohort', () => {
  const root = createWorkspace(authoringCrates({ computeLinksSimulator: true }));
  const run = authoringRun(root);

  assert.notEqual(run.status, 0, run.stdout);
  assert.match(run.stdout, /graph pin failed/);
  assert.match(
    run.stdout,
    /nexus-workflow-simulate must be MISSING from nexus-core--features compute/,
  );
});

test('a Connect-only selection that forwards authoring fails the cohort', () => {
  const root = createWorkspace(authoringCrates({ connectForwards: true }));
  const run = authoringRun(root);

  assert.notEqual(run.status, 0, run.stdout);
  assert.match(run.stdout, /graph pin failed/);
  assert.match(
    run.stdout,
    /nexus-core for nexus42 --no-default-features --features connect-host: feature 'strategy-authoring' must stay unselected/,
  );
  assert.doesNotMatch(run.stdout, /ok: nexus42 keeps feature strategy-authoring unselected/);
});

test('a Connect-only selection that links the simulator directly fails the cohort', () => {
  const root = createWorkspace(authoringCrates({ connectSimulatorDep: true }));
  const run = authoringRun(root);

  assert.notEqual(run.status, 0, run.stdout);
  assert.match(run.stdout, /ok: nexus-core keeps feature strategy-authoring unselected for nexus42 --no-default-features --features connect-host/);
  assert.match(
    run.stdout,
    /nexus-workflow-simulate must be MISSING from nexus42--no-default-features --features connect-host/,
  );
});

test('unknown cohort is rejected before any Cargo probe', () => {
  const run = runGuard(['--cohort', 'runtime']);

  assert.notEqual(run.status, 0);
  assert.match(run.stdout, /unknown cohort 'runtime'/);
});

test('unknown argument is rejected', () => {
  const run = runGuard(['--watch']);

  assert.notEqual(run.status, 0);
  assert.match(run.stdout, /unknown argument '--watch'/);
});

test('missing manifest path is rejected', () => {
  const run = runGuard([
    '--cohort',
    'domain',
    '--manifest-path',
    path.join(scratchRoot, 'no-such-dir', 'Cargo.toml'),
  ]);

  assert.notEqual(run.status, 0);
  assert.match(run.stdout, /manifest not found/);
});

test('a manifest without the probed package fails', () => {
  const root = createWorkspace({ 'not-the-core': { deps: {}, devDeps: {} } });
  const run = domainRun(root);

  assert.notEqual(run.status, 0);
  assert.match(run.stdout, /graph pin failed/);
  assert.match(run.stdout, /nexus-core/);
});

test('an invalid feature dependency fails through real Cargo resolution', () => {
  const root = createWorkspace(domainCrates(), {
    afterLock: (dir) => {
      writeFileSync(
        path.join(dir, 'nexus-core', 'Cargo.toml'),
        manifestFor('nexus-core', {
          deps: {
            'nexus-memory-provider': ', features = ["does-not-exist"]',
            'spoke-operations': null,
          },
        }),
      );
    },
  });
  const run = domainRun(root);

  assert.notEqual(run.status, 0);
  assert.match(run.stdout, /graph pin failed/);
  assert.match(run.stdout, /does-not-exist/);
});

test('Cargo failure on a broken manifest propagates', () => {
  const root = createWorkspace(
    {
      ...domainCrates(),
      'nexus-core': { manifest: '[package\nname = "nexus-core"\n' },
    },
    { lockfile: false },
  );
  const run = domainRun(root);

  assert.notEqual(run.status, 0);
  assert.match(run.stdout, /graph pin failed/);
  assert.match(run.stdout, /failed to load manifest|unclosed table/);
});

test('the full matrix still runs without --cohort (manifest override reaches it)', () => {
  const root = createWorkspace(domainCrates());
  const run = runGuard(['--manifest-path', path.join(root, 'Cargo.toml')]);

  assert.notEqual(run.status, 0);
  assert.match(run.stdout, /graph pin failed/);
  assert.match(run.stdout, /nexus42/);
});

const POLICY_FORBIDDEN = [
  'nexus-orchestration',
  'nexus-execution',
  'nexus-session-storage',
  'graph-flow',
  'nexus-agent-host',
  'nexus-acp-host',
  'nexus-wasm-host',
  'nexus-connect',
  'spoke-connect',
  'libp2p',
  'rmcp',
];

function policyRun(root, opts) {
  return runGuard(['--cohort', 'policy', '--manifest-path', path.join(root, 'Cargo.toml')], opts);
}

test('policy cohort passes on a policy-shaped workspace', () => {
  const root = createWorkspace({
    'nexus-compute-policy': { deps: {} },
    'nexus-contracts': { deps: {} },
    'nexus-workflow-ir': { deps: {} },
    'nexus-pattern': { deps: {} },
    'nexus-knowledge': { deps: {} },
    'nexus-narrative': { deps: {} },
  });
  const run = policyRun(root);

  assert.equal(run.status, 0, `${run.stdout}\n${run.stderr}`);
  assert.match(
    run.stdout,
    /ok: nexus-compute-policy --all-features --target all resolves with zero dependencies \(root only\)/,
  );
  assert.doesNotMatch(run.stdout, /absent from nexus-compute-policy/);
  assert.match(run.stdout, /ok: nexus-compute-policy absent from nexus-contracts --all-features --target all/);
  assert.match(run.stdout, /graph pins OK \(cohort: policy\)$/m);
});

test('a deliberate forbidden policy edge is rejected nonzero', () => {
  const root = createWorkspace({
    'nexus-compute-policy': { deps: { 'nexus-wasm-host': null } },
    'nexus-wasm-host': { deps: {} },
  });
  const run = policyRun(root);

  assert.notEqual(run.status, 0, 'a real policy -> nexus-wasm-host edge was accepted');
  assert.match(run.stdout, /graph pin failed/);
  assert.match(run.stdout, /nexus-compute-policy --all-features --target all must resolve with ZERO dependencies, got 2/);
  assert.match(run.stdout, /nexus-wasm-host v0\.1\.0/);
});

test('each forbidden policy back-edge fails the cohort', async (t) => {
  for (const pkg of POLICY_FORBIDDEN) {
    await t.test(pkg, () => {
      const root = createWorkspace({
        'nexus-compute-policy': { deps: { [pkg]: null } },
        [pkg]: { deps: {} },
      });
      const run = policyRun(root);

      assert.notEqual(run.status, 0, `a real nexus-compute-policy -> ${pkg} edge was accepted`);
      assert.match(run.stdout, /graph pin failed/);
      assert.match(run.stdout, /must resolve with ZERO dependencies, got 2/);
      assert.match(run.stdout, new RegExp(`${pkg} v0\\.1\\.0`));
    });
  }
});

test('a public package gaining a private-policy edge fails the cohort', () => {
  const root = createWorkspace({
    'nexus-compute-policy': { deps: {} },
    'nexus-contracts': { deps: { 'nexus-compute-policy': null } },
  });
  const run = policyRun(root);

  assert.notEqual(run.status, 0, 'a real nexus-contracts -> nexus-compute-policy edge was accepted');
  assert.match(run.stdout, /graph pin failed/);
  assert.match(
    run.stdout,
    /nexus-compute-policy must be MISSING from nexus-contracts--all-features --target all graph/,
  );
});

test('a feature-scoped policy edge is caught by the widened selectors', () => {
  const root = createWorkspace({
    'nexus-compute-policy': {
      manifest: [
        '[package]',
        'name = "nexus-compute-policy"',
        'version = "0.1.0"',
        'edition = "2021"',
        '',
        '[lib]',
        'path = "src/lib.rs"',
        '',
        '[features]',
        'default = []',
        'special = ["dep:nexus-wasm-host"]',
        '',
        '[dependencies]',
        'nexus-wasm-host = { path = "../nexus-wasm-host", optional = true }',
        '',
      ].join('\n'),
    },
    'nexus-wasm-host': { deps: {} },
    'nexus-contracts': { deps: {} },
    'nexus-workflow-ir': { deps: {} },
    'nexus-pattern': { deps: {} },
    'nexus-knowledge': { deps: {} },
    'nexus-narrative': { deps: {} },
  });
  const run = policyRun(root);

  assert.notEqual(run.status, 0, 'a feature-scoped policy edge was accepted');
  assert.match(run.stdout, /graph pin failed/);
  assert.match(
    run.stdout,
    /nexus-compute-policy --all-features --target all must resolve with ZERO dependencies, got 2/,
  );
  assert.match(run.stdout, /nexus-wasm-host v0\.1\.0/);
});

test('ASCII rendering cannot hide a real dependency row from the census', () => {
  const root = createWorkspace({
    'nexus-compute-policy': { deps: { 'nexus-wasm-host': null } },
    'nexus-wasm-host': { deps: {} },
    'nexus-contracts': { deps: {} },
    'nexus-workflow-ir': { deps: {} },
    'nexus-pattern': { deps: {} },
    'nexus-knowledge': { deps: {} },
    'nexus-narrative': { deps: {} },
  });
  const ascii = policyRun(root, { env: { CARGO_TERM_UNICODE: 'false' } });

  assert.notEqual(ascii.status, 0, 'an ASCII-rendered dependency row was hidden from the census');
  assert.match(ascii.stdout, /graph pin failed/);
  assert.match(ascii.stdout, /must resolve with ZERO dependencies, got 2/);
  assert.match(ascii.stdout, /nexus-wasm-host v0\.1\.0/);

  const unicode = policyRun(root);
  assert.notEqual(unicode.status, 0, unicode.stdout);
  assert.match(unicode.stdout, /must resolve with ZERO dependencies, got 2/);
});

test('a target-scoped policy edge is caught by --target all', () => {
  const root = createWorkspace({
    'nexus-compute-policy': {
      manifest: [
        '[package]',
        'name = "nexus-compute-policy"',
        'version = "0.1.0"',
        'edition = "2021"',
        '',
        '[lib]',
        'path = "src/lib.rs"',
        '',
        '[features]',
        'default = []',
        '',
        "[target.'cfg(windows)'.dependencies]",
        'nexus-wasm-host = { path = "../nexus-wasm-host" }',
        '',
      ].join('\n'),
    },
    'nexus-wasm-host': { deps: {} },
    'nexus-contracts': { deps: {} },
    'nexus-workflow-ir': { deps: {} },
    'nexus-pattern': { deps: {} },
    'nexus-knowledge': { deps: {} },
    'nexus-narrative': { deps: {} },
  });
  const run = policyRun(root);

  assert.notEqual(run.status, 0, 'a cfg(windows)-scoped policy edge was accepted');
  assert.match(run.stdout, /graph pin failed/);
  assert.match(run.stdout, /must resolve with ZERO dependencies, got 2/);
  assert.match(run.stdout, /nexus-wasm-host v0\.1\.0/);
});

test('a failing census leaves no temp files behind', () => {
  const guardTmp = mkdtempSync(path.join(tmpdir(), 'nexus-graph-pins-guard-tmp-'));
  fixtures.push(guardTmp);
  const root = createWorkspace(
    {
      'nexus-compute-policy': { deps: { 'nexus-wasm-host': null } },
      'nexus-wasm-host': { deps: {} },
      'nexus-contracts': { deps: {} },
      'nexus-workflow-ir': { deps: {} },
      'nexus-pattern': { deps: {} },
      'nexus-knowledge': { deps: {} },
      'nexus-narrative': { deps: {} },
    },
    { lockfile: false },
  );
  const run = policyRun(root, { env: { TMPDIR: guardTmp } });

  assert.notEqual(run.status, 0);
  assert.match(run.stdout, /graph probe for nexus-compute-policy .* failed \(exit \d+\)/);
  assert.match(run.stdout, /--locked/);
  assert.deepEqual(readdirSync(guardTmp), [], `temp files leaked: ${readdirSync(guardTmp)}`);
});

// The case above only proves the trap deletes what mktemp created INSIDE
// guardTmp. A bare `mktemp` on BSD ignores TMPDIR, so the census file would
// live somewhere else, guardTmp would still be empty and the cleanup proof
// above would be vacuous. Every census call site must therefore hand mktemp an
// explicit `${TMPDIR}`-based template; the variable name, the length of the
// random run and trailing redirections stay free to change.

// First shell word after `mktemp` on the same physical line, or undefined when
// that word is not an argument at all. A quoted body is taken verbatim. A bare
// word that STARTS with a redirect (`2>"…"`, `> x`, `< x`) carries no
// template; a bare word that merely ends at one (`…XXXXXX</dev/null`) does —
// the shell ends an unquoted word at the first `<` or `>`. A comment or
// separator (`#`, `;`, `|`, `&`) still means no template. Text on later
// physical lines is never read, so a decoy parked inside a multi-line command
// substitution cannot supply one.
function mktempTemplate(line) {
  const marker = line.match(/\bmktemp\b/);
  if (!marker) return undefined;
  const rest = line.slice(marker.index + 'mktemp'.length);
  const quoted = /^\s*"([^"]*)"/.exec(rest) ?? /^\s*'([^']*)'/.exec(rest);
  if (quoted) return quoted[1];
  const word = (/^\s*([^\s)]*)/.exec(rest) ?? [])[1];
  if (!word || /^[0-9]*(?:>|<|&>)/.test(word) || /[#;|&]/.test(word)) return undefined;
  return word.split(/[<>]/)[0] || undefined;
}

test('every census mktemp call templates the temp file into TMPDIR', () => {
  const lines = readFileSync(GUARD, 'utf8')
    .split('\n')
    .filter((line) => !line.trimStart().startsWith('#'));

  let calls = 0;
  for (const line of lines) {
    if (!/\bmktemp\b/.test(line)) continue;
    calls += 1;
    const template = mktempTemplate(line);
    assert.ok(template, `mktemp needs an explicit template, not a redirection or comment: ${line.trim()}`);
    assert.match(template, /\$\{TMPDIR/, `census temp file must honour TMPDIR: ${line.trim()}`);
    assert.match(template, /nexus-graph-pins-census\./, `census temp file must use the census prefix: ${line.trim()}`);
  }
  assert.ok(calls > 0, 'the guard script calls mktemp nowhere; the census cleanup case cannot be proven');
});

const CONNECT_FORBIDDEN = [
  'nexus-core',
  'nexus-orchestration',
  'nexus-execution',
  'nexus-session-storage',
  'graph-flow',
];

function capabilityCrates({ connectDeps = {}, capabilityDeps = {}, extra = {} } = {}) {
  return {
    'nexus-connect': {
      features: [
        'default = []',
        'peer-tools = []',
        'connect-host = ["peer-tools"]',
        'connect-client = ["connect-host"]',
        'embedded-mcp = ["connect-client"]',
      ],
      deps: { 'nexus-capability': null, 'spoke-connect': null, ...connectDeps },
    },
    'nexus-capability': {
      features: ['default = []', 'wasm = ["dep:nexus-wasm-host"]'],
      deps: { 'nexus-preset': null, 'nexus-wasm-host': ', optional = true', ...capabilityDeps },
    },
    'nexus-preset': { deps: {} },
    'nexus-wasm-host': { deps: {} },
    'nexus-agent-host': { deps: {} },
    'spoke-connect': { deps: {} },
    'nexus-core': {
      features: [
        'default = []',
        'provider-host = []',
        'operation-id = []',
        'execution = ["operation-id", "nexus-connect/peer-tools"]',
        'compute = ["execution", "dep:nexus-wasm-host"]',
        'strategy-authoring = ["execution"]',
      ],
      deps: { 'nexus-connect': ', optional = true', 'nexus-wasm-host': ', optional = true' },
    },
    'nexus-core-node': {
      deps: {
        'nexus-capability': null,
        'nexus-wasm-host': null,
        'nexus-agent-host': null,
        'nexus-core':
          ', features = ["execution", "provider-host", "compute", "strategy-authoring"]',
      },
    },
    ...extra,
  };
}

function capabilityRun(root) {
  return runGuard(['--cohort', 'capability', '--manifest-path', path.join(root, 'Cargo.toml')]);
}

test('capability cohort passes on a capability-shaped workspace', () => {
  const root = createWorkspace(capabilityCrates());
  const run = capabilityRun(root);

  assert.equal(run.status, 0, `${run.stdout}\n${run.stderr}`);
  assert.match(run.stdout, /ok: nexus-connect --no-default-features --features connect-client,embedded-mcp resolves 3 distinct dependency package\(s\)/);
  assert.match(run.stdout, /ok: nexus-capability single version .* for nexus-connect/);
  assert.match(run.stdout, /ok: spoke-connect single version .* for nexus-connect/);
  assert.match(run.stdout, /ok: nexus-orchestration absent from nexus-connect/);
  assert.match(run.stdout, /ok: nexus-wasm-host absent from nexus-capability/);
  assert.match(run.stdout, /ok: nexus-wasm-host single version .* for nexus-capability --features wasm/);
  assert.match(run.stdout, /ok: nexus-capability single version .* for nexus-core-node/);
  assert.match(run.stdout, /ok: nexus-wasm-host single version .* for nexus-core-node/);
  assert.match(run.stdout, /graph pins OK \(cohort: capability\)$/m);
});

test('each forbidden Connect-plane edge fails the cohort', async (t) => {
  for (const pkg of CONNECT_FORBIDDEN) {
    await t.test(pkg, () => {
      const crates = capabilityCrates({ connectDeps: { [pkg]: null } });
      delete crates['nexus-core-node'];
      crates['nexus-core'] = { deps: {} };
      crates[pkg] ??= { deps: {} };
      const run = capabilityRun(createWorkspace(crates));

      assert.notEqual(run.status, 0, `a real nexus-connect -> ${pkg} edge was accepted`);
      assert.match(run.stdout, /graph pin failed/);
      assert.match(run.stdout, new RegExp(`${pkg} must be MISSING`));
    });
  }
});

test('an unconditional WASM edge fails the explicit no-default row first', () => {
  const run = capabilityRun(
    createWorkspace(
      capabilityCrates({
        extra: {
          'nexus-capability': {
            features: ['default = []', 'wasm = []'],
            deps: { 'nexus-preset': null, 'nexus-wasm-host': null },
          },
        },
      }),
    ),
  );

  assert.notEqual(run.status, 0, 'an unconditional capability -> nexus-wasm-host edge was accepted');
  assert.match(run.stdout, /graph pin failed/);
  assert.match(run.stdout, /nexus-wasm-host must be MISSING from nexus-capability--no-default-features/);
});

test('a `default`-selected WASM edge fails the default row only', () => {
  const run = capabilityRun(
    createWorkspace(
      capabilityCrates({
        extra: {
          'nexus-capability': {
            features: ['default = ["wasm"]', 'wasm = ["dep:nexus-wasm-host"]'],
            deps: { 'nexus-preset': null, 'nexus-wasm-host': ', optional = true' },
          },
        },
      }),
    ),
  );

  assert.notEqual(run.status, 0, 'a default-selected WASM edge was accepted');
  assert.match(run.stdout, /ok: nexus-wasm-host absent from nexus-capability --no-default-features/);
  assert.match(run.stdout, /graph pin failed/);
  assert.match(run.stdout, /nexus-wasm-host must be MISSING from nexus-capability graph/);
  assert.match(run.stdout, /found 1 entry/);
});

test('the capability cohort fails when the Connect library loses its capability edge', () => {
  const crates = capabilityCrates();
  delete crates['nexus-connect'].deps['nexus-capability'];
  const run = capabilityRun(createWorkspace(crates));

  assert.notEqual(run.status, 0);
  assert.match(run.stdout, /graph pin failed/);
  assert.match(run.stdout, /nexus-capability expected in nexus-connect.*but absent/);
});

test('the capability cohort fails when its graph is empty', () => {
  const run = capabilityRun(
    createWorkspace({
      'nexus-connect': {
        features: ['default = []', 'connect-client = []', 'embedded-mcp = ["connect-client"]'],
        deps: {},
      },
    }),
  );

  assert.notEqual(run.status, 0);
  assert.match(run.stdout, /absence pins would be vacuous/);
});


// The switches exist so ONE fixture case can make exactly one row fail:
// `forgotCompute` drops the explicit compute edge, `connectHostAddsCli` makes
// the selector imply the CLI cohort, `productCoreDep` drops the app's core edge
// (the product must never be core-free), and `productDeps` / `libraryDeps`
// inject one real forbidden edge.
function connectCrates({
  productDeps = {},
  libraryDeps = {},
  forgotCompute = false,
  connectHostAddsCli = false,
  productCoreDep = true,
} = {}) {
  const connectHost = [
    ...(productCoreDep ? ['"nexus-core/connect-host"'] : []),
    '"dep:nexus-connect"',
    '"nexus-connect/connect-host"',
    '"dep:spoke-connect"',
    '"dep:libp2p"',
    '"dep:nexus-spoke-adapter"',
    ...(forgotCompute ? [] : ['"nexus-spoke-adapter/compute"']),
    '"dep:nexus-wasm-host"',
    ...(connectHostAddsCli ? ['"cli"'] : []),
  ];
  const productDepLines = [
    ...(productCoreDep ? ['nexus-core = { path = "../nexus-core" }'] : []),
    'nexus-memory-provider = { path = "../nexus-memory-provider" }',
    'nexus-local-db = { path = "../nexus-local-db" }',
    'spoke-operations = { path = "../spoke-operations" }',
    'nexus-connect = { path = "../nexus-connect", optional = true }',
    'spoke-connect = { path = "../spoke-connect", optional = true }',
    'libp2p = { path = "../libp2p", optional = true }',
    'nexus-spoke-adapter = { path = "../nexus-spoke-adapter", default-features = false, optional = true }',
    'nexus-wasm-host = { path = "../nexus-wasm-host", optional = true }',
    ...Object.entries(productDeps).map(([dep, extra]) => `${dep} = { path = "../${dep}"${extra ?? ''} }`),
  ];
  const libraryDepLines = [
    'nexus-capability = { path = "../nexus-capability", optional = true }',
    'spoke-connect = { path = "../spoke-connect", optional = true }',
    'libp2p = { path = "../libp2p", optional = true }',
    'rmcp = { path = "../rmcp", optional = true }',
    ...Object.entries(libraryDeps).map(([dep, extra]) => `${dep} = { path = "../${dep}"${extra ?? ''} }`),
  ];
  const app = `[package]
name = "nexus42"
version = "0.1.0"
edition = "2021"

[features]
default = []
cli = []
connect-host = [${connectHost.join(', ')}]

[dependencies]
${productDepLines.join('\n')}
`;
  const core = `[package]
name = "nexus-core"
version = "0.1.0"
edition = "2021"

[features]
default = []
connect-host = ["nexus-connect/connect-host"]
operation-id = []

[dependencies]
nexus-connect = { path = "../nexus-connect", optional = true }
`;
  const connect = `[package]
name = "nexus-connect"
version = "0.1.0"
edition = "2021"

[features]
default = []
peer-tools = []
transport = []
connect-host = ["peer-tools", "transport", "dep:nexus-capability", "dep:spoke-connect", "dep:libp2p"]
connect-client = ["connect-host", "dep:rmcp"]
embedded-mcp = ["connect-client"]

[dependencies]
${libraryDepLines.join('\n')}
`;
  return {
    nexus42: { manifest: app },
    'nexus-core': { manifest: core },
    'nexus-connect': { manifest: connect },
    'nexus-memory-provider': {
      features: ['default = []', 'compute = []'],
      deps: { 'spoke-operations': null },
    },
    'nexus-local-db': { deps: {} },
    'spoke-operations': { deps: {} },
    'nexus-spoke-adapter': {
      features: ['default = []', 'compute = ["dep:nexus-wasm-host"]'],
      deps: { 'nexus-wasm-host': ', optional = true' },
    },
    'nexus-wasm-host': { deps: {} },
    'nexus-capability': { deps: {} },
    'spoke-connect': { deps: {} },
    libp2p: { deps: {} },
    rmcp: { deps: {} },
  };
}

function connectRun(root) {
  return runGuard(['--cohort', 'connect', '--manifest-path', path.join(root, 'Cargo.toml')]);
}

test('connect cohort passes on a connect-shaped workspace', () => {
  const run = connectRun(createWorkspace(connectCrates()));

  assert.equal(run.status, 0, `${run.stdout}\n${run.stderr}`);
  assert.match(
    run.stdout,
    /ok: nexus42 --no-default-features --features connect-host resolves \d+ distinct dependency package\(s\)/,
  );
  assert.match(run.stdout, /ok: nexus-core single version .* for nexus42 --no-default-features --features connect-host/);
  assert.match(run.stdout, /ok: nexus-memory-provider single version .* for nexus42/);
  assert.match(
    run.stdout,
    /ok: nexus-spoke-adapter v0\.1\.0 feats=\[compute\] for nexus42 --no-default-features --features connect-host/,
  );
  assert.match(run.stdout, /ok: nexus-wasm-host single version .* for nexus42/);
  assert.match(
    run.stdout,
    /ok: nexus42 v0\.1\.0 feats=\[connect-host\] for nexus42 --no-default-features --features connect-host/,
  );
  assert.match(run.stdout, /ok: rmcp absent from nexus42 --no-default-features --features connect-host/);
  assert.match(run.stdout, /ok: nexus-workflow-simulate absent from nexus42 --no-default-features --features connect-host/);
  assert.match(
    run.stdout,
    /ok: nexus-connect --no-default-features --features connect-client,embedded-mcp resolves \d+ distinct dependency package\(s\)/,
  );
  assert.match(run.stdout, /ok: nexus-capability single version .* for nexus-connect/);
  assert.match(run.stdout, /ok: rmcp single version .* for nexus-connect/);
  assert.match(
    run.stdout,
    /ok: nexus-core absent from nexus-connect --no-default-features --features connect-client,embedded-mcp/,
  );
  assert.match(run.stdout, /graph pins OK \(cohort: connect\)$/m);
});

const CONNECT_PRODUCT_FORBIDDEN = [
  'nexus-orchestration',
  'nexus-execution',
  'nexus-session-storage',
  'graph-flow',
  'nexus-workflow-simulate',
  'nexus-agent-host',
  'nexus-acp-host',
  'rmcp',
];

test('each forbidden Connect product edge fails the cohort', async (t) => {
  for (const pkg of CONNECT_PRODUCT_FORBIDDEN) {
    await t.test(pkg, () => {
      const crates = connectCrates({ productDeps: { [pkg]: null } });
      // `rmcp` and the engine crates already exist, the rest get a leaf.
      crates[pkg] ??= { deps: {} };
      const run = connectRun(createWorkspace(crates));

      assert.notEqual(run.status, 0, `a real nexus42 -> ${pkg} edge was accepted`);
      assert.match(run.stdout, /graph pin failed/);
      assert.match(
        run.stdout,
        new RegExp(`${pkg} must be MISSING from nexus42--no-default-features --features connect-host`),
      );
    });
  }
});

const CONNECT_LIBRARY_FORBIDDEN = [
  'nexus-core',
  'nexus-orchestration',
  'nexus-execution',
  'nexus-session-storage',
  'graph-flow',
];

test('each forbidden Connect library back-edge fails the cohort', async (t) => {
  for (const pkg of CONNECT_LIBRARY_FORBIDDEN) {
    await t.test(pkg, () => {
      const crates = connectCrates({ libraryDeps: { [pkg]: null } });
      crates['nexus-core'] = {
        features: ['default = []', 'connect-host = []', 'operation-id = []'],
        deps: {},
      };
      crates[pkg] ??= { deps: {} };
      const run = connectRun(createWorkspace(crates));

      assert.notEqual(run.status, 0, `a real nexus-connect -> ${pkg} edge was accepted`);
      assert.match(run.stdout, /graph pin failed/);
      assert.match(
        run.stdout,
        new RegExp(`${pkg} must be MISSING from nexus-connect--no-default-features --features connect-client,embedded-mcp`),
      );
    });
  }
});

test('the Connect product fails when the explicit spoke compute edge is dropped', () => {
  const run = connectRun(createWorkspace(connectCrates({ forgotCompute: true })));

  assert.notEqual(run.status, 0, 'a Connect product without explicit spoke compute was accepted');
  assert.match(run.stdout, /graph pin failed/);
  assert.match(
    run.stdout,
    /nexus-spoke-adapter for nexus42 --no-default-features --features connect-host: expected feats=\[compute\], got 'feats=\[\]'/,
  );
});

test('the Connect-only selector fails when it implies the CLI cohort', () => {
  const run = connectRun(createWorkspace(connectCrates({ connectHostAddsCli: true })));

  assert.notEqual(run.status, 0, 'a connect-host selector that implies cli was accepted');
  assert.match(run.stdout, /graph pin failed/);
  assert.match(
    run.stdout,
    /nexus42 for nexus42 --no-default-features --features connect-host: expected feats=\[connect-host\], got 'feats=\[cli,connect-host\]'/,
  );
});

test('the Connect product fails when core is dropped (never a core-free product)', () => {
  const run = connectRun(createWorkspace(connectCrates({ productCoreDep: false })));

  assert.notEqual(run.status, 0, 'a core-free Connect product was accepted');
  assert.match(run.stdout, /graph pin failed/);
  assert.match(
    run.stdout,
    /nexus-core expected in nexus42--no-default-features --features connect-host graph, but absent/,
  );
});

test('the connect cohort fails when its graph is empty', () => {
  const run = connectRun(
    createWorkspace({
      'nexus-connect': {
        features: ['default = []', 'connect-client = []', 'embedded-mcp = ["connect-client"]'],
        deps: {},
      },
      nexus42: { features: ['default = []', 'connect-host = []'], deps: {} },
    }),
  );

  assert.notEqual(run.status, 0);
  assert.match(run.stdout, /absence pins would be vacuous/);
});
