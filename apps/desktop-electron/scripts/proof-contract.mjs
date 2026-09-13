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
 *
 * The evaluator is the sole pass authority, and it validates the *raw*
 * observations (per-launch latencies, per-cycle durations, the resource trace)
 * rather than the summary numbers derived from them: a document that carries the
 * required check IDs with inflated summary counts but no raw samples is not
 * evidence (P3-T3 review C2).
 */

export const RUNTIME_SCHEMA = 'rft-p3-t3-runtime-proof/v2';

/** Exact phase set a canonical (gating) runtime run must execute. */
export const REQUIRED_PHASES = ['launch', 'resources', 'security', 'lifecycle'];

/** Exact check IDs a canonical runtime document must contain — no more, no fewer. */
export const REQUIRED_CHECKS = [
  'START-1',
  'RES-1',
  'RES-2',
  'SEC-renderer',
  'LIFECYCLE-native',
  'LIFECYCLE-fault',
  'PKG-2-electron',
];

/** The check that proves a native `.node` actually loaded and served real work. */
export const NATIVE_LOAD_CHECK_ID = 'LIFECYCLE-native';

/**
 * Canonical sample plan. These are not floors to be padded around: a canonical
 * run records exactly this many observations, so a canonical document must
 * carry exactly this many raw samples.
 */
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
  utility_owner_unavailable:
    'the packaged Electron utility owner exited or never became ready, so no native work was served',
  run_deadline_exceeded: 'the run exceeded its top-level deadline before completing every phase',
};

function isPlainObject(value) {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function getPath(root, path) {
  return path.split('.').reduce((acc, key) => (acc === undefined || acc === null ? undefined : acc[key]), root);
}

/**
 * Nearest-rank summary over a sample array. Shared with the driver so the raw
 * samples and the summary can never be produced by two different algorithms —
 * and so the contract can recompute the summary and reject a fabricated one.
 */
export function summarise(values) {
  const numbers = values.filter((value) => typeof value === 'number' && Number.isFinite(value)).sort((a, b) => a - b);
  if (numbers.length === 0) return { count: 0, min: null, p50: null, p95: null, max: null };
  const nearestRank = (q) => numbers[Math.min(numbers.length - 1, Math.ceil(q * numbers.length) - 1)];
  return {
    count: numbers.length,
    min: numbers[0],
    p50: nearestRank(0.5),
    p95: nearestRank(0.95),
    max: numbers[numbers.length - 1],
  };
}

function sameSummary(a, b) {
  if (!isPlainObject(a) || !isPlainObject(b)) return false;
  return ['count', 'min', 'p50', 'p95', 'max'].every((key) => a[key] === b[key]);
}

/**
 * Read `count` raw observations of `valueKey` (or the entry itself when
 * `valueKey` is null) from an array, requiring every value to be finite.
 */
function readRawSamples(root, path, count, valueKey, reasons) {
  const array = getPath(root, path);
  if (!Array.isArray(array)) {
    reasons.push(`${path} is not a raw sample array`);
    return null;
  }
  if (array.length !== count) {
    reasons.push(`${path} has ${array.length} raw samples, canonical run records exactly ${count}`);
    return null;
  }
  const values = [];
  for (let i = 0; i < array.length; i += 1) {
    const entry = valueKey === null ? array[i] : array[i]?.[valueKey];
    if (typeof entry !== 'number' || !Number.isFinite(entry)) {
      reasons.push(`${path}[${i}]${valueKey === null ? '' : `.${valueKey}`} is not a finite number`);
      return null;
    }
    values.push(entry);
  }
  return values;
}

/** Require at least `count` finite observations; used for traces that poll continuously. */
function readTrace(root, path, count, valueKey, reasons) {
  const array = getPath(root, path);
  if (!Array.isArray(array)) {
    reasons.push(`${path} is not a raw trace array`);
    return null;
  }
  if (array.length < count) {
    reasons.push(`${path} has ${array.length} trace samples, requires >= ${count}`);
    return null;
  }
  const values = [];
  for (let i = 0; i < array.length; i += 1) {
    const entry = valueKey === null ? array[i] : array[i]?.[valueKey];
    if (typeof entry !== 'number' || !Number.isFinite(entry)) {
      reasons.push(`${path}[${i}]${valueKey === null ? '' : `.${valueKey}`} is not a finite number`);
      return null;
    }
    values.push(entry);
  }
  return values;
}

/** Raw-observation requirements per check, plus the summary that must be derivable from them. */
function validateRawObservations(doc, reasons) {
  // START-1 — per-launch latencies, recomputed against the recorded summary.
  const coldMs = readRawSamples(doc, 'launch.samples.cold', CANONICAL_SAMPLES.cold, 'ms', reasons);
  const warmMs = readRawSamples(doc, 'launch.samples.warm', CANONICAL_SAMPLES.warm, 'ms', reasons);
  if (coldMs && !sameSummary(summarise(coldMs), getPath(doc, 'launch.summary.cold'))) {
    reasons.push('launch.summary.cold does not match the raw cold samples');
  }
  if (warmMs && !sameSummary(summarise(warmMs), getPath(doc, 'launch.summary.warm'))) {
    reasons.push('launch.summary.warm does not match the raw warm samples');
  }

  // RES-1 — soak duration, owned-process trace, and the workload counters actually run.
  const soakSeconds = getPath(doc, 'resources.soak.soak_seconds');
  if (soakSeconds !== CANONICAL_SAMPLES.soakSeconds) {
    reasons.push(`resources.soak.soak_seconds=${soakSeconds ?? 'absent'} required=${CANONICAL_SAMPLES.soakSeconds}`);
  }
  const traceRss = readTrace(doc, 'resources.soak.trace', 100, 'rss_bytes', reasons);
  if (traceRss && !sameSummary(summarise(traceRss), getPath(doc, 'resources.soak.summary'))) {
    reasons.push('resources.soak.summary does not match the raw trace');
  }
  const workload = getPath(doc, 'resources.soak.workload');
  if (!isPlainObject(workload)) {
    reasons.push('resources.soak.workload is absent');
  } else {
    for (const key of ['reads', 'writes']) {
      if (typeof workload[key] !== 'number' || !Number.isFinite(workload[key])) {
        reasons.push(`resources.soak.workload.${key} is not a finite number`);
      }
    }
  }

  // RES-2 — per-cycle durations and the periodic RSS samples.
  const cycleMs = readRawSamples(doc, 'resources.cycles.durations_ms', CANONICAL_SAMPLES.cycles, null, reasons);
  if (cycleMs && !sameSummary(summarise(cycleMs), getPath(doc, 'resources.cycles.summary'))) {
    reasons.push('resources.cycles.summary does not match the raw cycle durations');
  }
  readTrace(doc, 'resources.cycles.samples', 10, 'rss_bytes', reasons);
  const plateau = getPath(doc, 'resources.cycles.plateau_after_cooldown');
  if (!isPlainObject(plateau) || typeof plateau.rss_bytes !== 'number' || !Number.isFinite(plateau.rss_bytes)) {
    reasons.push('resources.cycles.plateau_after_cooldown.rss_bytes is not a finite number');
  }
  if (typeof getPath(doc, 'resources.cycles.retained_growth_bytes') !== 'number') {
    reasons.push('resources.cycles.retained_growth_bytes is not a number');
  }
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

  // --- completeness: exact phase set, exact check ID set, raw observations ----
  const phases = Array.isArray(doc.phases_executed) ? [...doc.phases_executed].sort() : [];
  const expectedPhases = [...REQUIRED_PHASES].sort();
  if (doc.gating !== true) {
    reasons.push('runtime evidence is not marked gating');
  }
  if (phases.join(',') !== expectedPhases.join(',')) {
    reasons.push(`phases_executed=[${phases.join(',')}] expected=[${expectedPhases.join(',')}]`);
  }

  // Exact set equality: a duplicate can hide an earlier failure behind the
  // `Map` lookup, and an extra ID means the document is not the locked shape.
  const ids = doc.checks.map((check) => check?.id);
  const duplicates = [...new Set(ids.filter((id, index) => ids.indexOf(id) !== index))];
  if (duplicates.length > 0) reasons.push(`duplicate check IDs: ${duplicates.join(', ')}`);
  const missing = REQUIRED_CHECKS.filter((id) => !ids.includes(id));
  if (missing.length > 0) reasons.push(`required check missing: ${missing.join(', ')}`);
  const extra = [...new Set(ids.filter((id) => !REQUIRED_CHECKS.includes(id)))];
  if (extra.length > 0) reasons.push(`unexpected check IDs: ${extra.join(', ')}`);

  validateRawObservations(doc, reasons);

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
  if (mismatches.length > 0) {
    return { state: 'stale', reasons: [...reasons, `provenance mismatch: ${mismatches.join('; ')}`], detail: { provenance, mismatches } };
  }

  // --- validity (confounded runs are not product results) --------------------
  // Checked before the completeness verdict: a run that admits it was
  // confounded is a diagnosis, not a shape complaint, so its confounders are
  // the reason. Provenance is still checked first — stale evidence is stale
  // regardless of how the run went.
  const validity = doc.validity ?? {};
  const confounders = Array.isArray(validity.confounders) ? validity.confounders : [];
  if (validity.valid === false) {
    return {
      state: 'confounded',
      reasons: confounders.length > 0 ? confounders.map((c) => `confounder: ${c}`) : ['run marked invalid'],
      detail: {
        validity,
        phases_executed: phases,
        // Shape problems are recorded, not hidden, but they are not the headline:
        // the run already declared its measurements unusable.
        secondary_shape_problems: reasons,
      },
    };
  }

  if (reasons.length > 0) {
    return { state: 'incomplete', reasons, detail: { phases_executed: phases } };
  }

  const byId = new Map(doc.checks.map((check) => [check.id, check]));
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

/**
 * A provider operation counts as a completed lifecycle only when every
 * observable step succeeded: probe, launch, execute, a drain that saw at least
 * one `MessageDelta` and exactly one terminal event for the same operation, a
 * successful cancel, and a successful shutdown. A dispatched request whose
 * event pull or shutdown failed is not an observed lifecycle (P3-T3 review I7).
 */
export function providerLifecycleComplete(op) {
  const steps = op?.steps ?? {};
  const pull = steps.pull;
  return Boolean(
    steps.probe?.ok === true &&
      steps.launch?.ok === true &&
      steps.execute?.ok === true &&
      pull?.ok === true &&
      pull.deltas >= 1 &&
      pull.terminal_count === 1 &&
      pull.terminal_matches_operation === true &&
      steps.cancel?.ok === true &&
      steps.shutdown?.ok === true,
  );
}
