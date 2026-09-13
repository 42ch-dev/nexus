#!/usr/bin/env node
/**
 * P3-T3 runtime-evidence contract — the single authority shared by the runtime
 * driver (`proof-runtime.mjs`) and the package/decision gate (`proof-package.mjs`).
 *
 * Two consumers, one definition: the driver uses it to decide whether a run may
 * write canonical evidence at all, and the gate uses it to decide whether an
 * existing document is acceptable. Keeping one implementation is the point — a
 * second, looser check in the gate is exactly how a partial run becomes a green
 * "pass" (P3-T3 review C1).
 */

export const RUNTIME_SCHEMA = 'rft-p3-t3-runtime-proof/v2';

/** Exact phase set a canonical (gating) runtime run must execute. */
export const REQUIRED_PHASES = ['launch', 'resources', 'security', 'lifecycle'];

/** Exact check IDs a canonical runtime document must contain, and their sample floors. */
export const REQUIRED_CHECKS = {
  'START-1': { phase: 'launch', samples: { cold: 10, warm: 30 } },
  'RES-1': { phase: 'resources', samples: { soak_seconds: 600 } },
  'RES-2': { phase: 'resources', samples: { cycles: 100 } },
  'SEC-renderer': { phase: 'security', samples: {} },
  'LIFECYCLE-native': { phase: 'lifecycle', samples: {} },
  'LIFECYCLE-fault': { phase: 'lifecycle', samples: {} },
  'PKG-2-electron': { phase: 'all', samples: {} },
};

/** The check that proves a native `.node` actually loaded and served real work. */
export const NATIVE_LOAD_CHECK_ID = 'LIFECYCLE-native';

export const CANONICAL_SAMPLES = { cold: 10, warm: 30, cycles: 100, soakSeconds: 600 };

/** Provenance fields a canonical document must carry to be bindable to an artifact. */
export const REQUIRED_PROVENANCE_FIELDS = [
  'app_path',
  'app_bundle_id',
  'arch',
  'app_bundle_sha256',
  'native_node_sha256',
  'electron_version',
  'packager_version',
  'source_sha',
  'tree_digest',
  'tree_dirty',
  'command',
  'utc_start',
];

/** Confounders that invalidate a measured run rather than recording a product result. */
export const KNOWN_CONFOUNDERS = {
  direct_executable_launch:
    'app was launched by executing Contents/MacOS/<binary> directly instead of via LaunchServices',
};

function isPlainObject(value) {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function sampleValue(doc, checkId, key) {
  if (checkId === 'START-1') {
    const summary = doc.launch?.summary ?? {};
    if (key === 'cold') return summary.cold?.count ?? 0;
    if (key === 'warm') return summary.warm?.count ?? 0;
  }
  if (checkId === 'RES-2') return doc.resources?.cycles?.cycle_count ?? 0;
  if (checkId === 'RES-1') {
    return doc.resources?.soak?.soak_seconds ?? 0;
  }
  return null;
}

/**
 * Evaluate a runtime evidence document against the locked contract.
 *
 * Returns a state in exactly one of:
 *   missing | malformed | incomplete | stale | confounded | valid-pass | valid-fail
 * plus machine-readable reasons. `valid-fail` is a genuine measured failure and
 * must map to no-go; everything else that is not `valid-pass` blocks.
 */
export function evaluateRuntimeEvidence(doc, expectations = {}) {
  const reasons = [];
  if (!doc) return { state: 'missing', reasons: ['runtime evidence absent'], detail: {} };
  if (!isPlainObject(doc)) {
    return { state: 'malformed', reasons: ['runtime evidence is not an object'], detail: {} };
  }
  if (doc.schema !== RUNTIME_SCHEMA) {
    return {
      state: 'malformed',
      reasons: [`runtime schema is ${doc.schema ?? 'absent'}, expected ${RUNTIME_SCHEMA}`],
      detail: { schema: doc.schema ?? null },
    };
  }
  if (!Array.isArray(doc.checks) || doc.checks.length === 0) {
    return { state: 'malformed', reasons: ['runtime evidence carries no checks'], detail: {} };
  }

  // --- completeness: exact phase set, exact check set, sample floors ---------
  const phases = Array.isArray(doc.phases_executed) ? [...doc.phases_executed].sort() : [];
  const expectedPhases = [...REQUIRED_PHASES].sort();
  if (doc.gating !== true) {
    reasons.push('runtime evidence is not marked gating');
  }
  if (phases.join(',') !== expectedPhases.join(',')) {
    reasons.push(`phases_executed=[${phases.join(',')}] expected=[${expectedPhases.join(',')}]`);
  }
  const byId = new Map(doc.checks.map((check) => [check.id, check]));
  for (const [checkId, spec] of Object.entries(REQUIRED_CHECKS)) {
    const check = byId.get(checkId);
    if (!check) {
      reasons.push(`required check missing: ${checkId}`);
      continue;
    }
    for (const [key, floor] of Object.entries(spec.samples)) {
      const actual = sampleValue(doc, checkId, key);
      if (typeof actual !== 'number' || actual < floor) {
        reasons.push(`${checkId} ${key} samples=${actual ?? 'absent'} required>=${floor}`);
      }
    }
  }

  // --- provenance binding ---------------------------------------------------
  const provenance = doc.provenance ?? {};
  for (const field of REQUIRED_PROVENANCE_FIELDS) {
    if (provenance[field] === undefined || provenance[field] === null || provenance[field] === '') {
      reasons.push(`provenance.${field} missing`);
    }
  }
  const mismatches = [];
  for (const [key, expected] of Object.entries(expectations)) {
    if (expected === undefined || expected === null) continue;
    const actual = provenance[key];
    if (actual !== expected) mismatches.push(`${key}: evidence=${actual ?? 'absent'} expected=${expected}`);
  }
  if (mismatches.length > 0) reasons.push(`provenance mismatch: ${mismatches.join('; ')}`);
  if (mismatches.length > 0) {
    return { state: 'stale', reasons, detail: { provenance, mismatches } };
  }

  // --- validity (confounded runs are not product results) --------------------
  const validity = doc.validity ?? {};
  const confounders = Array.isArray(validity.confounders) ? validity.confounders : [];
  if (validity.valid === false) {
    return {
      state: 'confounded',
      reasons: [...reasons, ...(confounders.length > 0 ? confounders.map((c) => `confounder: ${c}`) : ['run marked invalid'])],
      detail: { validity, phases_executed: phases },
    };
  }

  if (reasons.length > 0) {
    return { state: 'incomplete', reasons, detail: { phases_executed: phases } };
  }

  const failed = [...byId.values()].filter((check) => check.ok !== true).map((check) => check.id);
  const nativeLoad = doc.native_utility_load ?? {};
  if (failed.length > 0) {
    return { state: 'valid-fail', reasons: [`failed checks: ${failed.join(', ')}`], detail: { failed } };
  }
  if (nativeLoad.ok !== true) {
    return {
      state: 'valid-fail',
      reasons: ['native utility load not proven in this run'],
      detail: { native_utility_load: nativeLoad },
    };
  }
  return { state: 'valid-pass', reasons: [], detail: { native_utility_load: nativeLoad } };
}

/** Map a contract state onto the exact decision vocabulary. */
export function decisionForState(state, { signedOk, stapleOk, hardenedOk }) {
  if (state === 'valid-pass') {
    if (signedOk && stapleOk && hardenedOk) return 'go';
    return 'blocked';
  }
  if (state === 'valid-fail') return 'no-go';
  return 'blocked';
}
