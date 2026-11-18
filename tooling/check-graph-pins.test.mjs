// Regression for `check-graph-pins.sh` scoped cohorts and failure behavior.
//
// Every case below runs the REAL script against a REAL disposable Cargo
// workspace: no stubbed `cargo`, no canned tree output, no source-shape
// assertions. The fixtures use path-only dependencies, so the suite resolves
// offline and never touches the shared checkout's manifests or Cargo.lock.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const GUARD = path.join(path.dirname(fileURLToPath(import.meta.url)), 'check-graph-pins.sh');
const REPO_ROOT = path.resolve(path.dirname(GUARD), '..');

// The absence pins of the `domain` cohort, in script order.
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

// The absence pins of the `catalog` cohort, in script order.
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
  for (const [section, entries] of [
    ['dependencies', deps],
    ['dev-dependencies', devDeps],
  ]) {
    const names = Object.entries(entries);
    if (names.length === 0) continue;
    lines.push(`[${section}]`);
    // A dep value is either null or extra inline-table members (`, features = [..]`).
    for (const [dep, extra] of names) lines.push(`${dep} = { path = "../${dep}"${extra ?? ''} }`);
    lines.push('');
  }
  return lines.join('\n');
}

// Creates a workspace whose member directory equals its package name.
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
      // QC3-S003: a wedged cargo fails loudly instead of stalling the suite.
      timeout: 120_000,
      killSignal: 'SIGKILL',
    });
    assert.equal(lock.status, 0, `fixture lockfile generation failed: ${lock.stderr}`);
  }
  afterLock?.(root);
  return root;
}

// The real domain shape: core -> memory-provider -> spoke-operations, plus
// core's own direct spoke-operations edge.
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
    // QC3-S003: a wedged bash/cargo fails loudly instead of stalling the suite.
    timeout: 180_000,
    killSignal: 'SIGKILL',
  });
  // QC1-06 (partial, round 3): a timeout or signal must not look like an
  // ordinary nonzero refusal — `status: null` would satisfy `notEqual(…, 0)`
  // silently in the negative tests. Surface the abort condition explicitly.
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

// The real catalog shape: the domain edges plus the featureless spoke adapter
// core imports the tool grammar from. The adapter's own `default` turns the
// compute half on, so core's edge must pin `default-features = false` — that is
// what keeps the WASM plane out of the catalog cohort.
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
  // core keeps its direct spoke-operations edge and the package stays a
  // workspace member; only nexus-memory-provider -> spoke-operations is gone.
  // The forward half must stay green so the failure is attributable to the
  // provider's required edge, not to the whole graph having collapsed.
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
  // `cargo tree` includes dev-dependencies by default; the pins must not.
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
  // The positive controls assert a COUNT, so a collapsed graph cannot pass by
  // matching a zero-row "success" — the exact false-green the domain cohort
  // shipped before R3.
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
  // The provider is a workspace member but core holds no edge to it: the
  // exact-one probe resolves no row at all. The pin must fail and must never
  // print its `ok:` line, so the happy-path `ok:` assertions above can never be
  // satisfied by an empty result set.
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

// --- authoring cohort (v1.249 A3) ------------------------------------------

// The two shipped authoring selectors plus the core they forward to. The
// manifests are raw because the shape under test IS the feature wiring: a
// selector's forwarding edge, core's optional simulator dep and the plain
// store dep that gives each selection a non-empty census.
//
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
        // A stray direct simulator dep: the feature stays unselected, so only
        // the Connect-only row's package-absence probe can fail.
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
        // A non-optional simulator dep: the row that must fail is feature
        // containment, never the earlier exact-one probe.
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
  // Positive rows: both shipped selectors link the authoring surface.
  assert.match(run.stdout, /ok: nexus-core selects feature strategy-authoring for nexus42 <default>/);
  assert.match(run.stdout, /ok: nexus-core selects feature strategy-authoring for nexus-core-node <default>/);
  assert.match(run.stdout, /ok: nexus-workflow-simulate single version \(0\.1\.0\) for nexus42/);
  assert.match(run.stdout, /ok: nexus-workflow-simulate single version \(0\.1\.0\) for nexus-core-node/);
  // Negative rows: no non-authoring selection links the simulator, and each
  // carries its own positive census on the same root and selection.
  assert.match(run.stdout, /ok: nexus-core --features compute resolves \d+ distinct dependency package\(s\)/);
  assert.match(run.stdout, /ok: nexus-workflow-simulate absent from nexus-core --features compute/);
  assert.match(run.stdout, /ok: nexus-core --no-default-features resolves \d+ distinct dependency package\(s\)/);
  assert.match(run.stdout, /ok: nexus-workflow-simulate absent from nexus-core --no-default-features/);
  // The consumer rows resolve the CHECKED-IN fixture, not this workspace, so
  // the pinned core sets are the real consumer's (`test-hooks` is the checker's
  // own `observable` feature, selecting the lane-consultation counter). The
  // version is NOT pinned: it is the real package's, and a routine bump with an
  // unchanged feature set must stay green.
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
  // No plain dep and no compute leaf: `--features compute` resolves the root
  // alone, which `assert_empty` would otherwise report as "absent".
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
  // The compute leaf keeps that row non-empty, so the failure can only be the
  // no-default census.
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
  // The CLI row still printed its ok line, so the failure is attributable to
  // the native selector rather than to a collapsed fixture.
  assert.match(run.stdout, /ok: nexus-core selects feature strategy-authoring for nexus42/);
});

test('a native selector with the simulator but without the authoring feature fails on containment', () => {
  // The simulator is linked independently, so the earlier exact-one row passes
  // and the ONLY possible failure is feature containment. Without that check a
  // "selected the simulator some other way" graph would green the cohort.
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
  // Connect alone forwards core's authoring feature. The simulator comes with
  // it, so the feature probe must be the row's FIRST half and it must fire.
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
  // The package arrives without the feature, so the feature probe passes and
  // only the package-absence probe can catch the run-only row.
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
      // Valid at lock time, rejected during resolution: the guard must
      // propagate Cargo's verdict rather than read an empty tree as absence.
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
  // Proves the no-argument path is intact and that --manifest-path applies to
  // every probe: the fixture has no `nexus42`, so the first exact-one pin
  // fails there instead of silently passing an absent product cohort.
  const root = createWorkspace(domainCrates());
  const run = runGuard(['--manifest-path', path.join(root, 'Cargo.toml')]);

  assert.notEqual(run.status, 0);
  assert.match(run.stdout, /graph pin failed/);
  assert.match(run.stdout, /nexus42/);
});

// The forbidden edges of the `policy` cohort, in script order.
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
  // The five workspace-member public packages exist in the real workspace
  // (the standalone module-sdk/manifest/test crates are outside `-p` reach);
  // the fixture mirrors them as dependency-free leaves so the inverted
  // probes resolve.
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
  // QC-fix: the leaf pin runs widened (`--all-features --target all`), and
  // the eleven subsumed back-edge probes no longer print anything.
  assert.match(
    run.stdout,
    /ok: nexus-compute-policy --all-features --target all resolves with zero dependencies \(root only\)/,
  );
  assert.doesNotMatch(run.stdout, /absent from nexus-compute-policy/);
  assert.match(run.stdout, /ok: nexus-compute-policy absent from nexus-contracts --all-features --target all/);
  assert.match(run.stdout, /graph pins OK \(cohort: policy\)$/m);
});

test('a deliberate forbidden policy edge is rejected nonzero', () => {
  // Real-Cargo control: policy grows a real normal edge onto a WASM-plane
  // crate; the zero-dependency leaf pin must refuse through actual
  // resolution, not a string match. The positive root stays resolvable so
  // the failure is attributable to the forbidden edge alone — the refusal
  // names the offending dependency in its resolved tree.
  const root = createWorkspace({
    'nexus-compute-policy': { deps: { 'nexus-wasm-host': null } },
    'nexus-wasm-host': { deps: {} },
  });
  const run = policyRun(root);

  assert.notEqual(run.status, 0, 'a real policy -> nexus-wasm-host edge was accepted');
  assert.match(run.stdout, /graph pin failed/);
  assert.match(run.stdout, /nexus-compute-policy --all-features --target all must resolve with ZERO dependencies, got 2/);
  // The failure dump is a `--prefix none` tree: flat rows, no glyphs.
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
      // The zero-dependency pin refuses the edge through real resolution and
      // names the offending dependency in the dumped (prefix-none) tree.
      assert.match(run.stdout, /must resolve with ZERO dependencies, got 2/);
      assert.match(run.stdout, new RegExp(`${pkg} v0\\.1\\.0`));
    });
  }
});

test('a public package gaining a private-policy edge fails the cohort', () => {
  // compass D3: public module packages must gain no private dependency.
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
  // QC1-02 / QC2-F001: the optional edge only resolves under a NON-default
  // feature (`special`), so the pre-fix host-default selectors passed this
  // workspace; the widened `--all-features --target all` leaf pin must
  // refuse it. (Observed both ways: the same fixture passes exit 0 under
  // the pre-fix selectors — see task-2-report QC fix round.)
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
  // QC2-F004 (round 2 item B): with CARGO_TERM_UNICODE=false Cargo prints
  // children as `|-- pkg v…`; the round-1 glyph-stripping matcher ignored
  // those rows and the zero-dependency pin false-greened. The `--prefix
  // none` census is rendering-independent, so this test FAILS against the
  // old matcher (it asserted a green run) and passes against the new one.
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

  // Same workspace, Unicode rendering: identical verdict — the census does
  // not depend on rendering.
  const unicode = policyRun(root);
  assert.notEqual(unicode.status, 0, unicode.stdout);
  assert.match(unicode.stdout, /must resolve with ZERO dependencies, got 2/);
});

test('a target-scoped policy edge is caught by --target all', () => {
  // QC1-02 residue (round 2 item F): the edge exists only under
  // `cfg(windows)`, so host-target selectors never see it; the widened
  // `--target all` leaf pin must.
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
  // QC1-09 / QC2-F007 (round 3 blocker 1): the census's REAL cargo command
  // must fail, so the script exits through `fail` with the EXIT trap armed.
  // A valid graph (the round-2 shape) would let Cargo succeed, run the
  // success-path `rm`, clear the trap, and stay green even if the trap were
  // deleted — hiding exactly the regression this test exists to catch. A
  // valid workspace with NO lockfile under `--locked` is that failure.
  const guardTmp = mkdtempSync(path.join(tmpdir(), 'nexus-graph-pins-guard-tmp-'));
  // Registered with the shared teardown the moment it is created, so no
  // run (green or red) leaks the directory itself.
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
  // The refusal names the failed census probe, not an ordinary pin breach.
  assert.match(run.stdout, /graph probe for nexus-compute-policy .* failed \(exit \d+\)/);
  assert.match(run.stdout, /--locked/);
  assert.deepEqual(readdirSync(guardTmp), [], `temp files leaked: ${readdirSync(guardTmp)}`);
});

// The five engine-plane edges the enabled Connect library must NOT resolve
// (v1.249 C4), in script order.
const CONNECT_FORBIDDEN = [
  'nexus-core',
  'nexus-orchestration',
  'nexus-execution',
  'nexus-session-storage',
  'graph-flow',
];

// The real capability-extraction shape: the Connect library links the private
// capability crate plus the WS transport; the capability crate binds
// `nexus-wasm-host` only behind its `wasm` feature; the native host selects the
// factory's full feature set on core. `execution` forwards through
// `nexus-connect/peer-tools` (not `dep:`) so core keeps the implicit
// `nexus-connect` feature the native pin asserts.
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
        // v1.249 A3: the node host selects the authoring surface, so the
        // synthetic core declares what the real one forwards.
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
  // Positive controls assert a COUNT, so a collapsed graph cannot satisfy them.
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
      // The native row's `nexus-core → nexus-connect` edge would form a cycle
      // with the very edge under test, and this case must attribute the
      // failure to the Connect row, so the library-only shape drops the native
      // host and keeps core a leaf.
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
  // Control for the feature gate itself: with the host as a NORMAL dependency
  // the `--no-default-features` row (which runs first) is already red — a
  // feature cannot be the only thing keeping the host out.
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
  // The asymmetry the finding requires: `default = ["wasm"]` changes ONLY the
  // default selection. Probing both configurations with `--no-default-features`
  // would leave this fixture green; the separate default row is what catches it.
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
  // The no-default row ran earlier and stayed green...
  assert.match(run.stdout, /ok: nexus-wasm-host absent from nexus-capability --no-default-features/);
  // ...and the DEFAULT selection is the row that failed.
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
  // Declared features, zero dependencies: the absence pins would be vacuous.
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
