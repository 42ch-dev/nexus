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
