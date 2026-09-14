#!/usr/bin/env node
/**
 * P3-T3 runtime-evidence contract — the single authority shared by the runtime
 * driver (`proof-runtime.mjs`) and the package/decision gates.
 *
 * Two consumers, one definition: the driver uses it to decide whether a run may
 * write canonical evidence at all, and the gates use it to decide whether an
 * existing document is acceptable. Keeping one implementation is the point — a
 * second, looser check is exactly how a partial run becomes a green "pass".
 *
 * The evaluator is the sole pass authority. It validates the *raw* observations
 * (per-launch latencies, per-cycle durations, the resource trace) rather than the
 * summaries derived from them (C2), the canonical run metadata — mode, status and
 * sample plan — rather than trusting that a document merely looks complete (C5),
 * and the provenance binding against exact source/tree/artifact expectations
 * rather than shape alone (I9).
 */

import { createHash } from 'node:crypto';
import { existsSync, lstatSync, readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';

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
 * Canonical sample plan. A canonical run records exactly this many
 * observations, so a canonical document must carry exactly this many raw
 * samples and declare exactly this plan.
 */
export const CANONICAL_SAMPLES = { cold: 10, warm: 30, cycles: 100, soakSeconds: 600 };

/** The `sample_plan` object a canonical document must declare, verbatim. */
export const CANONICAL_SAMPLE_PLAN = {
  cold: CANONICAL_SAMPLES.cold,
  warm: CANONICAL_SAMPLES.warm,
  cycles: CANONICAL_SAMPLES.cycles,
  soak_seconds: CANONICAL_SAMPLES.soakSeconds,
};

/** The only run mode permitted to gate a decision. */
export const CANONICAL_MODE = 'canonical';

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

/** Provenance fields an expectation may pin; a mismatch makes evidence stale. */
export const BINDABLE_PROVENANCE_FIELDS = [
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
];

/** Confounders that invalidate a measured run rather than recording a product result. */
export const KNOWN_CONFOUNDERS = {
  direct_executable_launch:
    'app was launched by executing Contents/MacOS/<binary> directly instead of via LaunchServices',
  utility_owner_unavailable:
    'the packaged Electron utility owner exited or never became ready, so no native work was served',
  run_deadline_exceeded: 'the run exceeded its top-level deadline before completing every phase',
  run_interrupted: 'the run was interrupted by a signal before completing every phase',
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
  const coldMs = readRawSamples(doc, 'launch.samples.cold', CANONICAL_SAMPLES.cold, 'ms', reasons);
  const warmMs = readRawSamples(doc, 'launch.samples.warm', CANONICAL_SAMPLES.warm, 'ms', reasons);
  if (coldMs && !sameSummary(summarise(coldMs), getPath(doc, 'launch.summary.cold'))) {
    reasons.push('launch.summary.cold does not match the raw cold samples');
  }
  if (warmMs && !sameSummary(summarise(warmMs), getPath(doc, 'launch.summary.warm'))) {
    reasons.push('launch.summary.warm does not match the raw warm samples');
  }

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

  // --- canonical run metadata (C5) ------------------------------------------
  // Only an exact canonical run may gate, and the document has to say so. A
  // diagnostic document that happens to carry canonical-shaped observations is
  // still not canonical evidence.
  const meta = [];
  if (doc.mode !== CANONICAL_MODE) meta.push(`mode is ${doc.mode ?? 'absent'}, expected ${CANONICAL_MODE}`);
  if (doc.gating !== true) meta.push('gating is not true');
  if (JSON.stringify(doc.sample_plan ?? null) !== JSON.stringify(CANONICAL_SAMPLE_PLAN)) {
    meta.push(`sample_plan ${JSON.stringify(doc.sample_plan ?? null)} != ${JSON.stringify(CANONICAL_SAMPLE_PLAN)}`);
  }
  if (meta.length > 0) {
    return { state: 'malformed', reasons: meta, detail: { mode: doc.mode ?? null, sample_plan: doc.sample_plan ?? null } };
  }
  if (!Array.isArray(doc.checks) || doc.checks.length === 0) {
    return { state: 'malformed', reasons: ['runtime evidence carries no checks'], detail: {} };
  }

  // --- completeness: exact phase set, exact check ID set, raw observations ----
  const phases = Array.isArray(doc.phases_executed) ? [...doc.phases_executed].sort() : [];
  const expectedPhases = [...REQUIRED_PHASES].sort();
  if (phases.join(',') !== expectedPhases.join(',')) {
    reasons.push(`phases_executed=[${phases.join(',')}] expected=[${expectedPhases.join(',')}]`);
  }

  const ids = doc.checks.map((check) => check?.id);
  const duplicates = [...new Set(ids.filter((id, index) => ids.indexOf(id) !== index))];
  if (duplicates.length > 0) reasons.push(`duplicate check IDs: ${duplicates.join(', ')}`);
  const missing = REQUIRED_CHECKS.filter((id) => !ids.includes(id));
  if (missing.length > 0) reasons.push(`required check missing: ${missing.join(', ')}`);
  const extra = [...new Set(ids.filter((id) => !REQUIRED_CHECKS.includes(id)))];
  if (extra.length > 0) reasons.push(`unexpected check IDs: ${extra.join(', ')}`);

  validateRawObservations(doc, reasons);

  // --- provenance binding (I9) ----------------------------------------------
  const provenance = doc.provenance ?? {};
  for (const field of REQUIRED_PROVENANCE_FIELDS) {
    if (provenance[field] === undefined || provenance[field] === null || provenance[field] === '') {
      reasons.push(`provenance.${field} missing`);
    }
  }
  const mismatches = [];
  for (const [key, expected] of Object.entries(expectations)) {
    if (!BINDABLE_PROVENANCE_FIELDS.includes(key)) continue;
    if (expected === undefined || expected === null) continue;
    if (provenance[key] !== expected) {
      mismatches.push(`${key}: evidence=${provenance[key] ?? 'absent'} expected=${expected}`);
    }
  }
  const recordedConfounders = Array.isArray(doc.validity?.confounders) ? doc.validity.confounders : null;

  // Provenance is checked before validity: evidence measured against a different
  // artifact or revision is stale no matter how the run itself went. The
  // document's own confounders travel with the verdict so nothing is lost.
  if (mismatches.length > 0) {
    return {
      state: 'stale',
      reasons: [...reasons, `provenance mismatch: ${mismatches.join('; ')}`],
      detail: { provenance, mismatches, recorded_confounders: recordedConfounders },
    };
  }

  // --- validity: confounded runs are not product results (C6) ----------------
  // Any confounder — recorded, or implied by a non-LaunchServices launch method
  // — makes the run non-product *regardless* of the boolean the document asserts.
  // A document claiming `valid: true` alongside a direct launch or a non-empty
  // confounder list is internally contradictory and must never reach valid-pass.
  const launchMethod = doc.validity?.launch_method;
  const implicitConfounders = [];
  if (launchMethod !== 'launchservices') {
    implicitConfounders.push(
      launchMethod === 'direct'
        ? 'direct_executable_launch'
        : `unrecognized_launch_method:${launchMethod ?? 'absent'}`,
    );
  }
  const allConfounders = [...new Set([...(recordedConfounders ?? []), ...implicitConfounders])];
  const declaredInvalid = doc.validity?.valid !== true;
  if (declaredInvalid || allConfounders.length > 0) {
    const reasonsOut = allConfounders.length > 0 ? allConfounders.map((c) => `confounder: ${c}`) : [];
    if (doc.validity?.valid === true && allConfounders.length > 0) {
      reasonsOut.push('validity.valid is true but the run carries confounders');
    }
    if (doc.validity?.valid === undefined) {
      reasonsOut.push('validity.valid is absent');
    }
    return {
      state: 'confounded',
      reasons: reasonsOut.length > 0 ? reasonsOut : ['run marked invalid'],
      detail: {
        validity: doc.validity,
        phases_executed: phases,
        recorded_confounders: recordedConfounders,
        effective_confounders: allConfounders,
        secondary_shape_problems: reasons,
      },
    };
  }

  if (reasons.length > 0) {
    return { state: 'incomplete', reasons, detail: { phases_executed: phases } };
  }

  // --- terminal verdicts, with the document's own status required to agree ----
  const byId = new Map(doc.checks.map((check) => [check.id, check]));
  const failed = [...byId.values()].filter((check) => check.ok !== true).map((check) => check.id);
  if (failed.length > 0) {
    if (doc.status !== 'fail') {
      return {
        state: 'malformed',
        reasons: [`status ${doc.status ?? 'absent'} contradicts failed checks: ${failed.join(', ')}`],
        detail: { failed },
      };
    }
    return { state: 'valid-fail', reasons: [`failed checks: ${failed.join(', ')}`], detail: { failed } };
  }
  const nativeLoad = doc.native_utility_load ?? {};
  if (nativeLoad.ok !== true) {
    if (doc.status !== 'fail') {
      return {
        state: 'malformed',
        reasons: [`status ${doc.status ?? 'absent'} contradicts unproven native utility load`],
        detail: { native_utility_load: nativeLoad },
      };
    }
    return {
      state: 'valid-fail',
      reasons: ['native utility load not proven in this run'],
      detail: { native_utility_load: nativeLoad },
    };
  }
  if (doc.status !== 'pass') {
    return {
      state: 'malformed',
      reasons: [`status ${doc.status ?? 'absent'} contradicts a fully passing run`],
      detail: { status: doc.status ?? null },
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

/** Per-criterion verdicts from a canonical runtime document. */
export function runtimeCriterionVerdict(state, doc, checkId) {
  if (state === 'valid-pass' || state === 'valid-fail') {
    const check = (doc?.checks ?? []).find((entry) => entry?.id === checkId);
    if (!check) return 'NOT OBSERVED';
    return check.ok === true ? 'PASS' : 'FAIL';
  }
  return 'NOT OBSERVED';
}

/**
 * A provider operation counts as a completed lifecycle only when every
 * observable step succeeded: probe, launch, execute, a drain that saw at least
 * one `MessageDelta` and exactly one terminal event for the same operation, a
 * successful cancel, and a successful shutdown (I7).
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

// --- artifact identity ------------------------------------------------------

/** SHA-256 of one file, as lowercase hex. */
export function sha256File(path) {
  return createHash('sha256').update(readFileSync(path)).digest('hex');
}

/**
 * Walk a tree without following symlinks.
 *
 * macOS `.app` bundles are full of version symlinks, so following them would
 * count the same bytes several times and make the digest unstable.
 */
export function walkFiles(root) {
  if (!existsSync(root)) return { files: [], symlinks: [] };
  const files = [];
  const symlinks = [];
  const queue = [root];
  while (queue.length) {
    const dir = queue.pop();
    for (const entry of readdirSync(dir)) {
      const full = join(dir, entry);
      const st = lstatSync(full);
      if (st.isSymbolicLink()) symlinks.push(full);
      else if (st.isDirectory()) queue.push(full);
      else files.push(full);
    }
  }
  return { files, symlinks };
}

/**
 * Deterministic digest of an installed bundle: sorted relative path plus the
 * content digest of each file. Shared by the runtime driver (which records it)
 * and the decision generator (which recomputes it), so "the app that was
 * measured" and "the app on disk now" are compared with one algorithm.
 */
export function digestAppBundle(appPath) {
  const walked = walkFiles(appPath);
  const hash = createHash('sha256');
  for (const path of walked.files.map((p) => p.replace(appPath, '')).sort()) {
    hash.update(path).update('\0');
    hash.update(sha256File(join(appPath, path.replace(/^\//, ''))));
  }
  return {
    sha256: hash.digest('hex'),
    file_count: walked.files.length,
    symlink_count: walked.symlinks.length,
  };
}
