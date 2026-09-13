#!/usr/bin/env node
/**
 * P3-T3 fixture verifier.
 *
 * Negative and positive checks for the shared evidence contract, the package
 * gate's entitlement predicate, the provider lifecycle predicate, and the
 * decision generator's evidence derivation. Everything here is synthetic: no
 * packaged app is launched and no real evidence is amended.
 *
 * Usage: node apps/desktop-electron/scripts/proof-contract-fixtures.mjs
 * Exit code 0 only when every check passes.
 */
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  CANONICAL_SAMPLES,
  RUNTIME_SCHEMA,
  decisionForState,
  evaluateRuntimeEvidence,
  providerLifecycleComplete,
} from './proof-contract.mjs';
import { compareEntitlements } from './proof-package.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..', '..');
const TMP = join('/tmp', `p3t3-fixtures-${process.pid}`);

/** Primary worktree root: the harness tree (`.mstar/**`) lives there, not here. */
function primaryRoot() {
  const common = spawnSync('git', ['rev-parse', '--path-format=absolute', '--git-common-dir'], {
    cwd: ROOT,
    encoding: 'utf8',
  }).stdout.trim();
  return common ? dirname(common) : ROOT;
}

const REAL_EVIDENCE = join(primaryRoot(), '.mstar', 'iterations', 'v1.189', 'guides', 'evidence');

let passed = 0;
let failed = 0;
const failures = [];

function check(name, actual, expected) {
  if (actual === expected) {
    passed += 1;
    return;
  }
  failed += 1;
  failures.push(`${name}: got=${JSON.stringify(actual)} want=${JSON.stringify(expected)}`);
  console.log(`FAIL ${name}: got=${JSON.stringify(actual)} want=${JSON.stringify(expected)}`);
}

// --- fixtures ---------------------------------------------------------------

const FIXTURE_APP = join(TMP, 'app', 'Nexus RFT Feasibility.app');
mkdirSync(join(FIXTURE_APP, 'Contents', 'MacOS'), { recursive: true });
writeFileSync(join(FIXTURE_APP, 'Contents', 'MacOS', 'Nexus RFT Feasibility'), 'not a real binary\n');
const FIXTURE_APP_REAL = realpathSync(FIXTURE_APP);

const PROVENANCE = {
  app_path: FIXTURE_APP_REAL,
  app_bundle_id: 'com.nexus42.rft-electron-proof',
  arch: 'arm64',
  app_bundle_sha256: 'a'.repeat(64),
  native_node_sha256: 'b'.repeat(64),
  electron_version: '44.3.0',
  packager_version: '20.3.0',
  source_sha: 'deadbeef',
  tree_digest: 'c'.repeat(64),
  tree_dirty: false,
  command: 'node scripts/proof-runtime.mjs',
  utc_start: '2026-09-13T00:00:00.000Z',
};

const EXPECT = { app_path: FIXTURE_APP_REAL, app_bundle_id: 'com.nexus42.rft-electron-proof', arch: 'arm64' };

const ms = (n, offset = 0) => Array.from({ length: n }, (_, i) => ({ index: i, ms: 100 + i + offset }));
const cycles = (n) => Array.from({ length: n }, (_, i) => 50 + i);
const trace = (n) => Array.from({ length: n }, (_, i) => ({ at: `t${i}`, rss_bytes: 300 * 1024 * 1024 + i }));

/** Nearest-rank summary, mirroring the contract's algorithm for fixture authoring. */
function summarise(values) {
  const numbers = [...values].filter(Number.isFinite).sort((a, b) => a - b);
  const nearestRank = (q) => numbers[Math.min(numbers.length - 1, Math.ceil(q * numbers.length) - 1)];
  return {
    count: numbers.length,
    min: numbers[0],
    p50: nearestRank(0.5),
    p95: nearestRank(0.95),
    max: numbers[numbers.length - 1],
  };
}

function completeDoc(overrides = {}) {
  const cold = ms(CANONICAL_SAMPLES.cold);
  const warm = ms(CANONICAL_SAMPLES.warm, 5);
  const cycleMs = cycles(CANONICAL_SAMPLES.cycles);
  const soakTrace = trace(300);
  const doc = {
    schema: RUNTIME_SCHEMA,
    mode: 'canonical',
    gating: true,
    phases_requested: ['launch', 'resources', 'security', 'lifecycle'],
    phases_executed: ['launch', 'resources', 'security', 'lifecycle'],
    launch: {
      samples: { cold, warm },
      summary: { cold: summarise(cold.map((e) => e.ms)), warm: summarise(warm.map((e) => e.ms)) },
      failures: 0,
    },
    resources: {
      soak: {
        soak_seconds: CANONICAL_SAMPLES.soakSeconds,
        trace: soakTrace,
        summary: summarise(soakTrace.map((e) => e.rss_bytes)),
        workload: { reads: 6000, writes: 1200, conflicts: 0, read_errors: 0, write_errors: [] },
      },
      cycles: {
        cycle_count: CANONICAL_SAMPLES.cycles,
        durations_ms: cycleMs,
        summary: summarise(cycleMs),
        samples: trace(11),
        plateau_after_cooldown: { rss_bytes: 300 * 1024 * 1024 },
        retained_growth_bytes: 1024,
        survivors_after_final_close: [],
      },
    },
    validity: { valid: true, launch_method: 'launchservices', confounders: [] },
    provenance: { ...PROVENANCE },
    native_utility_load: { ok: true },
    checks: [
      { id: 'START-1', ok: true },
      { id: 'RES-1', ok: true },
      { id: 'RES-2', ok: true },
      { id: 'SEC-renderer', ok: true },
      { id: 'LIFECYCLE-native', ok: true },
      { id: 'LIFECYCLE-fault', ok: true },
      { id: 'PKG-2-electron', ok: true },
    ],
    status: 'pass',
  };
  return { ...doc, ...overrides };
}

const state = (doc, expectations = EXPECT) => evaluateRuntimeEvidence(doc, expectations).state;

// --- contract: baseline -----------------------------------------------------

check('absent document is missing', state(null), 'missing');
check('non-object document is malformed', state('nope'), 'malformed');
check('wrong schema is malformed', state({ schema: 'v1' }), 'malformed');
check('no checks is malformed', state({ schema: RUNTIME_SCHEMA, checks: [] }), 'malformed');
check('complete document is valid-pass', state(completeDoc()), 'valid-pass');
check(
  'a failed check is valid-fail',
  state(completeDoc({ checks: completeDoc().checks.map((c) => (c.id === 'RES-1' ? { ...c, ok: false } : c)) })),
  'valid-fail',
);

// --- contract: C2 exact check-ID set ---------------------------------------

check(
  'duplicate check ID is incomplete',
  state(completeDoc({ checks: [...completeDoc().checks, { id: 'RES-1', ok: true }] })),
  'incomplete',
);
check(
  'duplicate cannot hide an earlier failure',
  state(
    completeDoc({
      checks: [{ id: 'RES-1', ok: false }, ...completeDoc().checks],
    }),
  ),
  'incomplete',
);
check(
  'extra check ID is incomplete',
  state(completeDoc({ checks: [...completeDoc().checks, { id: 'MADE-UP', ok: true }] })),
  'incomplete',
);
check(
  'missing check ID is incomplete',
  state(completeDoc({ checks: completeDoc().checks.filter((c) => c.id !== 'SEC-renderer') })),
  'incomplete',
);

// --- contract: C2 raw observations -----------------------------------------

check(
  'missing raw cold samples is incomplete',
  state(completeDoc({ launch: { ...completeDoc().launch, samples: { warm: ms(30) } } })),
  'incomplete',
);
check(
  'short raw cold sample array is incomplete',
  state(
    completeDoc({
      launch: { ...completeDoc().launch, samples: { cold: ms(9), warm: ms(30) } },
    }),
  ),
  'incomplete',
);
check(
  'null latency inside raw samples is incomplete',
  state(
    completeDoc({
      launch: { ...completeDoc().launch, samples: { cold: [{ index: 0, ms: null }, ...ms(9)], warm: ms(30) } },
    }),
  ),
  'incomplete',
);
check(
  'fabricated cold summary is incomplete',
  state(
    completeDoc({
      launch: {
        ...completeDoc().launch,
        summary: { cold: { count: 10, min: 1, p50: 1, p95: 1, max: 1 }, warm: completeDoc().launch.summary.warm },
      },
    }),
  ),
  'incomplete',
);
check(
  'inflated cold count in summary is incomplete',
  state(
    completeDoc({
      launch: {
        ...completeDoc().launch,
        summary: { cold: { ...completeDoc().launch.summary.cold, count: 999 }, warm: completeDoc().launch.summary.warm },
      },
    }),
  ),
  'incomplete',
);
check(
  'short raw cycle durations is incomplete',
  state(completeDoc({ resources: { ...completeDoc().resources, cycles: { ...completeDoc().resources.cycles, durations_ms: cycles(4) } } })),
  'incomplete',
);
check(
  'fabricated cycle summary is incomplete',
  state(
    completeDoc({
      resources: {
        ...completeDoc().resources,
        cycles: { ...completeDoc().resources.cycles, summary: { count: 100, min: 9, p50: 9, p95: 9, max: 9 } },
      },
    }),
  ),
  'incomplete',
);
check(
  'missing soak trace is incomplete',
  state(
    completeDoc({
      resources: { ...completeDoc().resources, soak: { ...completeDoc().resources.soak, trace: undefined } },
    }),
  ),
  'incomplete',
);
check(
  'wrong soak duration is incomplete',
  state(
    completeDoc({
      resources: { ...completeDoc().resources, soak: { ...completeDoc().resources.soak, soak_seconds: 10 } },
    }),
  ),
  'incomplete',
);
check(
  'fabricated soak summary is incomplete',
  state(
    completeDoc({
      resources: { ...completeDoc().resources, soak: { ...completeDoc().resources.soak, summary: { count: 300, min: 1, p50: 1, p95: 1, max: 1 } } },
    }),
  ),
  'incomplete',
);
check(
  'missing workload counters is incomplete',
  state(
    completeDoc({
      resources: { ...completeDoc().resources, soak: { ...completeDoc().resources.soak, workload: undefined } },
    }),
  ),
  'incomplete',
);

// --- contract: provenance / validity ---------------------------------------

check(
  'non-gating document is incomplete',
  state(completeDoc({ gating: false })),
  'incomplete',
);
check(
  'missing provenance field is incomplete',
  state(completeDoc({ provenance: { ...PROVENANCE, native_node_sha256: null } })),
  'incomplete',
);
check('different app path is stale', state(completeDoc(), { ...EXPECT, app_path: '/tmp/other.app' }), 'stale');
check('different bundle id is stale', state(completeDoc(), { ...EXPECT, app_bundle_id: 'com.example.other' }), 'stale');
check('different arch is stale', state(completeDoc(), { ...EXPECT, arch: 'x64' }), 'stale');
check(
  'direct-executable launch is confounded',
  state(
    completeDoc({
      validity: { valid: false, launch_method: 'direct', confounders: ['direct_executable_launch'] },
      checks: completeDoc().checks.map((c) => (c.id === 'LIFECYCLE-native' ? { ...c, ok: false } : c)),
    }),
  ),
  'confounded',
);
check(
  'unavailable utility owner is confounded, not a measured failure',
  state(
    completeDoc({
      validity: { valid: false, launch_method: 'launchservices', confounders: ['utility_owner_unavailable'] },
      checks: completeDoc().checks.map((c) => (c.id === 'LIFECYCLE-native' ? { ...c, ok: false } : c)),
    }),
  ),
  'confounded',
);
check('native load not proven is valid-fail', state(completeDoc({ native_utility_load: { ok: false } })), 'valid-fail');

// --- decision mapping -------------------------------------------------------

const ALL_SIG = { signedOk: true, stapleOk: true, hardenedOk: true };
check('valid-pass + all signature predicates is go', decisionForState('valid-pass', ALL_SIG), 'go');
check('valid-pass without signing is blocked', decisionForState('valid-pass', { signedOk: false, stapleOk: false, hardenedOk: false }), 'blocked');
check('valid-fail is no-go', decisionForState('valid-fail', ALL_SIG), 'no-go');
check('incomplete is blocked', decisionForState('incomplete', ALL_SIG), 'blocked');
check('stale is blocked', decisionForState('stale', ALL_SIG), 'blocked');
check('confounded is blocked', decisionForState('confounded', ALL_SIG), 'blocked');
check('missing is blocked', decisionForState('missing', ALL_SIG), 'blocked');
check('malformed is blocked', decisionForState('malformed', ALL_SIG), 'blocked');

// --- provider lifecycle completeness (I7) ----------------------------------

const completeOp = {
  steps: {
    probe: { ok: true },
    launch: { ok: true },
    execute: { ok: true },
    pull: { ok: true, deltas: 1, terminal_count: 1, terminal_matches_operation: true },
    cancel: { ok: true },
    shutdown: { ok: true },
  },
};
check('complete provider operation passes', providerLifecycleComplete(completeOp), true);
check(
  'failed pull fails the operation',
  providerLifecycleComplete({ steps: { ...completeOp.steps, pull: { ...completeOp.steps.pull, ok: false } } }),
  false,
);
check(
  'missing terminal event fails the operation',
  providerLifecycleComplete({ steps: { ...completeOp.steps, pull: { ...completeOp.steps.pull, terminal_count: 0 } } }),
  false,
);
check(
  'two terminal events fail the operation',
  providerLifecycleComplete({ steps: { ...completeOp.steps, pull: { ...completeOp.steps.pull, terminal_count: 2 } } }),
  false,
);
check(
  'terminal for another operation fails',
  providerLifecycleComplete({ steps: { ...completeOp.steps, pull: { ...completeOp.steps.pull, terminal_matches_operation: false } } }),
  false,
);
check(
  'no message delta fails the operation',
  providerLifecycleComplete({ steps: { ...completeOp.steps, pull: { ...completeOp.steps.pull, deltas: 0 } } }),
  false,
);
check(
  'failed shutdown fails the operation',
  providerLifecycleComplete({ steps: { ...completeOp.steps, shutdown: { ok: false } } }),
  false,
);

// --- entitlement predicate (I6) ---------------------------------------------

const EXPECTED_ENTITLEMENTS = {
  parsed: { 'com.apple.security.cs.allow-jit': true, 'com.apple.security.cs.allow-unsigned-executable-memory': true },
  path: '/expected.plist',
  source_sha256: 'd'.repeat(64),
};
const signedOk = (parsed, status = 0) =>
  compareEntitlements({ parsed, status, raw: '', plist_sha256: 'e'.repeat(64) }, EXPECTED_ENTITLEMENTS).pass;

check('identical entitlements pass', signedOk({ ...EXPECTED_ENTITLEMENTS.parsed }), true);
check(
  'changed entitlement value fails',
  signedOk({ ...EXPECTED_ENTITLEMENTS.parsed, 'com.apple.security.cs.allow-jit': false }),
  false,
);
check('extra entitlement key fails', signedOk({ ...EXPECTED_ENTITLEMENTS.parsed, 'com.apple.security.cs.disable-library-validation': true }), false);
check('missing entitlement key fails', signedOk({ 'com.apple.security.cs.allow-jit': true }), false);
check('unparseable entitlements fail', signedOk(null), false);
check('codesign failure fails', signedOk({ ...EXPECTED_ENTITLEMENTS.parsed }, 1), false);
check(
  'key order does not matter',
  compareEntitlements(
    { parsed: { b: 2, a: { d: 4, c: 3 } }, status: 0, raw: '' },
    { parsed: { a: { c: 3, d: 4 }, b: 2 }, path: '/x', source_sha256: 'f'.repeat(64) },
  ).pass,
  true,
);

// --- decision derivation (I5/I8) --------------------------------------------

const DECISION_FIXTURES = join(TMP, 'decision');

/** Run the decision generator, optionally against a substitute evidence root. */
function runDecisionWithRoot(rootArg) {
  const out = join(TMP, `decision-${rootArg ? 'fixture' : 'real'}.json`);
  const args = [join(__dirname, 'proof-decision.mjs'), '--out', out];
  if (rootArg) args.push('--evidence-root', rootArg);
  const res = spawnSync('node', args, { cwd: ROOT, encoding: 'utf8' });
  if (!existsSync(out)) return { status: 'no-output', stderr: res.stderr.slice(-400) };
  const doc = JSON.parse(readFileSync(out, 'utf8'));
  return { status: doc.status, doc };
}

// Real evidence root (the repo's own) — rows must derive from actual documents.
const realDecision = runDecisionWithRoot(null);
const realCounts = realDecision.doc?.row_counts ?? {};
check(
  'row counts reconcile',
  (realCounts.pass ?? 0) + (realCounts.fail ?? 0) + (realCounts.missing_or_unobserved ?? 0),
  realCounts.total,
);
check(
  'PKG-2 native payload is split per target',
  (realDecision.doc?.observed_rows ?? []).filter((r) => r.id.startsWith('PKG2-native-')).length,
  4,
);
check(
  'no aggregate native PKG-2 pass row remains',
  (realDecision.doc?.observed_rows ?? []).some((r) => r.id === 'PKG2-native'),
  false,
);
check(
  'every PASS row cites an existing verified document',
  (realDecision.doc?.observed_rows ?? []).filter((r) => r.verdict === 'PASS').every((r) => r.verification?.derived && r.verification.problems.length === 0 && r.verification.evidence_sha256),
  true,
);
check(
  'START/RES/MAINT rows are present and not passing',
  ['START-1', 'RES-1', 'RES-2', 'MAINT-1', 'SEC-renderer'].every((id) => {
    const row = (realDecision.doc?.observed_rows ?? []).find((r) => r.id === id);
    return row && row.verdict !== 'PASS';
  }),
  true,
);

// An evidence root with nothing in it: every derived row must fail closed.
const emptyRoot = join(DECISION_FIXTURES, 'empty', 'iterations', 'v1.189', 'guides', 'evidence');
mkdirSync(emptyRoot, { recursive: true });
const emptyDecision = runDecisionWithRoot(join(DECISION_FIXTURES, 'empty', 'iterations'));
check('absent evidence yields no PASS rows', emptyDecision.doc?.row_counts?.pass, 0);
check('absent evidence yields blocked', emptyDecision.status, 'blocked');

// A tampered receipt (status flipped to fail) must drop its row to non-PASS.
const tamperRoot = join(DECISION_FIXTURES, 'tamper', 'iterations');
const tamperEvidence = join(tamperRoot, 'v1.189', 'guides', 'evidence');
mkdirSync(join(tamperEvidence, 'install-macarm22'), { recursive: true });
const sourceEvidence = join(REAL_EVIDENCE, 'install-macarm22', 'install-proof.json');
check('tamper fixture source evidence is available', existsSync(sourceEvidence), true);
if (existsSync(sourceEvidence)) {
  const tampered = JSON.parse(readFileSync(sourceEvidence, 'utf8'));
  tampered.status = 'fail';
  writeFileSync(join(tamperEvidence, 'install-macarm22', 'install-proof.json'), JSON.stringify(tampered));
  const tamperedDecision = runDecisionWithRoot(tamperRoot);
  const row = (tamperedDecision.doc?.observed_rows ?? []).find((r) => r.id === 'PKG1-arm64-node22');
  check('tampered non-pass receipt drops the row from PASS', row && row.verdict !== 'PASS', true);
  check('tampered receipt row reports the reason', row?.verification?.problems?.length > 0, true);
}

rmSync(TMP, { recursive: true, force: true });

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) {
  for (const failure of failures) console.log(`- ${failure}`);
  process.exit(1);
}
