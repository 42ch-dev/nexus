/**
 * P3-T3 upstream request-budget guard — contract §6.3 of
 * `.mstar/iterations/v1.195/specs/current-host-contracts.md`.
 *
 * This file is a **Node ESM preload**, not a library: loading it installs the
 * ceiling in the current process, and it exports nothing. The supported load
 * mechanism is the one the contract fixes — the driver puts
 *
 *   NODE_OPTIONS=--import=<absolute path to this file>
 *
 * on the environment of each owned real-dsh child (the service child that dsh
 * inherits from, plus every other process in that tree, which is why a
 * non-dsh process is classified rather than silently exempted).
 *
 * What it guarantees, in order:
 *
 *  1. `globalThis.fetch` is replaced before dsh imports. The replacement is a
 *     non-configurable accessor whose setter is ignored, so a later
 *     `fetch = …` polyfill can neither drop the ceiling nor crash the process,
 *     and `delete`/`defineProperty` attempts fail. Every dispatch goes through
 *     the gate.
 *  2. At most ONE request reaches the original fetch in a whole attempt. The
 *     slot is a file named `spent` inside the attempt directory, taken with a
 *     filesystem exclusive create (`wx`) BEFORE the original fetch is called;
 *     that is atomic across processes, is consumed even when DNS/TLS/HTTP
 *     fails, is never reset by the guard (no unlink, no recreate, no reset
 *     path) and therefore survives a crash, a restart and a second launch.
 *  3. The only admissible request is `POST` to the exact preselected model URL
 *     — canonical `href` equality, no wildcard host or suffix matching. The
 *     path is pinned to `/chat/completions`, so `/models`, `/files`, telemetry
 *     and every other endpoint is denied before any network I/O, and cleartext
 *     is accepted only for the deterministic loopback origin (a non-loopback
 *     `http:` selection is a configuration refusal, never cleartext egress).
 *  4. Denials never reach the network and never consume the slot. Anything the
 *     guard cannot classify, cannot resolve, cannot record or cannot verify
 *     fails closed: malformed env, an unusable attempt directory, a missing
 *     `fetch`, a non-dsh runtime, an unclassifiable argument, an unexpected URL
 *     or method, a spent slot, and filesystem errors are all denials.
 *  5. The request is forwarded opaque: `originalFetch(input, {...init,
 *     redirect: 'error'})`. `headers`, `body` and `signal` are the caller's own
 *     objects (same references, uninspected, unmodified); only `redirect` is
 *     forced to `error`, so a redirect answer cannot become a second request.
 *     Nothing is retried — the guard never calls the transport twice.
 *  6. Evidence failures are terminal and never silent. If any event cannot be
 *     persisted (loaded, denied or admitted) the attempt is tainted: the
 *     `<attempt-dir>/evidence-failed` marker is written, every later request is
 *     denied before dispatch, and an admission whose own record could not be
 *     written is never dispatched. A run whose records are incomplete therefore
 *     cannot be read as a qualified one by counting events. If even the marker
 *     cannot be written, or if the ceiling cannot be **locked** (a pre-existing
 *     non-configurable `fetch`), the guard terminates the child with exit code
 *     78 instead of continuing — a replaceable wrapper or an unrecorded
 *     attempt must never be able to leave a green result behind.
 *
 * Evidence (credential-free, in the attempt directory the driver allocates
 * once and never resets):
 *
 *   <attempt-dir>/spent
 *     `{"schema":"nexus-request-guard-spent/1","pid":<pid>,"at":<iso>,
 *       "category":"model_request_admitted"}` — the consumed slot.
 *   <attempt-dir>/events/<kind>-<pid>-<counter>-<8 hex>.json
 *     `{"schema":"nexus-request-guard-event/1","kind":"loaded"|"admitted"|
 *       "denied","runtime":"dsh"|"other","category":<short enum>,
 *       "at":<iso>}` plus, on `loaded`, `"problems":[<fixed labels>]`.
 *     One file per event, written with `wx`, never rewritten.
 *   <attempt-dir>/evidence-failed
 *     `{"schema":"nexus-request-guard-evidence-failed/1","reason":<fixed enum>,
 *       "kind":<failed event kind or null>,"pid":<pid>,"at":<iso>}` — the
 *     terminal marker of an attempt whose evidence could not be persisted. Its
 *     mere presence disqualifies the attempt, whatever the event counts say.
 *     Written once with `wx`, never rewritten; `reason` is `event_write_failed`
 *     or `fetch_not_replaceable`.
 *
 * The guard reads only the three `NEXUS_WORKFLOW_*` variables below, plus
 * `process.argv[1]`. It never reads, copies, logs or persists a credential, a
 * URL, a header, a body, a response or an environment dump — not into the
 * evidence and not anywhere else. An event carries a fixed category string and
 * nothing derived from the request.
 *
 * Scope: this bounds the known model transport of the supported sealed runtime
 * (global `fetch`). It is not a network sandbox for hostile code, and it does
 * not cover a runtime that dispatches through another primitive — that drift
 * shows up as a missing `admitted` event and must block the run rather than be
 * worked around.
 */

import { existsSync, mkdirSync, realpathSync, statSync, writeFileSync } from 'node:fs';
import { randomUUID } from 'node:crypto';
import { dirname, isAbsolute, join, parse, resolve, sep } from 'node:path';

/** Event file schema identifier. */
const EVENT_SCHEMA = 'nexus-request-guard-event/1';
/** Consumed-slot file schema identifier. */
const SPENT_SCHEMA = 'nexus-request-guard-spent/1';
/** Fixed basename of the consumed slot inside the attempt directory. */
const SPENT_NAME = 'spent';
/** Marker file schema written when evidence could not be persisted. */
const MARKER_SCHEMA = 'nexus-request-guard-evidence-failed/1';
/** Fixed basename of the terminal evidence-failure marker. */
const MARKER_NAME = 'evidence-failed';
/** Exit code used when the child must die rather than continue unrecorded. */
const UNRECORDABLE_EXIT_CODE = 78;
/** Marker `reason` labels (fixed vocabulary). */
const MARKER_REASONS = Object.freeze({
  eventWriteFailed: 'event_write_failed',
  fetchNotReplaceable: 'fetch_not_replaceable',
});
/** The only model path this guard ever admits (contract §6.3 item 2). */
const MODEL_PATH = '/chat/completions';
/** Category recorded with the single admitted request. */
const ADMITTED_CATEGORY = 'model_request_admitted';
/** Category recorded by every process that loads the guard. */
const LOADED_CATEGORY = 'guard_loaded';
/** Attempt-directory permission bits other than the owner's must be clear. */
const OWNER_ONLY_MASK = 0o077;
/** Hostnames whose cleartext origin is the deterministic loopback test origin. */
const LOOPBACK_HOSTNAMES = new Set(['localhost', '[::1]', '::1']);
/** Bound on the walk from the dsh entry up to its package root. */
const MAX_PACKAGE_WALK = 12;

/** The exact environment contract; no other variable is read. */
const ENV_KEYS = Object.freeze({
  attemptDir: 'NEXUS_WORKFLOW_ATTEMPT_DIR',
  allowedUrl: 'NEXUS_WORKFLOW_ALLOWED_URL',
  dshRealpath: 'NEXUS_WORKFLOW_DSH_REALPATH',
});

/** The complete denial-category vocabulary (short, fixed, secret-free). */
const DENY = Object.freeze({
  malformedEnv: 'malformed_env',
  attemptDirMissing: 'attempt_dir_missing',
  attemptDirInsecure: 'attempt_dir_insecure',
  unsupportedTransport: 'unsupported_transport',
  foreignRuntime: 'foreign_runtime',
  packageChildRuntime: 'package_child_runtime',
  unclassifiableRequest: 'unclassifiable_request',
  unexpectedUrl: 'unexpected_url',
  unexpectedMethod: 'unexpected_method',
  alreadySpent: 'attempt_already_spent',
  filesystemError: 'filesystem_error',
  evidenceWriteFailed: 'evidence_write_failed',
});

/**
 * Fixed `problems` labels that make the whole configuration malformed: with any
 * of them present the guard cannot establish what to admit at all, so the
 * request is denied as `malformed_env` rather than being classified.
 */
const MALFORMED_LABELS = new Set([
  'not_node_runtime',
  ...Object.keys(ENV_KEYS).map((field) => `env_missing:${field}`),
  'dsh_realpath_not_absolute',
  'dsh_entry_unresolvable',
  'allowed_url_malformed',
  'allowed_url_not_http',
  'allowed_url_insecure_transport',
  'allowed_url_has_userinfo',
  'allowed_url_has_query_or_fragment',
  'allowed_url_not_model_endpoint',
  'allowed_url_no_host',
]);

const IS_NODE =
  typeof process === 'object' &&
  process !== null &&
  typeof process.versions === 'object' &&
  process.versions !== null &&
  typeof process.versions.node === 'string';

/** Denial surfaced to the caller as a rejected `fetch`. */
class RequestGuardDenied extends Error {
  /** @param {string} category fixed denial category */
  constructor(category) {
    super(`nexus request guard denied (${category})`);
    this.name = 'NexusRequestGuardError';
    this.code = category;
  }
}

// ---------------------------------------------------------------------------
// Configuration resolution (all of it, once, before the ceiling is installed)
// ---------------------------------------------------------------------------

/**
 * Read the three contract variables. Only the NAME is dereferenced; no other
 * variable (and never a credential-shaped one) is touched.
 */
function readGuardEnv() {
  const values = {};
  const problems = [];
  for (const [field, key] of Object.entries(ENV_KEYS)) {
    const raw = process.env[key];
    if (typeof raw !== 'string' || raw.trim() === '') {
      problems.push(`env_missing:${field}`);
      continue;
    }
    values[field] = raw.trim();
  }
  return { values, problems };
}

/** Canonical real path of a filesystem path, or null when it does not resolve. */
function canonicalPath(value) {
  try {
    return realpathSync(isAbsolute(value) ? value : resolve(value));
  } catch {
    return null;
  }
}

/** The directory of the nearest enclosing `package.json`, or null. */
function packageRootOf(filePath) {
  let current = dirname(filePath);
  for (let depth = 0; depth < MAX_PACKAGE_WALK; depth += 1) {
    if (existsSync(join(current, 'package.json'))) return current;
    const parent = dirname(current);
    if (parent === current || current === parse(current).root) return null;
    current = parent;
  }
  return null;
}

/**
 * Classify the process that loaded this guard by the identity of its entry
 * module (`process.argv[1]`), canonicalized so an npm bin symlink resolves to
 * the installed entry the driver recorded.
 *
 * `dsh` is the only runtime that may be admitted. Any other entry is `other`
 * and denied; when that entry sits inside the installed dsh package directory
 * the denial is labelled `package_child_runtime`, which is the same decision
 * with a drift diagnostic attached (a grandchild of dsh doing the fetch is
 * exactly the case that must be visible rather than mysterious).
 */
function classifyRuntime(dshCanonical) {
  const observed = resolveObservedEntry();
  if (dshCanonical !== null && observed !== null && observed === dshCanonical) {
    return { runtime: 'dsh', foreignCategory: null };
  }
  const packageRoot = dshCanonical === null ? null : packageRootOf(dshCanonical);
  const withinInstalledPackage =
    packageRoot !== null && observed !== null && observed.startsWith(packageRoot + sep);
  return {
    runtime: 'other',
    foreignCategory: withinInstalledPackage ? DENY.packageChildRuntime : DENY.foreignRuntime,
  };
}

/** Canonical real path of this process's entry module, or null. */
function resolveObservedEntry() {
  if (!IS_NODE || !Array.isArray(process.argv)) return null;
  const entry = process.argv[1];
  if (typeof entry !== 'string' || entry === '') return null;
  return canonicalPath(entry);
}

/**
 * The attempt directory must already exist, be owned by this user and carry no
 * group/other permission bits. The guard never creates it: that is what makes
 * "a second run cannot recreate the evidence directory" true.
 */
function validateAttemptDir(dir) {
  if (!isAbsolute(dir)) return { category: DENY.attemptDirMissing, problem: 'attempt_dir_not_absolute' };
  let stats;
  try {
    stats = statSync(dir);
  } catch {
    return { category: DENY.attemptDirMissing, problem: 'attempt_dir_missing' };
  }
  if (!stats.isDirectory()) return { category: DENY.attemptDirMissing, problem: 'attempt_dir_not_a_directory' };
  if ((stats.mode & OWNER_ONLY_MASK) !== 0) {
    return { category: DENY.attemptDirInsecure, problem: 'attempt_dir_not_owner_only' };
  }
  if (typeof process.getuid === 'function' && stats.uid !== process.getuid()) {
    return { category: DENY.attemptDirInsecure, problem: 'attempt_dir_other_owner' };
  }
  return { category: null, problem: null };
}

/**
 * The selected URL must be an absolute URL addressed at the pinned model path,
 * with no userinfo, query or fragment. Its canonical `href` is then the exact
 * string an incoming request has to equal.
 *
 * Cleartext is only ever the deterministic loopback test origin (§6.3 item 2);
 * a non-loopback `http:` origin is refused as a configuration fault so a
 * mis-set driver URL cannot turn a live model call into cleartext egress.
 */
function validateAllowedUrl(raw) {
  let url;
  try {
    url = new URL(raw);
  } catch {
    return { href: null, problem: 'allowed_url_malformed' };
  }
  if (url.protocol !== 'http:' && url.protocol !== 'https:') {
    return { href: null, problem: 'allowed_url_not_http' };
  }
  if (url.hostname === '') return { href: null, problem: 'allowed_url_no_host' };
  if (url.protocol === 'http:' && !isLoopbackHostname(url.hostname)) {
    return { href: null, problem: 'allowed_url_insecure_transport' };
  }
  if (url.username !== '' || url.password !== '') {
    return { href: null, problem: 'allowed_url_has_userinfo' };
  }
  if (url.search !== '' || url.hash !== '') {
    return { href: null, problem: 'allowed_url_has_query_or_fragment' };
  }
  if (url.pathname !== MODEL_PATH) return { href: null, problem: 'allowed_url_not_model_endpoint' };
  return { href: url.href, problem: null };
}

/** Is this the deterministic loopback origin (`127.0.0.0/8`, `localhost`, `::1`)? */
function isLoopbackHostname(hostname) {
  if (LOOPBACK_HOSTNAMES.has(hostname)) return true;
  return /^127(\.\d{1,3}){3}$/.test(hostname);
}

/**
 * Resolve the whole configuration. Every problem is collected (the `loaded`
 * event reports all of them); the first one in priority order becomes the
 * denial category for an attempted fetch.
 */
function initialize() {
  const state = {
    runtime: 'other',
    foreignCategory: DENY.foreignRuntime,
    attemptDir: null,
    eventsDir: null,
    allowedHref: null,
    problems: [],
    configError: null,
    transportOk: false,
    eventCounter: 0,
    tainted: false,
    taintReason: null,
    markerWritten: false,
  };

  if (!IS_NODE) {
    state.problems.push('not_node_runtime');
    state.configError = DENY.malformedEnv;
    return state;
  }

  const { values, problems } = readGuardEnv();
  state.problems.push(...problems);

  // Runtime identity. An unresolved recorded entry means the identity itself is
  // unknown — a configuration failure, never an admission.
  let dshCanonical = null;
  if (values.dshRealpath !== undefined) {
    if (!isAbsolute(values.dshRealpath)) {
      state.problems.push('dsh_realpath_not_absolute');
    } else {
      dshCanonical = canonicalPath(values.dshRealpath);
      if (dshCanonical === null) state.problems.push('dsh_entry_unresolvable');
    }
  }
  const runtime = classifyRuntime(dshCanonical);
  state.runtime = runtime.runtime;
  state.foreignCategory = runtime.foreignCategory ?? DENY.foreignRuntime;

  // Attempt directory: validated, never created.
  let dirCategory = null;
  if (values.attemptDir !== undefined) {
    const dir = validateAttemptDir(values.attemptDir);
    if (dir.category === null) {
      state.attemptDir = values.attemptDir;
      const eventsDir = join(values.attemptDir, 'events');
      try {
        mkdirSync(eventsDir, { recursive: true, mode: 0o700 });
        state.eventsDir = eventsDir;
      } catch {
        state.problems.push('events_dir_unavailable');
      }
    } else {
      state.problems.push(dir.problem);
      dirCategory = dir.category;
    }
  }

  // Selected URL.
  if (values.allowedUrl !== undefined) {
    const allowed = validateAllowedUrl(values.allowedUrl);
    if (allowed.href === null) state.problems.push(allowed.problem);
    else state.allowedHref = allowed.href;
  }

  const malformed = state.problems.some((problem) => MALFORMED_LABELS.has(problem));
  if (malformed) state.configError = DENY.malformedEnv;
  else if (dirCategory !== null) state.configError = dirCategory;
  else if (state.attemptDir !== null && state.eventsDir === null) state.configError = DENY.filesystemError;
  return state;
}

// ---------------------------------------------------------------------------
// Evidence (best effort on `loaded`, mandatory before an admission)
// ---------------------------------------------------------------------------

/**
 * Write one event file. Returns whether the evidence exists; the caller decides
 * what a failure means. An event is written once with `wx` and never rewritten.
 */
function writeEvent(state, kind, category, problems = null) {
  if (state.eventsDir === null) return false;
  const name = `${kind}-${process.pid}-${state.eventCounter}-${randomUUID().slice(0, 8)}.json`;
  state.eventCounter += 1;
  const record = {
    schema: EVENT_SCHEMA,
    kind,
    runtime: state.runtime,
    category,
    at: new Date().toISOString(),
  };
  if (problems !== null) record.problems = problems;
  try {
    writeFileSync(join(state.eventsDir, name), JSON.stringify(record), { flag: 'wx', mode: 0o600 });
    return true;
  } catch {
    return false;
  }
}

/**
 * Best-effort terminal marker. Its presence is what disqualifies the attempt,
 * so it is written once with `wx` (an existing marker already says exactly this)
 * and it never carries anything derived from a request. Returns false when no
 * marker exists and none could be written — the caller must then stop the child.
 */
function writeMarker(state, kind, reason) {
  if (state.markerWritten) return true;
  if (state.attemptDir === null) return false;
  const record = {
    schema: MARKER_SCHEMA,
    reason,
    kind: kind ?? null,
    pid: process.pid,
    at: new Date().toISOString(),
  };
  try {
    writeFileSync(join(state.attemptDir, MARKER_NAME), JSON.stringify(record), { flag: 'wx', mode: 0o600 });
    state.markerWritten = true;
    return true;
  } catch (error) {
    if (error !== null && typeof error === 'object' && error.code === 'EEXIST') {
      state.markerWritten = true;
      return true;
    }
    return false;
  }
}

/**
 * Stop the child when the guard can neither record the attempt nor hold the
 * ceiling. Nothing here may be catchable: `process.exit` cannot be intercepted
 * by the runtime, and the signal fallback covers the case where it returns.
 */
function terminateUnrecordable() {
  try {
    process.exit(UNRECORDABLE_EXIT_CODE);
  } catch {
    // fall through to the unconditional signal
  }
  try {
    process.kill(process.pid, 'SIGKILL');
  } catch {
    // nothing further is available; the exit above is the last resort
  }
}

/**
 * Terminal tainted state: this attempt can never be qualified and no further
 * request may be dispatched. The marker is the durable signal the consumer
 * checks; when even the marker cannot be written the child is terminated, so a
 * rejection the runtime catches can never leave a run looking green.
 */
function taint(state, kind, reason) {
  if (state.tainted) return;
  state.tainted = true;
  state.taintReason = reason;
  if (!writeMarker(state, kind, reason)) terminateUnrecordable();
}

/**
 * Persist one event, treating a real write failure as terminal for the attempt.
 * A missing evidence channel is not a write failure: it only happens when the
 * configuration was already refused (no admission is possible then, because an
 * admission requires this very write to succeed).
 */
function persistEvent(state, kind, category, problems = null) {
  const written = writeEvent(state, kind, category, problems);
  if (!written && state.eventsDir !== null) taint(state, kind, MARKER_REASONS.eventWriteFailed);
  return written;
}

/**
 * Take the single slot of this attempt. `wx` is an exclusive create, atomic
 * across processes; an existing slot is the second attempt and is denied
 * without any further inspection. The slot is never created, deleted or
 * recreated anywhere else in this file.
 */
function spend(state) {
  const spentPath = join(state.attemptDir, SPENT_NAME);
  const payload = JSON.stringify({
    schema: SPENT_SCHEMA,
    pid: process.pid,
    at: new Date().toISOString(),
    category: ADMITTED_CATEGORY,
  });
  try {
    writeFileSync(spentPath, payload, { flag: 'wx', mode: 0o600 });
    return null;
  } catch (error) {
    if (error !== null && typeof error === 'object' && error.code === 'EEXIST') {
      return DENY.alreadySpent;
    }
    return DENY.filesystemError;
  }
}

// ---------------------------------------------------------------------------
// Request classification (decides before any I/O)
// ---------------------------------------------------------------------------

/**
 * Classify one `fetch(input, init)` call against the pinned URL and method.
 * Returns the denial category, or null when the call may be admitted. Nothing
 * about the request is otherwise read, copied or recorded.
 */
function classifyRequest(state, input, init) {
  if (init !== undefined && init !== null && typeof init !== 'object') {
    return DENY.unclassifiableRequest;
  }

  let urlText = null;
  let method = null;
  if (typeof input === 'string') {
    urlText = input;
    method = init === undefined || init === null ? 'GET' : (init.method ?? 'GET');
  } else if (input instanceof URL) {
    urlText = input.href;
    method = init === undefined || init === null ? 'GET' : (init.method ?? 'GET');
  } else if (typeof Request === 'function' && input instanceof Request) {
    urlText = input.url;
    method = init === undefined || init === null ? input.method : (init.method ?? input.method);
  } else if (input !== null && typeof input === 'object' && typeof input.url === 'string') {
    urlText = input.url;
    method = init === undefined || init === null ? (input.method ?? 'GET') : (init.method ?? input.method);
  } else {
    return DENY.unclassifiableRequest;
  }

  let url;
  try {
    url = new URL(urlText);
  } catch {
    return DENY.unclassifiableRequest;
  }
  if (url.href !== state.allowedHref) return DENY.unexpectedUrl;
  if (typeof method !== 'string' || method.toUpperCase() !== 'POST') return DENY.unexpectedMethod;
  return null;
}

/** The forwarded init: the caller's objects, with `redirect` forced to `error`. */
function forwardedInit(init) {
  if (init === undefined || init === null) return { redirect: 'error' };
  return { ...init, redirect: 'error' };
}

// ---------------------------------------------------------------------------
// Installation
// ---------------------------------------------------------------------------

/**
 * Install the ceiling. `fetch` becomes a non-configurable accessor whose getter
 * always answers the guarded function: an assignment from a later polyfill is
 * ignored (it cannot silently drop the ceiling and it cannot crash dsh either),
 * while `delete`/`defineProperty` fail closed. An absent `globalThis.fetch` is
 * not a bypass — the accessor still answers, and every call is denied as an
 * unsupported transport.
 */
function installGuard(state) {
  const captured = typeof globalThis.fetch === 'function' ? globalThis.fetch : null;
  state.transportOk = captured !== null;
  if (!state.transportOk) state.problems.push(DENY.unsupportedTransport);

  const guarded = async (input, init) => {
    const deny = (category) => {
      persistEvent(state, 'denied', category);
      return new RequestGuardDenied(category);
    };
    if (!state.transportOk) throw deny(DENY.unsupportedTransport);
    // Once evidence is broken nothing may be dispatched again: the attempt is
    // already disqualified, and an admission could no longer be recorded.
    if (state.tainted) throw deny(DENY.evidenceWriteFailed);
    if (state.configError === DENY.malformedEnv) throw deny(DENY.malformedEnv);
    if (state.runtime !== 'dsh') throw deny(state.foreignCategory);
    if (state.configError !== null) throw deny(state.configError);
    const requestDenial = classifyRequest(state, input, init);
    if (requestDenial !== null) throw deny(requestDenial);
    const spendFailure = spend(state);
    if (spendFailure !== null) throw deny(spendFailure);
    // No dispatch without a persisted admission record: a run whose evidence is
    // incomplete must never be readable as a qualified one.
    if (!persistEvent(state, 'admitted', ADMITTED_CATEGORY)) throw deny(DENY.evidenceWriteFailed);
    return captured(input, forwardedInit(init));
  };

  try {
    Object.defineProperty(globalThis, 'fetch', {
      configurable: false,
      enumerable: true,
      get: () => guarded,
      set: () => {},
    });
  } catch {
    // Something already installed `fetch` as a non-configurable property, so
    // the ceiling cannot be locked. An installable-but-replaceable wrapper would
    // be a silent bypass: a later assignment would drop the guard entirely, and
    // a run could then be authorized by evidence produced before the drop. The
    // only fail-closed answer is to record the integrity failure and stop the
    // child — this is a preload/configuration failure, not a request decision.
    state.problems.push(MARKER_REASONS.fetchNotReplaceable);
    taint(state, null, MARKER_REASONS.fetchNotReplaceable);
    terminateUnrecordable();
  }
}

const guardState = initialize();
installGuard(guardState);
// The loaded handshake is part of the evidence: if it cannot be persisted the
// attempt is tainted (and the child terminated when the marker cannot be
// written either), so a missing handshake can never be mistaken for a clean run.
persistEvent(guardState, 'loaded', LOADED_CATEGORY, guardState.problems);
