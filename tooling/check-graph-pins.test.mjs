// Regression for `check-graph-pins.sh` scoped cohorts and failure behavior.
//
// Every case below runs the REAL script against a REAL disposable Cargo
// workspace: no stubbed `cargo`, no canned tree output, no source-shape
// assertions. The fixtures use path-only dependencies, so the suite resolves
// offline and never touches the shared checkout's manifests or Cargo.lock.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
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

function manifestFor(name, { deps = {}, devDeps = {} } = {}) {
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
    'default = []',
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

function runGuard(args) {
  return spawnSync('bash', [GUARD, ...args], {
    cwd: REPO_ROOT,
    encoding: 'utf8',
    env: cargoEnv(),
    // QC3-S003: a wedged bash/cargo fails loudly instead of stalling the suite.
    timeout: 180_000,
    killSignal: 'SIGKILL',
  });
}

function domainRun(root) {
  return runGuard(['--cohort', 'domain', '--manifest-path', path.join(root, 'Cargo.toml')]);
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

function policyRun(root) {
  return runGuard(['--cohort', 'policy', '--manifest-path', path.join(root, 'Cargo.toml')]);
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
  assert.match(run.stdout, /└── nexus-wasm-host v/);
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
      // names the offending dependency in the dumped tree.
      assert.match(run.stdout, /must resolve with ZERO dependencies, got 2/);
      assert.match(run.stdout, new RegExp(`└── ${pkg} v`));
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
  assert.match(run.stdout, /└── nexus-wasm-host v/);
});
