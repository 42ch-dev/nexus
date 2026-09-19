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
import { createHash } from 'node:crypto';
import { cpSync, existsSync, mkdirSync, readFileSync, realpathSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  CANONICAL_SAMPLES,
  RUNTIME_SCHEMA,
  decisionForState,
  digestAppBundle,
  evaluateRuntimeEvidence,
  providerLifecycleComplete,
  runtimeCriterionVerdict,
  sha256File,
  walkFiles,
  compareEntitlements,
} from './proof-contract.mjs';

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
    sample_plan: { cold: 10, warm: 30, cycles: 100, soak_seconds: 600 },
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
  state(
    completeDoc({
      status: 'fail',
      checks: completeDoc().checks.map((c) => (c.id === 'RES-1' ? { ...c, ok: false } : c)),
    }),
  ),
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
  'non-gating document is rejected',
  state(completeDoc({ gating: false })),
  'malformed',
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

// C6: confounders win over the `valid` boolean, and a valid canonical product
// result must have been launched through LaunchServices.
check(
  'C6: valid:true with a recorded confounder is confounded',
  state(
    completeDoc({
      validity: { valid: true, launch_method: 'launchservices', confounders: ['direct_executable_launch'] },
    }),
  ),
  'confounded',
);
check(
  'C6: valid:true with an unknown confounder is confounded',
  state(
    completeDoc({
      validity: { valid: true, launch_method: 'launchservices', confounders: ['something_new'] },
    }),
  ),
  'confounded',
);
check(
  'C6: valid:true with a direct launch method is confounded',
  state(completeDoc({ validity: { valid: true, launch_method: 'direct', confounders: [] } })),
  'confounded',
);
check(
  'C6: absent validity.valid is confounded, not a pass',
  state(completeDoc({ validity: { launch_method: 'launchservices', confounders: [] } })),
  'confounded',
);
check(
  'C6: valid:true, launchservices, empty confounders is a pass',
  state(completeDoc({ validity: { valid: true, launch_method: 'launchservices', confounders: [] } })),
  'valid-pass',
);
check(
  'C6: a valid pass cannot coexist with a non-launchservices launch method',
  state(
    completeDoc({
      validity: { valid: true, launch_method: undefined, confounders: [] },
    }),
  ),
  'confounded',
);
check(
  'native load not proven is valid-fail',
  state(completeDoc({ status: 'fail', native_utility_load: { ok: false } })),
  'valid-fail',
);

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
  'runtime criteria rows are present and not passing',
  ['START-1-arm64', 'RES-1-arm64', 'RES-2-arm64', 'SEC-renderer-arm64'].every((id) => {
    const row = (realDecision.doc?.observed_rows ?? []).find((r) => r.id === id);
    return row && row.verdict !== 'PASS';
  }),
  true,
);
check(
  'the maintenance row exists and is derived from a document',
  (realDecision.doc?.observed_rows ?? []).find((r) => r.id === 'MAINT-1')?.verification?.derived,
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
  const row = (tamperedDecision.doc?.observed_rows ?? []).find((r) => r.id === 'PKG1-install-arm64-node22');
  check('tampered non-pass receipt drops the row from PASS', row && row.verdict !== 'PASS', true);
  check('tampered receipt row reports the reason', row?.verification?.problems?.length > 0, true);
}

// --- C5: canonical mode / status / sample-plan consistency ------------------

const plan = { cold: 10, warm: 30, cycles: 100, soak_seconds: 600 };
check(
  'diagnostic mode is rejected',
  evaluateRuntimeEvidence({ ...completeDoc(), mode: 'diagnostic' }, EXPECT).state,
  'malformed',
);
check(
  'missing mode is rejected',
  evaluateRuntimeEvidence({ ...completeDoc(), mode: undefined }, EXPECT).state,
  'malformed',
);
check(
  'non-canonical sample_plan is rejected',
  evaluateRuntimeEvidence({ ...completeDoc(), sample_plan: { ...plan, cycles: 50 } }, EXPECT).state,
  'malformed',
);
check(
  'missing sample_plan is rejected',
  evaluateRuntimeEvidence({ ...completeDoc(), sample_plan: undefined }, EXPECT).state,
  'malformed',
);
check(
  'status diagnostic with canonical shape is rejected',
  evaluateRuntimeEvidence({ ...completeDoc(), status: 'diagnostic' }, EXPECT).state,
  'malformed',
);
check(
  'status fail contradicting passing checks is rejected',
  evaluateRuntimeEvidence({ ...completeDoc(), status: 'fail' }, EXPECT).state,
  'malformed',
);
check(
  'status pass contradicting a failed check is rejected',
  evaluateRuntimeEvidence(
    { ...completeDoc(), checks: completeDoc().checks.map((c) => (c.id === 'RES-1' ? { ...c, ok: false } : c)) },
    EXPECT,
  ).state,
  'malformed',
);

// --- I9: runtime evidence bound to current source/tree identity -------------

const SOURCE_EXPECT = { source_sha: 'deadbeef', tree_digest: 'c'.repeat(64), tree_dirty: false };
check(
  'matching source identity passes',
  evaluateRuntimeEvidence(completeDoc(), { ...EXPECT, ...SOURCE_EXPECT }).state,
  'valid-pass',
);
const staleSource = { ...completeDoc(), provenance: { ...PROVENANCE, source_sha: 'othersha' } };
check('stale source_sha is rejected', evaluateRuntimeEvidence(staleSource, { ...EXPECT, ...SOURCE_EXPECT }).state, 'stale');
const staleTree = { ...completeDoc(), provenance: { ...PROVENANCE, tree_digest: 'f'.repeat(64) } };
check('stale tree_digest is rejected', evaluateRuntimeEvidence(staleTree, { ...EXPECT, ...SOURCE_EXPECT }).state, 'stale');
const dirtyNow = { ...completeDoc(), provenance: { ...PROVENANCE, tree_dirty: true } };
check(
  'dirty-tree mismatch is rejected',
  evaluateRuntimeEvidence(dirtyNow, { ...EXPECT, ...SOURCE_EXPECT }).state,
  'stale',
);

// --- C3: per-criterion runtime verdicts -------------------------------------

const failingDoc = {
  ...completeDoc(),
  status: 'fail',
  checks: completeDoc().checks.map((c) => (c.id === 'RES-1' ? { ...c, ok: false } : c)),
  native_utility_load: { ok: false },
};
check('valid-fail state for a failed criterion', evaluateRuntimeEvidence(failingDoc, EXPECT).state, 'valid-fail');
check('failed criterion maps to FAIL', runtimeCriterionVerdict('valid-fail', failingDoc, 'RES-1'), 'FAIL');
check('passing criterion maps to PASS', runtimeCriterionVerdict('valid-fail', failingDoc, 'START-1'), 'PASS');
check('unknown state maps to NOT OBSERVED', runtimeCriterionVerdict('confounded', failingDoc, 'RES-1'), 'NOT OBSERVED');
check('valid-pass maps to PASS', runtimeCriterionVerdict('valid-pass', completeDoc(), 'START-1'), 'PASS');
check('a non-canonical document yields no criterion verdict', runtimeCriterionVerdict('stale', completeDoc(), 'START-1'), 'NOT OBSERVED');

// --- synthetic matrix builder ----------------------------------------------

const CHECKS = [
  'package_receipt_pass',
  'empty_project_install',
  'installed_payload_paths',
  'installed_artifact_matches_packed_payload',
  'installed_platform_manifest_is_frozen_metadata',
  'installed_manifest_fences_against_real_artifact',
  'graph_read_through_installed_payload',
  'create_through_installed_payload',
  'update_through_installed_payload',
  'provider_probe_available',
  'provider_launch_session',
  'provider_operation_started',
  'provider_stream_delta_and_terminal',
  'provider_shutdown_ok',
  'happy_close_confirmed',
  'provider_cancel_accepted',
  'cancel_close_confirmed',
  'compilers_shadowed_in_child',
  'no_compiler_invocation_attempted',
  'fixture_children_reaped',
];
const ARCHES = [
  { key: 'arm64', target: 'aarch64-apple-darwin', suffix: 'darwin-arm64', install: { '22.22.0': 'install-macarm22', '24.20.0': 'install-macarm24' } },
  { key: 'x64', target: 'x86_64-apple-darwin', suffix: 'darwin-x64', install: { '22.22.0': 'install-macx6422', '24.20.0': 'install-macx6424' } },
  { key: 'win', target: 'x86_64-pc-windows-msvc', suffix: 'win32-x64-msvc', install: { '22.22.0': 'install-win22', '24.20.0': 'install-win24' } },
  { key: 'linux', target: 'x86_64-unknown-linux-gnu', suffix: 'linux-x64-gnu', install: { '22.22.0': 'install-linux22', '24.20.0': 'install-linux24' } },
];
const GUI = ARCHES.filter((a) => a.key === 'arm64' || a.key === 'x64');
const NODE_COHORTS = [
  { version: '22.22.0', id: 'node22' },
  { version: '24.20.0', id: 'node24' },
];

/** The identity the generator will compute for the current tree. */
function currentIdentity() {
  const sha = spawnSync('git', ['rev-parse', 'HEAD'], { cwd: ROOT, encoding: 'utf8' }).stdout.trim();
  const porcelain = spawnSync('git', ['status', '--porcelain'], { cwd: ROOT, encoding: 'utf8' }).stdout;
  const diff = spawnSync('git', ['diff', 'HEAD'], { cwd: ROOT, encoding: 'utf8' }).stdout;
  return {
    source_sha: sha,
    tree_digest: createHash('sha256').update(`${sha}\0${porcelain}\0${diff}`).digest('hex'),
    tree_dirty: porcelain.trim().length > 0,
  };
}

const IDENTITY = currentIdentity();

function installProof(target, nodeVersion) {
  return {
    schema: 'rft-p3-t1-native-install-proof/v1',
    status: 'pass',
    target,
    source_sha: IDENTITY.source_sha,
    node_version_requested: nodeVersion,
    checks: [
      ...CHECKS.map((name) => ({ name, ok: true })),
      ...Array.from({ length: 11 }, (_, i) => ({ name: `negative:case-${i}`, ok: true })),
    ],
  };
}

function packageReceipt(target, suffix) {
  return {
    schema: 'rft-p3-t1-package-receipt/v1',
    status: 'pass',
    target,
    source_sha: IDENTITY.source_sha,
    artifact: { sha256: (suffix.length % 10).toString().repeat(64).slice(0, 64), bytes: 1024 },
    compatibility: {
      source: 'executed artifact compatibility()',
      manifest: {
        target_triple: target,
        native_api_version: 1,
        writer_protocol: 1,
        contract_tree_sha256: 'a'.repeat(64),
      },
    },
    packages: [
      { name: '@42ch/nexus-native', tarball_bytes: 67948 },
      {
        name: `@42ch/nexus-native-${suffix}`,
        tarball_bytes: 6_000_000,
        entries: [
          'package/package.json',
          'package/AGENTS.md',
          'package/LICENSE',
          'package/native/nexus_core_node.node',
          'package/native/compatibility.json',
        ],
      },
    ],
  };
}

function binaryInspection(target, artifactSha) {
  return {
    schema: 'rft-p3-t1-binary-inspection/v1',
    status: 'pass',
    target,
    artifact: { sha256: artifactSha },
    checks: [{ name: 'artifact_container_matches_target', ok: true }],
    findings: { container: { container: 'mach-o', machine: 'arm64' }, minimum_os: '11.0' },
  };
}

const GATE_BUNDLE_ID = 'com.nexus42.rft-electron-proof';
const NATIVE_NODE_REL =
  '/Contents/Resources/app.asar.unpacked/node_modules/@42ch/nexus-native-darwin-arm64/native/nexus_core_node.node';

/** Create a real (tiny) app bundle and return its recomputable identity. */
function makeAppBundle(evidenceDir, archKey) {
  const bundleDir = join(
    evidenceDir,
    `electron-packages/${archKey}`,
    `Nexus RFT Feasibility-darwin-${archKey}`,
    'Nexus RFT Feasibility.app',
  );
  mkdirSync(join(bundleDir, 'Contents', 'MacOS'), { recursive: true });
  writeFileSync(join(bundleDir, 'Contents', 'MacOS', 'Nexus RFT Feasibility'), `fixture ${archKey}\n`);
  const nativePath = join(bundleDir, NATIVE_NODE_REL.replace(/^\//, ''));
  mkdirSync(dirname(nativePath), { recursive: true });
  writeFileSync(nativePath, `native fixture ${archKey}\n`);
  writeFileSync(join(bundleDir, 'Contents', 'Info.plist'), `<plist><dict><key>CFBundleIdentifier</key><string>${GATE_BUNDLE_ID}</string></dict></plist>\n`);
  const real = realpathSync(bundleDir);
  const digest = digestAppBundle(real);
  const nativeNode = walkFiles(real).files.find((p) => p.endsWith('.node')) ?? null;
  return {
    dir: real,
    sha256: digest.sha256,
    file_count: digest.file_count,
    native_node_path_relative: nativeNode ? nativeNode.replace(real, '') : null,
    native_node_sha256: nativeNode ? sha256File(nativeNode) : null,
  };
}

function runtimeProof(archKey, identity, appPath, appIdentity) {
  const cold = ms(10);
  const warm = ms(30, 5);
  const cycleMs = cycles(100);
  const soakTrace = trace(300);
  return {
    schema: RUNTIME_SCHEMA,
    mode: 'canonical',
    gating: true,
    sample_plan: { cold: 10, warm: 30, cycles: 100, soak_seconds: 600 },
    phases_requested: ['launch', 'resources', 'security', 'lifecycle'],
    phases_executed: ['launch', 'resources', 'security', 'lifecycle'],
    launch: {
      samples: { cold, warm },
      summary: { cold: summarise(cold.map((e) => e.ms)), warm: summarise(warm.map((e) => e.ms)) },
      failures: 0,
    },
    resources: {
      soak: {
        soak_seconds: 600,
        trace: soakTrace,
        summary: summarise(soakTrace.map((e) => e.rss_bytes)),
        workload: { reads: 6000, writes: 1200, read_errors: 0, write_errors: [] },
      },
      cycles: {
        cycle_count: 100,
        durations_ms: cycleMs,
        summary: summarise(cycleMs),
        samples: trace(11),
        plateau_after_cooldown: { rss_bytes: 300 * 1024 * 1024 },
        retained_growth_bytes: 1024,
      },
    },
    validity: { valid: true, launch_method: 'launchservices', confounders: [] },
    provenance: {
      app_path: appPath,
      app_bundle_id: GATE_BUNDLE_ID,
      arch: archKey,
      app_bundle_sha256: appIdentity.sha256,
      native_node_path_relative: appIdentity.native_node_path_relative,
      native_node_sha256: appIdentity.native_node_sha256,
      electron_version: '44.3.0',
      packager_version: '20.3.0',
      source_sha: identity.source_sha,
      tree_digest: identity.tree_digest,
      tree_dirty: identity.tree_dirty,
      command: 'node scripts/proof-runtime.mjs',
      utc_start: '2026-09-13T00:00:00.000Z',
    },
    native_utility_load: {
      ok: true,
      target_triple: `arch:${archKey}`,
      provider_lifecycle: { complete: true },
      native_payload_sha256: appIdentity.native_node_sha256,
    },
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
}

function electronSize(archKey) {
  return {
    schema: 'rft-p3-t3-electron-size/v1',
    status: 'pass',
    arch: archKey,
    app_bundle_id: 'com.nexus42.rft-electron-proof',
    limits: { zip_mib: 250, installed_mib: 600 },
    sizes: { zip_mib: 140, app_bundle_mib: 330 },
    checks: [
      { id: 'PKG-2-electron-zip', ok: true, measured_mib: 140, limit_mib: 250 },
      { id: 'PKG-2-electron-installed', ok: true, measured_mib: 330, limit_mib: 600 },
    ],
    ...IDENTITY,
    command: 'node scripts/proof-runtime.mjs',
    utc_start: '2026-09-13T00:00:00.000Z',
    utc_end: '2026-09-13T00:10:00.000Z',
  };
}

/**
 * A gate document with every raw predicate the SEC-1 row independently verifies
 * (C7), using the real field names the package gate emits.
 */
function packageGate(archKey, identity, appIdentity) {
  const appRealpath = appIdentity.dir;
  return {
    schema: 'rft-p3-t3-package-gate/v2',
    status: 'go',
    bundle_id: GATE_BUNDLE_ID,
    arch: archKey,
    signed_required: true,
    app_path: appRealpath,
    app_realpath: appRealpath,
    missing_inputs: [],
    reasons: [],
    checks: {
      codesign: {
        deep_status: 0,
        display_status: 0,
        identifier: GATE_BUNDLE_ID,
        team_identifier: 'ABCDE12345',
        signature: 'Developer ID Application: Nexus (ABCDE12345)',
        flags: 'runtime',
        hardened_runtime: true,
        authority: ['Developer ID Application: Nexus (ABCDE12345)', 'Developer ID Certification Authority'],
      },
      signature_predicates: {
        pass: true,
        failed: [],
        hardened_runtime_flags: 'runtime',
        signature: 'Developer ID Application: Nexus (ABCDE12345)',
        team_identifier: 'ABCDE12345',
        authority: ['Developer ID Application: Nexus (ABCDE12345)'],
      },
      spctl_execute: { status: 0, stdout: 'accepted', stderr: '' },
      notary: { status: 0, stdout: 'accepted', stderr: '' },
      stapler_validate: { status: 0, stdout: 'The validate action worked!', stderr: '' },
      bundle_identifier: { expected: GATE_BUNDLE_ID, actual: GATE_BUNDLE_ID },
      entitlements: {
        pass: true,
        keys_match: true,
        values_match: true,
        problems: [],
        signed: {
          'com.apple.security.cs.allow-jit': true,
          'com.apple.security.cs.allow-unsigned-executable-memory': true,
          'com.apple.security.files.user-selected.read-only': true,
          'com.apple.security.network.client': true,
        },
        expected: {
          'com.apple.security.cs.allow-jit': true,
          'com.apple.security.cs.allow-unsigned-executable-memory': true,
          'com.apple.security.files.user-selected.read-only': true,
          'com.apple.security.network.client': true,
        },
        expected_source: { path: '/expected.plist', sha256: 'd'.repeat(64) },
        signed_entitlements_plist_sha256: 'e'.repeat(64),
        signed_raw_tail: 'Identifier=com.nexus42.rft-electron-proof',
      },
      runtime_lifecycle: {
        path: '/runtime-lifecycle.json',
        present: true,
        contract_state: 'valid-pass',
        contract_reasons: [],
        provenance: {
          app_path: appRealpath,
          app_bundle_id: GATE_BUNDLE_ID,
          arch: archKey,
          app_bundle_sha256: appIdentity.sha256,
          app_bundle_file_count: appIdentity.file_count,
          native_node_path_relative: appIdentity.native_node_path_relative,
          native_node_sha256: appIdentity.native_node_sha256,
          electron_version: '44.3.0',
          packager_version: '20.3.0',
          source_sha: identity.source_sha,
          tree_digest: identity.tree_digest,
          tree_dirty: identity.tree_dirty,
          command: 'node scripts/proof-runtime.mjs',
          utc_start: '2026-09-13T00:00:00.000Z',
        },
        phases_executed: ['launch', 'resources', 'security', 'lifecycle'],
        checks_summary: [
          { id: 'START-1', ok: true },
          { id: 'RES-1', ok: true },
          { id: 'RES-2', ok: true },
          { id: 'SEC-renderer', ok: true },
          { id: 'LIFECYCLE-native', ok: true },
          { id: 'LIFECYCLE-fault', ok: true },
          { id: 'PKG-2-electron', ok: true },
        ],
        native_utility_load: { ok: true, native_payload_sha256: appIdentity.native_node_sha256 },
        recorded_confounders: [],
        bound_to: {
          app_path: appRealpath,
          app_bundle_id: GATE_BUNDLE_ID,
          arch: archKey,
          electron_version: '44.3.0',
          packager_version: '20.3.0',
          source_sha: identity.source_sha,
          tree_digest: identity.tree_digest,
          tree_dirty: identity.tree_dirty,
        },
      },
    },
    decision_inputs: {
      runtime_contract_state: 'valid-pass',
      signature_predicates_pass: true,
      stapler_ok: true,
      hardened_runtime: true,
      entitlements_match: true,
      native_utility_load_proven: true,
      native_load_check_id: 'LIFECYCLE-native',
      rules: 'go = valid-pass runtime + signature predicates + staple + hardened + provenance-bound native load',
    },
  };
}

function maintenanceTrace() {
  return {
    schema: 'rft-p3-t3-maintenance-rebuild/v1',
    status: 'pass',
    arch: 'arm64',
    target_electron_version: '44.3.0',
    packaged_electron_version: '44.3.0',
    rebuild_count: 1,
    manual_binary_patch: false,
    elapsed_seconds: 420,
    max_seconds: 1800,
    checks: [
      { name: 'pin_is_target', ok: true },
      { name: 'frozen_lockfile_install', ok: true },
      { name: 'package_rebuild', ok: true },
      { name: 'packaged_version_matches_target', ok: true },
      { name: 'no_manual_binary_patch', ok: true },
      { name: 'within_thirty_minutes', ok: true },
    ],
    ...IDENTITY,
  };
}

/**
 * Materialise a complete synthetic matrix. `mutate` receives the document map
 * (keyed by path below the evidence root) so a case can break exactly one thing.
 */
function buildMatrix(name, mutate = () => {}) {
  const root = join(DECISION_FIXTURES, name, 'iterations');
  const evidence = join(root, 'v1.189', 'guides', 'evidence');
  rmSync(join(DECISION_FIXTURES, name), { recursive: true, force: true });
  const docs = {};
  const context = { apps: {}, evidence };
  const put = (relative, doc) => {
    docs[relative] = doc;
  };

  for (const arch of ARCHES) {
    for (const cohort of NODE_COHORTS) {
      put(`${arch.install[cohort.version]}/install-proof.json`, installProof(arch.target, cohort.version));
    }
    put(`native-packages/${arch.suffix}/package-receipt.json`, packageReceipt(arch.target, arch.suffix));
  }
  for (const arch of ARCHES) {
    const receipt = docs[`native-packages/${arch.suffix}/package-receipt.json`];
    put(`native-binary-${arch.suffix}/binary-inspection.json`, binaryInspection(arch.target, receipt.artifact.sha256));
  }
  for (const arch of GUI) {
    // A real (tiny) bundle on disk, so the SEC row's bundle/native hash checks
    // are exercised against actual bytes rather than a fabricated digest.
    const appIdentity = makeAppBundle(evidence, arch.key);
    context.apps[arch.key] = appIdentity;
    put(`electron-${arch.key}/runtime-lifecycle.json`, runtimeProof(arch.key, IDENTITY, appIdentity.dir, appIdentity));
    put(`electron-${arch.key}/electron-size.json`, electronSize(arch.key));
    put(`electron-${arch.key}/proof-package.json`, packageGate(arch.key, IDENTITY, appIdentity));
  }
  put('maintenance-rebuild.json', maintenanceTrace());

  mutate(docs, context);

  for (const [relative, doc] of Object.entries(docs)) {
    const target = join(evidence, relative);
    mkdirSync(dirname(target), { recursive: true });
    writeFileSync(target, `${JSON.stringify(doc, null, 2)}\n`);
  }
  return root;
}

// --- synthetic complete matrix: GO is reachable without editing the reducer --

const greenRoot = buildMatrix('green');
const greenDecision = runDecisionWithRoot(greenRoot);
check('a complete development matrix yields GO', greenDecision.status, 'go');
check('development GO matrix has no non-pass gating rows', greenDecision.doc?.row_counts?.missing_or_unobserved, 0);
check('development GO matrix row count excludes two release observations', greenDecision.doc?.row_counts?.total, 35);
check(
  'GO matrix retains both GUI architectures as release observations',
  (greenDecision.doc?.release_observations ?? []).filter((r) => r.id.startsWith('SEC1-signed-')).every((r) => r.verdict === 'PASS'),
  true,
);
check(
  'GO matrix derives per-arch Electron size as PASS',
  ['PKG2-electron-arm64', 'PKG2-electron-x64'].every(
    (id) => (greenDecision.doc?.observed_rows ?? []).find((r) => r.id === id)?.verdict === 'PASS',
  ),
  true,
);
check(
  'GO matrix derives MAINT-1 as PASS',
  (greenDecision.doc?.observed_rows ?? []).find((r) => r.id === 'MAINT-1')?.verdict,
  'PASS',
);

// --- valid measured failures yield no-go ------------------------------------

const failCases = [
  ['start', (docs) => { docs['electron-arm64/runtime-lifecycle.json'].checks = docs['electron-arm64/runtime-lifecycle.json'].checks.map((c) => (c.id === 'START-1' ? { ...c, ok: false } : c)); docs['electron-arm64/runtime-lifecycle.json'].status = 'fail'; }],
  ['res', (docs) => { docs['electron-arm64/runtime-lifecycle.json'].checks = docs['electron-arm64/runtime-lifecycle.json'].checks.map((c) => (c.id === 'RES-2' ? { ...c, ok: false } : c)); docs['electron-arm64/runtime-lifecycle.json'].status = 'fail'; }],
  ['size', (docs) => { docs['electron-x64/electron-size.json'].sizes.zip_mib = 300; docs['electron-x64/electron-size.json'].status = 'fail'; }],
  ['payload', (docs) => { docs['native-packages/darwin-x64/package-receipt.json'].packages[1].tarball_bytes = 60 * 1048576; docs['native-packages/darwin-x64/package-receipt.json'].status = 'fail'; }],
  ['install', (docs) => {
    // A genuine measured install failure names the check that failed.
    const proof = docs['install-win24/install-proof.json'];
    proof.status = 'fail';
    proof.checks.find((c) => c.name === 'empty_project_install').ok = false;
  }],
  ['maint', (docs) => {
    const trace = docs['maintenance-rebuild.json'];
    trace.status = 'fail';
    trace.elapsed_seconds = 3600;
    trace.checks.find((c) => c.name === 'within_thirty_minutes').ok = false;
  }],
];
for (const [label, mutate] of failCases) {
  const root = buildMatrix(`fail-${label}`, mutate);
  const decision = runDecisionWithRoot(root);
  check(`measured failure (${label}) yields no-go`, decision.status, 'no-go');
  check(`measured failure (${label}) records a FAIL row`, (decision.doc?.row_counts?.fail ?? 0) > 0, true);
}

// F-001: a status field alone is not a measurement.
for (const label of ['status-only-flip-install', 'status-only-flip-package', 'status-only-flip-size', 'status-only-flip-maintenance']) {
  const root = buildMatrix(`unsupported-${label}`, (docs) => {
    const map = {
      'status-only-flip-install': 'install-macarm22/install-proof.json',
      'status-only-flip-package': 'native-packages/darwin-arm64/package-receipt.json',
      'status-only-flip-size': 'electron-x64/electron-size.json',
      'status-only-flip-maintenance': 'maintenance-rebuild.json',
    };
    docs[map[label]].status = 'fail';
  });
  const decision = runDecisionWithRoot(root);
  check(`F-001 unsupported failure (${label}) blocks instead of no-go`, decision.status, 'blocked');
  const failedRows = (decision.doc?.observed_rows ?? []).filter((r) => r.verdict === 'FAIL');
  check(`F-001 unsupported failure (${label}) records no FAIL row`, failedRows.length, 0);
}

// F-001 (missing-evidence variant): a failed document whose required checks or
// structural fields are absent is an incomplete producer, not a measurement —
// it must block, and it must never create a FAIL row or force a no-go.
{
  const root = buildMatrix('unsupported-fail-missing-checks', (docs) => {
    const proof = docs['install-macarm22/install-proof.json'];
    proof.status = 'fail';
    proof.checks = proof.checks.filter((c) => c.name !== 'empty_project_install');
    const unrelated = proof.checks.find((c) => c.name === 'graph_read_through_installed_payload');
    unrelated.ok = false;
  });
  const decision = runDecisionWithRoot(root);
  check('F-001 failed install with an absent required check plus unrelated measured failure blocks', decision.status, 'blocked');
  check(
    'F-001 failed install with an absent required check plus unrelated measured failure records no FAIL row',
    (decision.doc?.observed_rows ?? []).filter((r) => r.verdict === 'FAIL').length,
    0,
  );
}
{
  const root = buildMatrix('unsupported-fail-unknown-check', (docs) => {
    const proof = docs['install-macarm22/install-proof.json'];
    proof.status = 'fail';
    proof.checks.push({ name: 'unrecognized_future_check', ok: false });
  });
  const decision = runDecisionWithRoot(root);
  check('F-001 failed install with an unknown false check blocks', decision.status, 'blocked');
  check(
    'F-001 failed install with an unknown false check records no FAIL row',
    (decision.doc?.observed_rows ?? []).filter((r) => r.verdict === 'FAIL').length,
    0,
  );
}
{
  const root = buildMatrix('unsupported-fail-absent-fields', (docs) => {
    const receipt = docs['native-packages/darwin-arm64/package-receipt.json'];
    receipt.status = 'fail';
    delete receipt.artifact;
  });
  const decision = runDecisionWithRoot(root);
  check('F-001 failed package receipt with absent fields blocks', decision.status, 'blocked');
  check(
    'F-001 failed package receipt with absent fields records no FAIL row',
    (decision.doc?.observed_rows ?? []).filter((r) => r.verdict === 'FAIL').length,
    0,
  );
}

// --- F-002: missing inputs are derived from the rows -------------------------
const realMissing = realDecision.doc?.missing_inputs ?? [];
for (const id of ['PKG2-electron-arm64', 'PKG2-electron-x64']) {
  const row = (realDecision.doc?.observed_rows ?? []).find((entry) => entry.id === id);
  check(
    `F-002 ${id} missing-input membership matches its current verdict`,
    realMissing.some((entry) => String(entry.input).startsWith(id)),
    row?.verdict !== 'PASS',
  );
}
check(
  'F-002 every non-pass development row appears in missing_inputs',
  (realDecision.doc?.observed_rows ?? [])
    .filter((r) => r.gating !== false && r.verdict !== 'PASS')
    .every((r) => realMissing.some((entry) => String(entry.input).startsWith(r.id))),
  true,
);
check(
  'F-002 no PASS row appears in missing_inputs',
  (realDecision.doc?.observed_rows ?? [])
    .filter((r) => r.verdict === 'PASS')
    .every((r) => !realMissing.some((entry) => String(entry.input).startsWith(r.id))),
  true,
);

// --- malformed / inconsistent evidence blocks -------------------------------

const inconsistentCases = [
  ['status-pass-with-false-check', (docs) => { docs['install-macarm22/install-proof.json'].checks[0].ok = false; }],
  ['wrong-target', (docs) => { docs['install-macarm22/install-proof.json'].target = 'x86_64-unknown-linux-gnu'; }],
  ['wrong-source', (docs) => { docs['install-win22/install-proof.json'].source_sha = 'othersha'; }],
  ['missing-check-name', (docs) => { docs['install-linux22/install-proof.json'].checks = docs['install-linux22/install-proof.json'].checks.filter((c) => c.name !== 'provider_cancel_accepted'); }],
  ['size-over-limit-with-pass-status', (docs) => { docs['electron-x64/electron-size.json'].sizes.zip_mib = 300; }],
  ['runtime-status-inconsistent', (docs) => { const doc = docs['electron-arm64/runtime-lifecycle.json']; doc.checks = doc.checks.map((c) => (c.id === 'RES-1' ? { ...c, ok: false } : c)); }],
];
for (const [label, mutate] of inconsistentCases) {
  const root = buildMatrix(`bad-${label}`, mutate);
  const decision = runDecisionWithRoot(root);
  check(`inconsistent evidence (${label}) blocks`, decision.status, 'blocked');
  check(
    `inconsistent evidence (${label}) yields no PASS row for the broken document`,
    decision.status !== 'go',
    true,
  );
}

// --- C7: one adversarial mutation per raw gate field ------------------------
// Each case starts from the fully-satisfied gate and breaks exactly one raw
// field, so the SEC row must not be satisfiable by the `go` status alone.

const RAW_FIELD_MUTATIONS = [
  ['signed_required', (g) => { g.signed_required = false; }],
  ['decision_inputs.signature_predicates_pass', (g) => { g.decision_inputs.signature_predicates_pass = false; }],
  ['signature_predicates.pass', (g) => { g.checks.signature_predicates.pass = false; }],
  ['signature_predicates.failed', (g) => { g.checks.signature_predicates.failed = ['codesign --verify --deep --strict']; }],
  ['codesign.deep_status', (g) => { g.checks.codesign.deep_status = 1; }],
  ['codesign.identifier', (g) => { g.checks.codesign.identifier = 'Electron'; }],
  ['codesign.signature', (g) => { g.checks.codesign.signature = 'adhoc'; }],
  ['codesign.team_identifier', (g) => { g.checks.codesign.team_identifier = 'not set'; }],
  ['codesign.authority', (g) => { g.checks.codesign.authority = []; }],
  ['codesign.hardened_runtime', (g) => { g.checks.codesign.hardened_runtime = false; }],
  ['codesign.flags', (g) => { g.checks.codesign.flags = 'adhoc,linker-signed'; }],
  ['signature_predicates.hardened_runtime_flags', (g) => { g.checks.signature_predicates.hardened_runtime_flags = 'adhoc'; }],
  ['spctl_execute.status', (g) => { g.checks.spctl_execute.status = 1; }],
  ['notary.status', (g) => { g.checks.notary.status = 3; }],
  ['stapler_validate.status', (g) => { g.checks.stapler_validate.status = 65; }],
  ['bundle_identifier.actual', (g) => { g.checks.bundle_identifier.actual = 'Electron'; }],
  ['bundle_identifier.expected', (g) => { g.checks.bundle_identifier.expected = 'com.example.other'; }],
  ['entitlements.pass', (g) => { g.checks.entitlements.pass = false; }],
  ['entitlements.values_match', (g) => { g.checks.entitlements.values_match = false; }],
  ['entitlements.keys_match', (g) => { g.checks.entitlements.keys_match = false; }],
  ['entitlements.signed value', (g) => { g.checks.entitlements.signed['com.apple.security.cs.allow-jit'] = false; }],
  ['entitlements.signed_entitlements_plist_sha256', (g) => { g.checks.entitlements.signed_entitlements_plist_sha256 = null; }],
  ['entitlements.expected_source.sha256', (g) => { g.checks.entitlements.expected_source.sha256 = null; }],
  ['entitlements.problems', (g) => { g.checks.entitlements.problems = ['signed entitlements unparseable']; }],
  ['decision_inputs.hardened_runtime', (g) => { g.decision_inputs.hardened_runtime = false; }],
  ['decision_inputs.stapler_ok', (g) => { g.decision_inputs.stapler_ok = false; }],
  ['decision_inputs.entitlements_match', (g) => { g.decision_inputs.entitlements_match = false; }],
  ['decision_inputs.native_utility_load_proven', (g) => { g.decision_inputs.native_utility_load_proven = false; }],
  ['decision_inputs.native_load_check_id', (g) => { g.decision_inputs.native_load_check_id = 'SOMETHING-ELSE'; }],
  ['nested contract_state vs decision input', (g) => { g.checks.runtime_lifecycle.contract_state = 'confounded'; }],
  ['nested present', (g) => { g.checks.runtime_lifecycle.present = false; }],
  ['nested native_utility_load.ok', (g) => { g.checks.runtime_lifecycle.native_utility_load = { ok: false }; }],
  ['nested checks_summary falsified', (g) => {
    g.checks.runtime_lifecycle.checks_summary = g.checks.runtime_lifecycle.checks_summary.map((c) =>
      c.id === 'LIFECYCLE-native' ? { ...c, ok: false } : c,
    );
  }],
  ['nested checks_summary load check removed', (g) => {
    g.checks.runtime_lifecycle.checks_summary = g.checks.runtime_lifecycle.checks_summary.filter(
      (c) => c.id !== 'LIFECYCLE-native',
    );
  }],
  ['nested provenance.app_path', (g) => { g.checks.runtime_lifecycle.provenance.app_path = '/elsewhere/Other.app'; }],
  ['nested provenance.app_bundle_id', (g) => { g.checks.runtime_lifecycle.provenance.app_bundle_id = 'com.example.other'; }],
  ['nested provenance.arch', (g) => { g.checks.runtime_lifecycle.provenance.arch = g.arch === 'arm64' ? 'x64' : 'arm64'; }],
  ['nested provenance.source_sha', (g) => { g.checks.runtime_lifecycle.provenance.source_sha = 'othersha'; }],
  ['nested provenance.tree_digest', (g) => { g.checks.runtime_lifecycle.provenance.tree_digest = 'f'.repeat(64); }],
  ['nested provenance.tree_dirty', (g) => { g.checks.runtime_lifecycle.provenance.tree_dirty = !g.checks.runtime_lifecycle.provenance.tree_dirty; }],
  ['nested provenance.app_bundle_sha256', (g) => { g.checks.runtime_lifecycle.provenance.app_bundle_sha256 = 'b'.repeat(64); }],
  ['nested provenance.native_node_sha256', (g) => { g.checks.runtime_lifecycle.provenance.native_node_sha256 = 'c'.repeat(64); }],
  ['nested provenance.native_node_path_relative', (g) => { delete g.checks.runtime_lifecycle.provenance.native_node_path_relative; }],
  ['bound_to.app_path', (g) => { g.checks.runtime_lifecycle.bound_to.app_path = '/elsewhere/Other.app'; }],
  ['bound_to.app_bundle_id', (g) => { g.checks.runtime_lifecycle.bound_to.app_bundle_id = 'com.example.other'; }],
  ['bound_to.arch', (g) => { g.checks.runtime_lifecycle.bound_to.arch = g.arch === 'arm64' ? 'x64' : 'arm64'; }],
  ['bound_to.source_sha', (g) => { g.checks.runtime_lifecycle.bound_to.source_sha = 'othersha'; }],
  ['bound_to.tree_digest', (g) => { g.checks.runtime_lifecycle.bound_to.tree_digest = 'f'.repeat(64); }],
  ['bound_to.tree_dirty', (g) => { g.checks.runtime_lifecycle.bound_to.tree_dirty = !g.checks.runtime_lifecycle.bound_to.tree_dirty; }],
];

for (const [label, mutate] of RAW_FIELD_MUTATIONS) {
  const root = buildMatrix(`raw-${label.replace(/[^a-z0-9]+/gi, '-')}`, (docs) => {
    mutate(docs['electron-arm64/proof-package.json']);
  });
  const decision = runDecisionWithRoot(root);
  const row = (decision.doc?.observed_rows ?? []).find((r) => r.id === 'SEC1-signed-arm64');
  check(`C7 raw field (${label}) does not pass the SEC row`, row?.verdict !== 'PASS', true);
  check(`C7 raw field (${label}) does not block development`, decision.status, 'go');
  check(`C7 raw field (${label}) names the broken predicate`, (row?.verification?.problems ?? []).length > 0, true);
}

// The on-disk bundle must actually match what the gate claims about it.
const BUNDLE_MUTATIONS = [
  ['bundle-file-added', (ctx) => {
    writeFileSync(join(ctx.apps.arm64.dir, 'Contents', 'injected.txt'), 'tampered\n');
  }],
  ['native-node-modified', (ctx) => {
    writeFileSync(join(ctx.apps.arm64.dir, 'Contents', 'Resources', 'app.asar.unpacked', 'node_modules', '@42ch', 'nexus-native-darwin-arm64', 'native', 'nexus_core_node.node'), 'tampered native\n');
  }],
];
for (const [label, mutate] of BUNDLE_MUTATIONS) {
  const root = buildMatrix(`bundle-${label}`, (docs, ctx) => mutate(ctx));
  const decision = runDecisionWithRoot(root);
  const row = (decision.doc?.observed_rows ?? []).find((r) => r.id === 'SEC1-signed-arm64');
  check(`C7 on-disk artifact (${label}) does not pass the SEC row`, row?.verdict !== 'PASS', true);
  check(`C7 on-disk release artifact (${label}) does not block development`, decision.status, 'go');
}

// --- QC3 W1/S1/S2/S3: driver and utility-environment invariants --------------
// These are source-level invariants enforced by the driver CLI and the utility
// environment builder; they are checked here so a regression cannot silently
// reintroduce the packaged-owner failure.

const SANITIZE_SRC = readFileSync(join(__dirname, '..', 'src', 'env.ts'), 'utf8');
check(
  'W1: sanitizeInheritedEnv does not set ELECTRON_RUN_AS_NODE for the utility child',
  /next\.ELECTRON_RUN_AS_NODE\s*=/.test(SANITIZE_SRC),
  false,
);
check(
  'W1: the utility environment builder documents why the flag is absent',
  /ELECTRON_RUN_AS_NODE/.test(SANITIZE_SRC) && /bad option/.test(SANITIZE_SRC),
  true,
);

const MAIN_SRC = readFileSync(join(__dirname, '..', 'src', 'main.ts'), 'utf8');
const SERVICE_SRC = readFileSync(join(__dirname, '..', 'src', 'service-controller.ts'), 'utf8');
check(
  'F-003: service close retains ownership while close is in flight',
  /if \(this\.closeInFlight\)/.test(SERVICE_SRC) && /this\.closeInFlight = true/.test(SERVICE_SRC),
  true,
);
check(
  'F-003: the old close-in-flight abandonment predicate is absent',
  /lifecycle\.phase !== 'closed' && !closeInFlight/.test(MAIN_SRC),
  false,
);
check(
  'F-003: the quit handshake is guarded against re-entry',
  /if \(quitAllowed\) return/.test(MAIN_SRC) && /requestQuit\(\)/.test(MAIN_SRC),
  true,
);
check(
  'F-003: product quit is requested through the desktop quit controller',
  /requestQuit\(\)/.test(MAIN_SRC),
  true,
);

check(
  'S2: packaged builds refuse a missing bundled web dist instead of falling back',
  /app\.isPackaged/.test(MAIN_SRC) && /packaged build is missing its bundled web artifact/.test(MAIN_SRC),
  true,
);

const RUNTIME_SRC = readFileSync(join(__dirname, 'proof-runtime.mjs'), 'utf8');
check(
  'SEC-renderer derives web_bundle from the asar web-dist entry, not a request count',
  /evidence\.asar\?\.web_dist_index_present === true/.test(RUNTIME_SRC),
  true,
);
check(
  'SEC-renderer derives no_native_in_renderer from the unpacked asar entry',
  /\.every\(\(entry\) => entry\.unpacked === true\)/.test(RUNTIME_SRC),
  true,
);
check(
  'LIFECYCLE-fault refuses to run without a live native owner to kill',
  /no live native owner before the fault/.test(RUNTIME_SRC),
  true,
);
check(
  'LIFECYCLE-fault targets the Node-service utility, not any helper',
  /function isNativeOwnerProcess/.test(RUNTIME_SRC) && /node\.mojom\.NodeService/.test(RUNTIME_SRC),
  true,
);
check(
  'RSS is measured over the bundle processes, not a pid tree',
  /function rssSample\(appPath, rootPid\)/.test(RUNTIME_SRC) && /bundleProcesses\(appPath\)/.test(RUNTIME_SRC),
  true,
);
check(
  'RES-2 samples the plateau before quitting the app',
  /const plateau = rssSample\(cyclesApp\.appPath, cyclesApp\.pid\);\s*\n\s*const survivors = await collectSurvivors\(cyclesApp\);\s*\n\s*const cycleExit = await cyclesApp\.quitAndWait\(\);/.test(RUNTIME_SRC),
  true,
);
check(
  'RES-1 records the per-process RSS composition',
  /idleSample\.processes\.map/.test(RUNTIME_SRC),
  true,
);
check(
  'the soak workload uses entities the world actually reports',
  /soak_seed/.test(RUNTIME_SRC) && /cannot establish a soak workload/.test(RUNTIME_SRC),
  true,
);
check(
  'the provider terminal reads the wire field op_id',
  /event\.OpFinished\?\.op_id \?\? event\.OpFailed\?\.op_id/.test(RUNTIME_SRC),
  true,
);
check(
  'LIFECYCLE-fault waits for recovery instead of a fixed sleep',
  /function waitForUtilityRecovery/.test(RUNTIME_SRC) && /await waitForUtilityRecovery\(app/.test(RUNTIME_SRC),
  true,
);
check(
  'provider probe failures record the exact error and reply shape',
  /error: probe\?\.error\?\.message \?\? null/.test(RUNTIME_SRC),
  true,
);


check(
  'F-003: the driver collects survivors only after the close completes',
  /const close = await app\.closeCleanly\(\);\s*\n\s*const survivalAfterClose = close\.survivors;/.test(RUNTIME_SRC),
  true,
);
check(
  'F-003: the driver asks the app to quit after closing the owner',
  /requestQuit\?\.\(\)/.test(RUNTIME_SRC),
  true,
);
check(
  'close success requires a bundle-wide empty observation, not a quiet pid tree',
  /function waitForBundleEmpty/.test(RUNTIME_SRC) &&
    /await waitForBundleEmpty\(this\.appPath, timeoutMs, this\.trackedDescendants\)/.test(RUNTIME_SRC),
  true,
);
check(
  'LIFECYCLE-fault tracks the provider child from the live owner before killing it',
  /app\.trackedDescendants = trackOwnerDescendants\(victim\.pid\);/.test(RUNTIME_SRC) &&
    /await waitForTrackedExit\(app\.trackedDescendants, 15_000\)/.test(RUNTIME_SRC) &&
    /tracked_descendants_gone: trackedSurvivors\.length === 0/.test(RUNTIME_SRC),
  true,
);
// The unpack pattern is a bare glob and must actually match: @electron/asar
// calls minimatch(filename, unpack, { matchBase: true }), and minimatch treats
// braces as expansion, so a `{**/*.node}` pattern matches nothing and silently
// packs the native payload inside the asar.
const PACKAGE_SRC = readFileSync(join(__dirname, '..', 'scripts', 'package.mjs'), 'utf8');
check(
  'W1: the asar unpack pattern is a bare glob for the native payload',
  /unpack: '\*\*\/\*\.node'/.test(PACKAGE_SRC),
  true,
);
check(
  'W1: the unpack pattern is not a literal-brace string',
  /unpack: '\{[^']*\}'/.test(PACKAGE_SRC),
  false,
);
check(
  'W1: the unpack pattern does not include utility-host.js',
  /unpack:[^\n]*utility-host/.test(PACKAGE_SRC),
  false,
);
check(
  'W1: main does not fork the utility from app.asar.unpacked',
  /app\.asar\.unpacked\/dist\/utility-host\.js/.test(MAIN_SRC) && /existsSync\(unpacked\)/.test(MAIN_SRC),
  false,
);
check(
  'W1: main forks the injected asar-resident utility entry',
  /utilityProcess\.fork\(input\.paths\.utilityEntry/.test(MAIN_SRC),
  true,
);
check(
  'W1: the product host has no proof-only log environment',
  /NEXUS_PROOF_LOG/.test(MAIN_SRC),
  false,
);
check(
  'W1: the driver routes app diagnostics into the evidence on owner-unavailable',
  /diagnostics: app\.diagnostics\(\)/.test(RUNTIME_SRC),
  true,
);
check(
  'W1: the driver redacts absolute home paths out of captured diagnostics',
  /replace\(\/\\\/\(Users\|home\)\\\/\[\^\\s'"\]\+\/g, '<path>'\)/.test(RUNTIME_SRC),
  true,
);

check(
  'S3: each run materialises a private home from the seeded template',
  /function prepareRunHome/.test(RUNTIME_SRC) && /run-home/.test(RUNTIME_SRC),
  true,
);

// S1 is behaviour, not text: a canonical invocation that asks for a direct
// launch must be refused before any work starts, so the gating path can never
// silently produce a confounded document.
const cliHome = join(TMP, 'cli-home');
mkdirSync(join(cliHome, 'config'), { recursive: true });
const cliOut = join(TMP, 'cli-out');
mkdirSync(cliOut, { recursive: true });
const directCanonical = spawnSync(
  'node',
  [
    join(__dirname, 'proof-runtime.mjs'),
    '--app', FIXTURE_APP_REAL,
    '--out', cliOut,
    '--home', cliHome,
    '--launch-method', 'direct',
  ],
  { cwd: ROOT, encoding: 'utf8' },
);
check('S1: canonical + direct launch is refused', directCanonical.status !== 0, true);
check(
  'S1: the refusal explains the launchservices requirement',
  /canonical \(gating\) runs require --launch-method launchservices/.test(directCanonical.stdout + directCanonical.stderr),
  true,
);
check(
  'S1: the refused run writes no canonical document',
  existsSync(join(cliOut, 'runtime-lifecycle.json')),
  false,
);
const helpRes = spawnSync('node', [join(__dirname, 'proof-runtime.mjs'), '--help'], { cwd: ROOT, encoding: 'utf8' });
check('S1: --help documents the launch-method requirement', /--launch-method direct\|launchservices/.test(helpRes.stdout + helpRes.stderr), true);

// --- C8: path identity, not path markers ------------------------------------
// Every case here keeps the recorded hashes green (they are copied from the real
// bundle) and moves the paths, so only real path identity can reject it.

const PATH_SUBSTITUTION_CASES = [
  ['gate-app-path-other-dir-same-arch-marker', (docs, ctx) => {
    // A different bundle whose path still contains `darwin-arm64`.
    const decoy = join(ctx.evidence, 'decoy', 'darwin-arm64', 'Nexus RFT Feasibility.app');
    cpSync(ctx.apps.arm64.dir, decoy, { recursive: true });
    const decoyReal = realpathSync(decoy);
    const gate = docs['electron-arm64/proof-package.json'];
    gate.app_path = decoyReal;
    gate.app_realpath = decoyReal;
  }],
  ['runtime-provenance-app-path-elsewhere', (docs, _ctx) => {
    docs['electron-arm64/proof-package.json'].checks.runtime_lifecycle.provenance.app_path = '/tmp/elsewhere/darwin-arm64/Nexus RFT Feasibility.app';
  }],
  ['bound-to-app-path-elsewhere', (docs, _ctx) => {
    docs['electron-arm64/proof-package.json'].checks.runtime_lifecycle.bound_to.app_path = '/tmp/elsewhere/darwin-arm64/Nexus RFT Feasibility.app';
  }],
  ['gate-app-path-traversal', (docs, ctx) => {
    const gate = docs['electron-arm64/proof-package.json'];
    const traversing = join(ctx.apps.arm64.dir, '..', '..', 'darwin-arm64', 'Nexus RFT Feasibility.app');
    gate.app_path = traversing;
    gate.app_realpath = traversing;
  }],
  ['native-relative-path-altered', (docs) => {
    docs['electron-arm64/proof-package.json'].checks.runtime_lifecycle.provenance.native_node_path_relative =
      '/Contents/Resources/app.asar.unpacked/node_modules/@42ch/nexus-native-darwin-arm64/native/other.node';
  }],
  ['native-relative-path-traversal', (docs) => {
    docs['electron-arm64/proof-package.json'].checks.runtime_lifecycle.provenance.native_node_path_relative =
      '/../../../../etc/hosts';
  }],
  ['native-relative-path-absolute-elsewhere', (docs) => {
    docs['electron-arm64/proof-package.json'].checks.runtime_lifecycle.provenance.native_node_path_relative =
      '/tmp/elsewhere/nexus_core_node.node';
  }],
  ['native-relative-path-empty', (docs) => {
    docs['electron-arm64/proof-package.json'].checks.runtime_lifecycle.provenance.native_node_path_relative = '';
  }],
  ['native-relative-path-absent', (docs) => {
    delete docs['electron-arm64/proof-package.json'].checks.runtime_lifecycle.provenance.native_node_path_relative;
  }],
];

for (const [label, mutate] of PATH_SUBSTITUTION_CASES) {
  const root = buildMatrix(`path-${label}`, mutate);
  const decision = runDecisionWithRoot(root);
  const row = (decision.doc?.observed_rows ?? []).find((r) => r.id === 'SEC1-signed-arm64');
  check(`C8 substitution (${label}) does not pass the SEC row`, row?.verdict !== 'PASS', true);
  check(`C8 release substitution (${label}) does not block development`, decision.status, 'go');
  check(`C8 substitution (${label}) names a path predicate`, (row?.verification?.problems ?? []).some((p) => /app_path|app_realpath|native_node_path_relative/.test(p)), true);
}

// A symlinked app path must canonicalize to the same bundle, not a different one.
const SYMLINK_CASE = buildMatrix('path-symlink-alias', (docs, ctx) => {
  const aliasRoot = join(ctx.evidence, 'alias');
  mkdirSync(aliasRoot, { recursive: true });
  const alias = join(aliasRoot, 'Nexus RFT Feasibility.app');
  rmSync(alias, { recursive: true, force: true });
  symlinkSync(ctx.apps.arm64.dir, alias);
  const gate = docs['electron-arm64/proof-package.json'];
  gate.app_path = alias;
  gate.app_realpath = alias;
  gate.checks.runtime_lifecycle.provenance.app_path = alias;
  gate.checks.runtime_lifecycle.bound_to.app_path = alias;
});
const symlinkDecision = runDecisionWithRoot(SYMLINK_CASE);
const symlinkRow = (symlinkDecision.doc?.observed_rows ?? []).find((r) => r.id === 'SEC1-signed-arm64');
check(
  'C8 symlink alias is accepted by canonicalization and not rejected as a mismatch',
  (symlinkRow?.verification?.problems ?? []).every((p) => !/app_path|app_realpath/.test(p)),
  true,
);
check(
  'C8 symlink alias to a copied sibling is rejected',
  (() => {
    const root = buildMatrix('path-symlink-sibling', (docs, ctx) => {
      const decoy = join(ctx.evidence, 'sibling', 'darwin-arm64', 'Nexus RFT Feasibility.app');
      cpSync(ctx.apps.arm64.dir, decoy, { recursive: true });
      const alias = join(ctx.evidence, 'sibling-alias.app');
      rmSync(alias, { recursive: true, force: true });
      symlinkSync(decoy, alias);
      const gate = docs['electron-arm64/proof-package.json'];
      gate.app_path = alias;
      gate.app_realpath = alias;
      gate.checks.runtime_lifecycle.provenance.app_path = alias;
      gate.checks.runtime_lifecycle.bound_to.app_path = alias;
    });
    const decision = runDecisionWithRoot(root);
    const releaseRow = (decision.doc?.release_observations ?? []).find((r) => r.id === 'SEC1-signed-arm64');
    return decision.status === 'go' && releaseRow?.verdict !== 'PASS';
  })(),
  true,
);

// --- strengthened predicate coverage: a green status cannot carry a bad body --

const BODY_CASES = [
  ['install-extra-false-check', (docs) => {
    docs['install-macarm22/install-proof.json'].checks.push({ name: 'some_extra_check', ok: false });
  }],
  ['package-missing-frozen-entry', (docs) => {
    const pkg = docs['native-packages/darwin-arm64/package-receipt.json'];
    pkg.packages.find((p) => p.name.includes('native-') && p.name !== '@42ch/nexus-native').entries = ['package/package.json'];
  }],
  ['maintenance-false-predicate', (docs) => {
    docs['maintenance-rebuild.json'].checks.find((c) => c.name === 'within_thirty_minutes').ok = false;
  }],
  ['maintenance-no-predicates', (docs) => { docs['maintenance-rebuild.json'].checks = []; }],
  ['binary-inspection-false-check', (docs) => {
    docs['native-binary-darwin-arm64/binary-inspection.json'].checks.push({ name: 'extra', ok: false });
  }],
  ['size-false-check', (docs) => {
    docs['electron-arm64/electron-size.json'].checks[0].ok = false;
  }],
];
for (const [label, mutate] of BODY_CASES) {
  const root = buildMatrix(`body-${label}`, mutate);
  const decision = runDecisionWithRoot(root);
  check(`green status with bad body (${label}) does not yield GO`, decision.status !== 'go', true);
  check(`green status with bad body (${label}) blocks`, decision.status, 'blocked');
}

// --- I12: a forged gate status is never believed ----------------------------
// Each case keeps the gate identity valid and breaks one predicate, so the row
// must go BLOCKED rather than accepting the status field at face value.

const FORGED_GATES = [
  ['go-without-signature-predicate', (g) => { g.decision_inputs.signature_predicates_pass = false; }],
  ['go-with-failed-signature-check', (g) => { g.checks.signature_predicates = { pass: false, failed: ['codesign --verify --deep --strict'] }; }],
  ['go-without-signed-required', (g) => { g.signed_required = false; }],
  ['go-with-failing-stapler', (g) => { g.checks.stapler_validate = { status: 1 }; }],
  ['go-without-hardened-runtime', (g) => { g.decision_inputs.hardened_runtime = false; }],
  ['go-with-entitlement-mismatch', (g) => { g.decision_inputs.entitlements_match = false; g.checks.entitlements.values_match = false; }],
  ['go-with-unproven-native-load', (g) => { g.decision_inputs.native_utility_load_proven = false; }],
  ['go-with-confounded-runtime', (g) => { g.decision_inputs.runtime_contract_state = 'confounded'; g.checks.runtime_lifecycle.contract_state = 'confounded'; }],
  ['go-with-stale-source-binding', (g) => { g.checks.runtime_lifecycle.bound_to.source_sha = 'othersha'; }],
  ['go-with-stale-tree-binding', (g) => { g.checks.runtime_lifecycle.bound_to.tree_digest = 'f'.repeat(64); }],
  ['go-with-mismatched-runtime-app', (g) => { g.checks.runtime_lifecycle.provenance.app_path = '/elsewhere/Other.app'; }],
  ['go-with-mismatched-runtime-arch', (g) => { g.checks.runtime_lifecycle.provenance.arch = g.arch === 'arm64' ? 'x64' : 'arm64'; }],
  ['nogo-with-passing-runtime', (g) => { g.status = 'no-go'; }],
];
for (const [label, mutate] of FORGED_GATES) {
  const root = buildMatrix(`forged-${label}`, (docs) => mutate(docs['electron-arm64/proof-package.json']));
  const decision = runDecisionWithRoot(root);
  const row = (decision.doc?.observed_rows ?? []).find((r) => r.id === 'SEC1-signed-arm64');
  check(`forged gate (${label}) does not pass`, row?.verdict !== 'PASS', true);
  check(`forged release gate (${label}) does not block development`, decision.status, 'go');
}

// A consistent no-go is accepted: measured failure, consistent inputs.
const nogoRoot = buildMatrix('consistent-nogo', (docs) => {
  const gate = docs['electron-arm64/proof-package.json'];
  gate.status = 'no-go';
  gate.decision_inputs.runtime_contract_state = 'valid-fail';
  gate.decision_inputs.native_utility_load_proven = false;
  gate.checks.runtime_lifecycle.contract_state = 'valid-fail';
  gate.checks.runtime_lifecycle.native_utility_load = { ok: false };
  gate.checks.runtime_lifecycle.checks_summary = gate.checks.runtime_lifecycle.checks_summary.map((c) =>
    c.id === 'RES-1' ? { ...c, ok: false } : c,
  );
});
const nogoDecision = runDecisionWithRoot(nogoRoot);
check(
  'a consistent gate no-go derives FAIL',
  (nogoDecision.doc?.observed_rows ?? []).find((r) => r.id === 'SEC1-signed-arm64')?.verdict,
  'FAIL',
);
check('a consistent release-gate no-go does not block development', nogoDecision.status, 'go');

// A gate whose schema/identity is wrong blocks regardless of a green status.
for (const [label, mutate] of [
  ['bad-schema', (g) => { g.schema = 'other/v1'; }],
  ['bad-bundle-id', (g) => { g.bundle_id = 'com.example.other'; }],
  ['bad-arch', (g) => { g.arch = 'x64'; }],
  ['bad-app-path', (g) => { g.app_realpath = '/evidence/electron-packages/arm64/NotOurApp.app'; }],
]) {
  const root = buildMatrix(`gate-${label}`, (docs) => mutate(docs['electron-arm64/proof-package.json']));
  const decision = runDecisionWithRoot(root);
  check(`release gate identity (${label}) does not block development`, decision.status, 'go');
  check(
    `gate identity (${label}) does not pass the SEC row`,
    (decision.doc?.observed_rows ?? []).find((r) => r.id === 'SEC1-signed-arm64')?.verdict !== 'PASS',
    true,
  );
}

// --- I11: per-arch Electron size rows are independently missing -------------

const halfRoot = buildMatrix('half-size', (docs) => {
  delete docs['electron-x64/electron-size.json'];
});
const halfDecision = runDecisionWithRoot(halfRoot);
check(
  'missing x64 size evidence yields MISSING for x64 only',
  (halfDecision.doc?.observed_rows ?? []).find((r) => r.id === 'PKG2-electron-x64')?.verdict,
  'MISSING',
);
check(
  'arm64 size evidence still derives PASS',
  (halfDecision.doc?.observed_rows ?? []).find((r) => r.id === 'PKG2-electron-arm64')?.verdict,
  'PASS',
);
check('one missing per-arch size row blocks GO', halfDecision.status, 'blocked');

rmSync(TMP, { recursive: true, force: true });

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) {
  for (const failure of failures) console.log(`- ${failure}`);
  process.exit(1);
}
