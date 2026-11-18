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
  return spawnSync('bash', [GUARD, ...args], { cwd: REPO_ROOT, encoding: 'utf8', env: cargoEnv() });
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
