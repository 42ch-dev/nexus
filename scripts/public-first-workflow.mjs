#!/usr/bin/env node
/**
 * Public first-workflow driver (P3-T1).
 *
 * Contract: `.mstar/iterations/v1.195/specs/current-host-contracts.md` §6.1.
 *
 * This is an executable developer example / proof driver, not a CLI product and
 * not an HTTP authority. It orchestrates only existing public CLI and HTTP
 * operations against a clean, child-isolated home, and prints a redacted
 * receipt:
 *
 *   node scripts/public-first-workflow.mjs --mode deterministic
 *
 * What "isolated" means here (S3-1):
 *   * one temporary root with distinct `home/`, `dsh-home/`, `workspace/` and
 *     `evidence/` directories;
 *   * every child (CLI, service, and dsh through the service) gets that root as
 *     `HOME`/`DSH_HOME`; the operator's real homes are never read or written;
 *   * credential-shaped inherited variables are removed from the child
 *     environment by NAME only — their values are never read, copied, logged
 *     or persisted;
 *   * the model protocol endpoint is a loopback-only server owned by this
 *     driver, addressed through `DEEPSEEK_BASE_URL` with a fixed non-secret
 *     placeholder key and `DSH_TELEMETRY_DISABLED=1`, so the deterministic run
 *     performs no egress;
 *   * the product database is never seeded — the Creator and workspace are
 *     created through the public CLI, and the preset through the public preset
 *     HTTP surface.
 *
 * Hard rules this driver obeys:
 *   * no build, install, codegen, schema-generation or dependency step;
 *   * fails (non-zero) instead of skipping when a prerequisite is missing — a
 *     missing dsh runtime, prepared artifact or producer is a blocker, never a
 *     pass;
 *   * the checked-in fixture is the single source of truth for the opened
 *     scope, the committed path and the committed bytes; nothing else is
 *     written into the isolated workspace;
 *   * the durable run identity is resolved with a bounded admission poll that
 *     keeps a still-pending asynchronous admission (§3.3) distinct from a
 *     terminal refusal — a pending admission is never reported as a missing
 *     producer;
 *   * W5/W6 (steer) are issued only at a durable pre-manual execution boundary
 *     read from the public execution projection (`recovery_class`,
 *     `allowed_actions`, `wait`); a plain resume is refused by the A4 fence at
 *     a human wait, so the driver never issues it there and never claims a
 *     Steer it could not place;
 *   * the receipt must carry the real **commit** revision of the declared
 *     workspace commit, read from a routed same-run frame that carries the
 *     canonical workspace-commit response shape (`rev_<id>`). The durable
 *     run-state revision is recorded beside it as an explicitly labeled
 *     informational fact and never substitutes for it: a run-state revision
 *     proves run state, not the committed workspace. A missing commit revision
 *     is a failure, never a success, and no number is ever invented or derived
 *     from the fixture bytes;
 *   * a failed or unconfirmed shutdown of any owned child — the service or the
 *     loopback model endpoint — overrides an otherwise successful journey: the
 *     receipt stays non-success and the process exits non-zero with its
 *     evidence retained;
 *   * every child it started is stopped and every path it created is removed
 *     unless `--keep` is given (ownership-scoped cleanup);
 *   * the receipt carries ids, statuses, hashes and ports — never environment
 *     dumps, request/response bodies or authorization headers;
 *   * it never executes a naked Host prompt turn as workflow proof.
 *
 * Scope split (P3-T1 vs P3-T2/T3): this driver implements the full callable
 * deterministic path — clean setup, preset authoring, admission, inspect,
 * stream, steer, cancel, restart and the declared workspace effect. Adversarial
 * protocol cases, sealed hostile-tool denial and restart/gap assertion
 * expansion are P3-T2; the separately authorized one-request live action and
 * its fetch guard are P3-T3.
 */

import { createHash } from 'node:crypto';
import { spawn, spawnSync } from 'node:child_process';
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { createServer, request as httpRequest } from 'node:http';
import { tmpdir } from 'node:os';
import { dirname, isAbsolute, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPT_PATH = fileURLToPath(import.meta.url);
const REPO_ROOT = resolve(dirname(SCRIPT_PATH), '..');
const FIXTURE_PATH = join(REPO_ROOT, 'examples', 'first-workflow', 'preset.yaml');
const SERVICE_ENTRY = join(REPO_ROOT, 'apps', 'nexus-service', 'dist', 'main.js');

/** The single ready line a service process prints on stdout (§7). */
const READY_PREFIX = 'NEXUS_SERVICE_READY ';
/** Node floor from the plan preconditions. */
const MIN_NODE = [22, 22];
/** Fixed non-secret placeholder key; the loopback endpoint ignores it (§6.1). */
const DUMMY_MODEL_KEY = 'dsh-test-nonsecret-loopback-key';
/** Deterministic one-turn model output for the sealed prompt. */
const MODEL_FINAL_TEXT = 'READY';
/** Provider id bound explicitly to the preset's single prompt role (§6.1). */
const DSH_PROVIDER_ID = 'dsh-native';
/** The preset's only prompt role: the `acp.prompt` capability carries `agent_ref: None`. */
const PROMPT_ROLE = 'default';
const CREATOR_DISPLAY_NAME = 'Nexus first workflow';
const WORKSPACE_DISPLAY_NAME = 'First workflow';

const SERVICE_READY_TIMEOUT_MS = 120_000;
const SERVICE_STOP_TIMEOUT_MS = 30_000;
/** Bounded confirmation window for the owned loopback model endpoint shutdown. */
const MODEL_ENDPOINT_CLOSE_TIMEOUT_MS = 5_000;
const CLI_TIMEOUT_MS = 120_000;
const HTTP_TIMEOUT_MS = 30_000;
const EVENT_STREAM_TIMEOUT_MS = 20_000;
const MAX_EVENT_FRAMES = 256;
/** Bounded admission poll: a W1 success may legitimately still be pending (§3.3). */
const ADMISSION_POLL_TIMEOUT_MS = 30_000;
const ADMISSION_POLL_START_INTERVAL_MS = 150;
const ADMISSION_POLL_MAX_INTERVAL_MS = 1_000;
/** Bounded synchronization of W5/W6 against the durable execution projection. */
const STEER_BOUNDARY_TIMEOUT_MS = 15_000;
const STEER_BOUNDARY_START_INTERVAL_MS = 100;
const STEER_BOUNDARY_MAX_INTERVAL_MS = 500;
/** Bounded revision follow-up reads (an O2 reconnect, never a new run). */
const EFFECT_REVISION_MAX_TAIL_READS = 5;
const EVENT_TAIL_TIMEOUT_MS = 2_000;
/** The Idea W5 appends before W6 resumes (S0-3 append-before-resume). */
const STEER_IDEA = 'public first-workflow steer';

/**
 * `creator_schedules.status` terminal values (`crates/nexus-orchestration/src/
 * schedule`). A terminal row without a claimed run can never produce one, so it
 * is a refusal; anything else without a run id is still pending.
 */
const TERMINAL_SCHEDULE_STATUSES = new Set(['cancelled', 'completed', 'failed']);

/**
 * Durable recovery classes at which `POST …/signal {signal:'resume'}` is legal.
 * The engine fences `resume` against terminal states, durable human waits and
 * in-flight/step-in-flight markers (`crates/nexus-orchestration/src/engine.rs`),
 * so only a fully committed step boundary with no wait token qualifies — the
 * pre-manual execution boundary W5/W6 must be placed at. Anything else is a
 * typed STOP, never a guess and never a bypassed human wait.
 */
const STEERABLE_RECOVERY_CLASSES = new Set(['safe_boundary']);
/** Legal-action marker of that boundary (`allowed_actions`, A2/A7 vocabulary). */
const STEERABLE_ALLOWED_ACTION = 'continue';
/** Committed revision identifier shape (`crates/nexus-core/src/execution/session_commit.rs`). */
const COMMIT_REVISION_PATTERN = /^rev_[A-Za-z0-9-]+$/;

/**
 * Credential-shaped environment variable names removed from every child.
 * Removal is by NAME: the values are never read, so an inherited secret can
 * neither be inspected nor forwarded into the deterministic run.
 */
const CREDENTIAL_ENV_KEYS = [
  'ANTHROPIC_API_KEY',
  'ANTHROPIC_AUTH_TOKEN',
  'AWS_ACCESS_KEY_ID',
  'AWS_SECRET_ACCESS_KEY',
  'AWS_SESSION_TOKEN',
  'AZURE_OPENAI_API_KEY',
  'DEEPSEEK_API_KEY',
  'GEMINI_API_KEY',
  'GH_TOKEN',
  'GITHUB_TOKEN',
  'GOOGLE_API_KEY',
  'GROQ_API_KEY',
  'MISTRAL_API_KEY',
  'NEXUS_API_KEY',
  'NEXUS_TOKEN',
  'OPENAI_API_KEY',
  'XAI_API_KEY',
];
/** Segment rule that catches the same class of name without a list edit. */
const CREDENTIAL_ENV_PATTERN = /(^|_)(API_KEY|TOKEN|SECRET|PASSWORD|CREDENTIAL|CREDENTIALS)(_|$)/;
/** A foreign preload would run inside the deterministic child; never inherit it. */
const NODE_OPTIONS_KEY = 'NODE_OPTIONS';
/** Variables this driver always sets on its own children (never inherited values). */
const DRIVER_ENV_OVERRIDES = [
  'HOME',
  'DSH_HOME',
  'DEEPSEEK_BASE_URL',
  'DEEPSEEK_API_KEY',
  'DSH_TELEMETRY_DISABLED',
  'DSH_RUNTIME_BIN',
];

const USAGE = `Usage: node scripts/public-first-workflow.mjs [options]

Options:
  --mode deterministic   Run the public clean-home deterministic journey (default).
  --keep                 Keep the isolated temporary root for inspection.
  --json                 Print the machine receipt instead of the summary.
  --help                 Print this help and exit.

Cleanup: a completed run removes the temporary root it created; a blocked or
failed run retains it (the printed receipt names it) so the STOP keeps evidence.

Exit codes:
  0  the journey completed, its declared facts were observed and every owned
     child was confirmed stopped
  1  unexpected internal failure, a runtime/recovery failure, or an
     unconfirmed/failed owned-service cleanup
  2  blocked: a prerequisite, runtime or producer required by the journey is missing
  64 usage error

Preconditions (never installed or built by this driver): prepared native
artifact / contracts / service dist, a prepared nexus42 binary, and a real
supported dsh runtime on PATH or in DSH_RUNTIME_BIN.

The driver never builds, installs, seeds the product database, reads the
operator's homes/credentials, or performs non-loopback network traffic.`;

class DriverFailure extends Error {
  /**
   * @param {'blocked'|'failed'} outcome
   * @param {string} category stable machine category
   * @param {string} detail human-readable, secret-free detail
   */
  constructor(outcome, category, detail) {
    super(detail);
    this.name = 'DriverFailure';
    this.outcome = outcome;
    this.category = category;
  }
}

const blocked = (category, detail) => new DriverFailure('blocked', category, detail);
const failed = (category, detail) => new DriverFailure('failed', category, detail);

function parseArgs(argv) {
  const options = { mode: 'deterministic', keep: false, json: false, help: false };
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    switch (token) {
      case '--mode': {
        const value = argv[++index];
        if (value !== 'deterministic') {
          throw new DriverFailure('failed', 'usage', `unsupported --mode ${JSON.stringify(value)}`);
        }
        options.mode = value;
        break;
      }
      case '--keep':
        options.keep = true;
        break;
      case '--json':
        options.json = true;
        break;
      case '--help':
      case '-h':
        options.help = true;
        break;
      default:
        throw new DriverFailure('failed', 'usage', `unknown argument: ${token}`);
    }
  }
  return options;
}

// ---------------------------------------------------------------------------
// Fixture extraction (no YAML dependency: the fixture is our own checked-in file
// with a stable shape, and every extraction asserts a single unambiguous match)
// ---------------------------------------------------------------------------

/** Strip one layer of matching YAML quotes and any trailing inline comment. */
function yamlScalar(raw) {
  let value = raw.trim();
  const hash = value.indexOf(' #');
  if (hash >= 0 && !value.startsWith('"') && !value.startsWith("'")) {
    value = value.slice(0, hash).trim();
  }
  if (value.length >= 2) {
    const first = value[0];
    const last = value[value.length - 1];
    if ((first === '"' && last === '"') || (first === "'" && last === "'")) {
      value = value.slice(1, -1);
    }
  }
  return value;
}

/** Indentation width of a line (the fixture uses spaces). */
function indentOf(line) {
  return line.length - line.trimStart().length;
}

/**
 * Find the argument block of one capability enter action and return the value
 * of `key` inside it. The block runs from the enter-action list item that owns
 * `name: <capability>` up to the next list item at the same or a shallower
 * indent, so sibling keys (`kind`, `name`, `args`) stay inside the block while
 * the next enter action does not.
 */
function extractCapabilityArg(lines, capability, key) {
  const nameIndex = lines.findIndex((line) => line.trim() === `name: ${capability}`);
  if (nameIndex < 0) {
    throw failed('fixture_contract', `fixture does not declare capability '${capability}'`);
  }
  let start = nameIndex;
  while (start > 0 && !/^\s*-\s+\S/.test(lines[start])) start -= 1;
  const itemIndent = indentOf(lines[start]);
  let end = lines.length;
  for (let index = start + 1; index < lines.length; index += 1) {
    const line = lines[index];
    if (line.trim() === '') continue;
    const indent = indentOf(line);
    if (indent < itemIndent || (indent === itemIndent && /^\s*-\s/.test(line))) {
      end = index;
      break;
    }
  }
  const matches = [];
  for (let index = start + 1; index < end; index += 1) {
    const match = /^\s*(?:-\s+)?([A-Za-z0-9_]+):\s*(.+)$/.exec(lines[index]);
    if (match && match[1] === key) matches.push(yamlScalar(match[2]));
  }
  if (matches.length !== 1) {
    throw failed(
      'fixture_contract',
      `fixture must declare exactly one '${key}' under '${capability}' (found ${matches.length})`,
    );
  }
  return matches[0];
}

/** Preset id from the `preset:` header block. */
function extractPresetId(lines) {
  const presetIndex = lines.findIndex((line) => line.trim() === 'preset:');
  if (presetIndex < 0) throw failed('fixture_contract', 'fixture has no preset: header');
  for (let index = presetIndex + 1; index < lines.length; index += 1) {
    const line = lines[index];
    if (line.trim() === '') continue;
    if (indentOf(line) <= indentOf(lines[presetIndex])) break;
    const match = /^\s*id:\s*(.+)$/.exec(line);
    if (match) return yamlScalar(match[1]);
  }
  throw failed('fixture_contract', 'fixture preset header has no id');
}

/**
 * Read the checked-in fixture and extract every fact this driver depends on.
 * The fixture stays authoritative; the driver never hard-codes the scope, the
 * committed path or the committed bytes.
 */
function readFixture() {
  if (!existsSync(FIXTURE_PATH)) {
    throw failed('fixture_missing', `fixture not found at ${FIXTURE_PATH}`);
  }
  const yaml = readFileSync(FIXTURE_PATH, 'utf8');
  const lines = yaml.split('\n');
  const presetId = extractPresetId(lines);
  const promptToolPolicy = extractCapabilityArg(lines, 'acp.prompt', 'tool_policy');
  const scopePath = extractCapabilityArg(lines, 'workspace.open', 'path');
  const changePath = extractCapabilityArg(lines, 'workspace.commit', 'path');
  const changeOp = extractCapabilityArg(lines, 'workspace.commit', 'op');
  const contentBase64 = extractCapabilityArg(lines, 'workspace.commit', 'contentBase64');

  if (promptToolPolicy !== 'deny_all') {
    throw failed(
      'fixture_contract',
      `the fixture prompt must use the sealed deny-all scope, got ${JSON.stringify(promptToolPolicy)}`,
    );
  }
  if (!/^[a-z][a-z0-9._-]*$/.test(presetId)) {
    throw failed('fixture_contract', `preset id ${JSON.stringify(presetId)} is not a valid preset id`);
  }
  for (const [label, value] of [
    ['workspace.open path', scopePath],
    ['workspace.commit path', changePath],
  ]) {
    if (value.length === 0 || isAbsolute(value) || value.includes('..') || value.includes('\\')) {
      throw failed('fixture_contract', `${label} ${JSON.stringify(value)} is not a safe relative path`);
    }
  }
  if (changeOp !== 'create') {
    throw failed('fixture_contract', `fixture commit op must be 'create', got ${JSON.stringify(changeOp)}`);
  }
  const declaredBytes = Buffer.from(contentBase64, 'base64');
  if (declaredBytes.length === 0 || declaredBytes.toString('base64') !== contentBase64) {
    throw failed('fixture_contract', 'fixture contentBase64 is not canonical base64');
  }
  return {
    yaml,
    presetId,
    scopePath,
    changePath,
    promptToolPolicy,
    declaredBytes,
    declaredSha256: sha256(declaredBytes),
  };
}

// ---------------------------------------------------------------------------
// Small utilities
// ---------------------------------------------------------------------------

function sha256(bytes) {
  return createHash('sha256').update(bytes).digest('hex');
}

function sleep(ms) {
  return new Promise((resolvePromise) => setTimeout(resolvePromise, ms));
}

function basenameOf(path) {
  return path.split('/').pop() ?? path;
}

function isExecutableFile(path) {
  try {
    const stat = statSync(path);
    return stat.isFile() && (stat.mode & 0o111) !== 0;
  } catch {
    return false;
  }
}

/** Resolve an executable from an explicit absolute path or from PATH (never a shell). */
function resolveExecutable(explicit, name) {
  if (explicit) {
    if (!isAbsolute(explicit) || !isExecutableFile(explicit)) {
      throw blocked('missing_prerequisite', `${name} override ${JSON.stringify(explicit)} is not an executable file`);
    }
    return explicit;
  }
  for (const entry of (process.env.PATH ?? '').split(':').filter(Boolean)) {
    const candidate = join(entry, name);
    if (isExecutableFile(candidate)) return candidate;
  }
  return null;
}

/** Build the child environment: isolated homes, loopback model, no inherited credentials. */
function buildChildEnv({ home, dshHome, modelPort, dshRuntimeBin }) {
  const env = {};
  // Key-first filtering (§6.1): the credential-shaped / preload decision is
  // made on the NAME before the value is ever dereferenced, so an inherited
  // secret value is never materialized, copied, logged or persisted. Iterating
  // `Object.entries`/`Object.values` here would read every value first and only
  // then discard the credential-shaped ones — a credential inspection.
  for (const key of Object.keys(process.env)) {
    if (key === NODE_OPTIONS_KEY || isCredentialEnvKey(key)) continue;
    const value = process.env[key];
    if (value === undefined) continue;
    env[key] = value;
  }
  env.HOME = home;
  env.DSH_HOME = dshHome;
  env.DEEPSEEK_BASE_URL = `http://127.0.0.1:${modelPort}`;
  env.DEEPSEEK_API_KEY = DUMMY_MODEL_KEY;
  env.DSH_TELEMETRY_DISABLED = '1';
  if (dshRuntimeBin) env.DSH_RUNTIME_BIN = dshRuntimeBin;
  return env;
}

/** Is this a credential-shaped variable name (by name only — never its value)? */
function isCredentialEnvKey(key) {
  return CREDENTIAL_ENV_KEYS.includes(key) || CREDENTIAL_ENV_PATTERN.test(key);
}

/**
 * Non-secret summary of the child environment this driver hands out: which
 * overrides are set, how many inherited credential-shaped names were dropped,
 * and whether any inherited credential-shaped name survived. Names only; no
 * value of an inherited variable is ever read.
 */
function summarizeChildEnv(inheritedKeys, env) {
  const removed = inheritedKeys.filter((key) => isCredentialEnvKey(key) && env[key] === undefined);
  const forwarded = inheritedKeys.filter(
    (key) => isCredentialEnvKey(key) && env[key] !== undefined && !DRIVER_ENV_OVERRIDES.includes(key),
  );
  return {
    overrides: DRIVER_ENV_OVERRIDES.filter((key) => env[key] !== undefined),
    removed_credential_key_count: removed.length + (inheritedKeys.includes(NODE_OPTIONS_KEY) ? 1 : 0),
    inherited_credential_keys_forwarded: forwarded,
  };
}

/** Reserve one free loopback port (bind, read, release). */
function reserveLoopbackPort() {
  return new Promise((resolvePromise, rejectPromise) => {
    const probe = createServer();
    probe.on('error', rejectPromise);
    probe.listen(0, '127.0.0.1', () => {
      const address = probe.address();
      const port = address && typeof address === 'object' ? address.port : null;
      probe.close(() => {
        if (port === null) rejectPromise(new Error('could not allocate a loopback port'));
        else resolvePromise(port);
      });
    });
  });
}

/** Bounded JSON/text HTTP request against the owned loopback service. */
function httpJson(port, method, path, { body, headers = {}, timeoutMs = HTTP_TIMEOUT_MS } = {}) {
  return new Promise((resolvePromise, rejectPromise) => {
    const payload = body === undefined ? null : Buffer.from(JSON.stringify(body), 'utf8');
    const requestHeaders = { ...headers };
    if (payload) {
      requestHeaders['content-type'] = 'application/json';
      requestHeaders['content-length'] = String(payload.length);
    }
    const request = httpRequest(
      { host: '127.0.0.1', port, path, method, headers: requestHeaders },
      (response) => {
        const chunks = [];
        response.on('data', (chunk) => chunks.push(chunk));
        response.on('end', () => {
          const text = Buffer.concat(chunks).toString('utf8');
          let json = null;
          if (text.length > 0) {
            try {
              json = JSON.parse(text);
            } catch {
              json = null;
            }
          }
          resolvePromise({ status: response.statusCode ?? 0, json, text });
        });
      },
    );
    request.setTimeout(timeoutMs, () => {
      request.destroy(new Error(`request timeout after ${timeoutMs}ms`));
    });
    request.on('error', rejectPromise);
    if (payload) request.write(payload);
    request.end();
  });
}

/** Classify a public HTTP refusal into a driver outcome. */
function statusFailure(step, response) {
  const code = response.json?.error?.code ?? response.json?.code ?? null;
  const detail = `${step}: HTTP ${response.status}${code ? ` (${code})` : ''}`;
  // §3.4 wire taxonomy (`apps/nexus-service/src/errors.ts`):
  //   * 501 `route_not_migrated` — the dependency release that owns this
  //     operation is not integrated in this tree. An unmet prerequisite.
  //   * 503 `busy` / `closing` / `interrupted` — the mounted producer's own
  //     runtime/recovery state (admission capacity, a closing owner, an
  //     interrupted provider operation). Calling that a missing producer would
  //     relabel a runtime/recovery defect as an unmet dependency (S3-6), so it
  //     keeps its own outcome category with the safe wire code preserved.
  if (response.status === 501) return blocked('unavailable_producer', detail);
  if (response.status === 503) {
    return failed('runtime_unavailable', `${detail} — mounted runtime/recovery state, not a missing dependency producer`);
  }
  return failed('contract_violation', detail);
}

function requireOk(step, response) {
  if (response.status < 200 || response.status >= 300) throw statusFailure(step, response);
  return response.json;
}

function requireFields(step, value, fields) {
  for (const field of fields) {
    if (value === null || typeof value !== 'object' || value[field] === undefined) {
      throw failed('contract_violation', `${step}: response is missing required field '${field}'`);
    }
  }
  return value;
}

/** The durable `ScheduleSummary` inside a frozen inspect response. */
function scheduleSummary(step, response) {
  const summary = response?.schedule;
  if (summary === null || typeof summary !== 'object') {
    throw failed('contract_violation', `${step}: response has no schedule summary`);
  }
  return requireFields(step, summary, ['schedule_id', 'status', 'current_core_context_version']);
}

function tailLines(text, limit) {
  return text
    .split('\n')
    .filter((line) => line.trim().length > 0)
    .slice(-limit);
}

// ---------------------------------------------------------------------------
// Durable state classification (pure) and the two bounded public polls
// ---------------------------------------------------------------------------

/**
 * Durable execution projection from a frozen inspect response (A2/A7). The
 * projection is the only routed surface that carries `recovery_class`,
 * `allowed_actions` and the human `wait`; `null` means it is not observable.
 */
function executionProjectionOf(summary) {
  const projection = summary?.execution;
  return projection === null || typeof projection !== 'object' ? null : projection;
}

/**
 * Classify one inspect observation. A still-pending asynchronous admission
 * (§3.3: W1 success means durable descriptor, the run identity may be claimed
 * after the response) is distinct from a terminal refusal that can never
 * publish a run identity, and both are distinct from a claimed identity.
 */
function classifyAdmissionObservation(summary) {
  const runId = summary?.current_session_id;
  const status = typeof summary?.status === 'string' ? summary.status : null;
  if (typeof runId === 'string' && runId.length > 0) {
    return { state: 'claimed', run_id: runId, status };
  }
  if (status !== null && TERMINAL_SCHEDULE_STATUSES.has(status)) {
    return { state: 'refused', run_id: null, status };
  }
  return { state: 'pending', run_id: null, status };
}

/** One public W4 inspect, returning both the frozen response and its summary. */
async function readScheduleInspect(port, scheduleId, step) {
  const inspected = requireOk(
    step,
    await httpJson(port, 'GET', `/v1/daemon/orchestration/schedules/${encodeURIComponent(scheduleId)}`),
  );
  return { inspected, summary: scheduleSummary(step, inspected) };
}

/**
 * Bounded admission poll: resolve the root run identity after W1 without
 * treating a pending asynchronous admission as a missing producer, and without
 * ever fabricating an id.
 */
async function awaitRunIdentity(port, scheduleId) {
  const deadline = Date.now() + ADMISSION_POLL_TIMEOUT_MS;
  let interval = ADMISSION_POLL_START_INTERVAL_MS;
  let polls = 0;
  let last = { state: 'pending', run_id: null, status: null };
  for (;;) {
    const { summary } = await readScheduleInspect(port, scheduleId, 'GET /orchestration/schedules/{id}');
    polls += 1;
    last = classifyAdmissionObservation(summary);
    if (last.state === 'claimed') return { ...last, summary, polls };
    if (last.state === 'refused') {
      throw failed(
        'admission_refused',
        `the admitted schedule settled as ${JSON.stringify(last.status)} without ever claiming a root run identity`,
      );
    }
    if (Date.now() >= deadline) {
      throw blocked(
        'admission_pending_timeout',
        `no current_session_id after ${ADMISSION_POLL_TIMEOUT_MS}ms across ${polls} inspects ` +
          `(last schedule status ${JSON.stringify(last.status)}) — the run identity is neither claimed nor refused`,
      );
    }
    await sleep(interval);
    interval = Math.min(interval * 2, ADMISSION_POLL_MAX_INTERVAL_MS);
  }
}

/**
 * Classify a durable execution projection into a Steer placement decision.
 *
 * `steerable` is the legal pre-manual execution boundary: a fully committed
 * step with no durable human wait and no in-flight marker, and the projected
 * legal actions include the continuation. Every other state is decisive and
 * the driver must stop instead of writing W5/W6:
 *
 *   * `human_wait` — a durable A4 wait exists; a plain `resume` is fenced and
 *     issuing it would bypass the human wait (§3.3 / §6.1).
 *   * `terminal` — the run already settled.
 *   * `not_legal` — in-flight/interrupted/other class, where resume is fenced.
 *   * `unobservable` — the public surface exposes no projection, so placement
 *     cannot be established at all.
 */
function classifySteerBoundary(projection) {
  if (projection === null) return { state: 'unobservable', observed: null };
  const wait = projection.wait ?? null;
  const recoveryClass = typeof projection.recovery_class === 'string' ? projection.recovery_class : null;
  const allowed = Array.isArray(projection.allowed_actions)
    ? projection.allowed_actions.filter((action) => typeof action === 'string')
    : [];
  const observed = {
    recovery_class: recoveryClass,
    wait_id: wait !== null && typeof wait.wait_id === 'string' ? wait.wait_id : null,
    wait_kind: wait !== null && typeof wait.kind === 'string' ? wait.kind : null,
    reason_code: projection.reason_code ?? null,
    execution_version: projection.execution_version ?? null,
    state_revision: projection.state_revision ?? null,
    allowed_actions: allowed,
  };
  if (recoveryClass === 'terminal') return { state: 'terminal', observed };
  if (wait !== null || recoveryClass === 'human_wait') return { state: 'human_wait', observed };
  if (!STEERABLE_RECOVERY_CLASSES.has(recoveryClass)) return { state: 'not_legal', observed };
  if (!allowed.includes(STEERABLE_ALLOWED_ACTION)) return { state: 'not_legal', observed };
  return { state: 'steerable', observed };
}

/** Turn a non-steerable placement into the exact, typed STOP for that state. */
function steerBoundaryStop(step, boundary) {
  const observed = JSON.stringify(boundary.observed);
  switch (boundary.state) {
    case 'unobservable':
      return blocked(
        'steer_boundary_unobservable',
        `${step}: the routed inspect response carries no durable execution projection ` +
          '(recovery_class/allowed_actions/wait), so a legal pre-manual Steer boundary cannot be ' +
          'established; W5/W6 are not issued and no Steer success is claimed',
      );
    case 'human_wait':
      return failed(
        'steer_boundary_missed',
        `${step}: the run already rests in a durable human wait (${observed}); the A4 fence makes a ` +
          'plain resume illegal there, so W5/W6 are not issued rather than bypassing the wait',
      );
    case 'terminal':
      return failed('steer_run_terminal', `${step}: the run is already terminal (${observed}); W5/W6 are not issued`);
    default:
      return failed(
        'steer_boundary_not_legal',
        `${step}: the durable state is not a legal pre-manual execution boundary (${observed}); ` +
          'W5/W6 are not issued',
      );
  }
}

/**
 * Bounded synchronization of the Steer against durable state. A decisive
 * observation (a legal boundary, a human wait or a terminal run) returns
 * immediately; an in-flight/other transient class is re-read until the bound,
 * after which the last observation is returned so the caller stops with the
 * exact observed state instead of writing on a timing assumption.
 */
async function awaitSteerBoundary(port, scheduleId) {
  const deadline = Date.now() + STEER_BOUNDARY_TIMEOUT_MS;
  let interval = STEER_BOUNDARY_START_INTERVAL_MS;
  let polls = 0;
  let observation = { state: 'unobservable', observed: null };
  for (;;) {
    const { summary } = await readScheduleInspect(port, scheduleId, 'GET /orchestration/schedules/{id} (steer boundary)');
    polls += 1;
    observation = classifySteerBoundary(executionProjectionOf(summary));
    if (observation.state !== 'not_legal') return { ...observation, polls };
    if (Date.now() >= deadline) return { ...observation, polls };
    await sleep(interval);
    interval = Math.min(interval * 2, STEER_BOUNDARY_MAX_INTERVAL_MS);
  }
}

// ---------------------------------------------------------------------------
// Owned child processes
// ---------------------------------------------------------------------------

/** Every child process and path this driver created, cleaned up in `main`. */
const owned = { children: new Set(), paths: [] };

/**
 * Start the isolated service process and return its discovery record.
 * Exactly one stdout line is the ready contract; every human log is stderr.
 */
async function startService({ home, port, childEnv, evidenceDir, label }) {
  const child = spawn(
    process.execPath,
    [SERVICE_ENTRY, '--home', home, '--host', '127.0.0.1', '--port', String(port)],
    { env: childEnv, stdio: ['ignore', 'pipe', 'pipe'] },
  );
  owned.children.add(child);

  let stdoutBuffer = '';
  let stderrBuffer = '';
  const ready = new Promise((resolvePromise, rejectPromise) => {
    const timer = setTimeout(() => {
      rejectPromise(
        failed('service_not_ready', `service ${label} printed no ready line within ${SERVICE_READY_TIMEOUT_MS}ms`),
      );
    }, SERVICE_READY_TIMEOUT_MS);
    child.stdout.setEncoding('utf8');
    child.stdout.on('data', (chunk) => {
      stdoutBuffer += chunk;
      const lines = stdoutBuffer.split('\n');
      stdoutBuffer = lines.pop() ?? '';
      for (const line of lines) {
        if (!line.startsWith(READY_PREFIX)) continue;
        clearTimeout(timer);
        try {
          resolvePromise(JSON.parse(line.slice(READY_PREFIX.length)));
        } catch (error) {
          rejectPromise(failed('service_not_ready', `service ${label} ready line is not JSON: ${error.message}`));
        }
      }
    });
    child.stderr.setEncoding('utf8');
    child.stderr.on('data', (chunk) => {
      stderrBuffer += chunk;
    });
    child.once('exit', (code, signal) => {
      clearTimeout(timer);
      rejectPromise(
        blocked('service_exited', `service ${label} exited before readiness (code=${code} signal=${signal ?? 'none'})`),
      );
    });
    child.once('error', (error) => {
      clearTimeout(timer);
      rejectPromise(failed('service_spawn', `service ${label} failed to spawn: ${error.message}`));
    });
  });

  let discovery;
  try {
    discovery = await ready;
  } catch (error) {
    writeFileSync(join(evidenceDir, `service-${label}.stderr.log`), stderrBuffer, 'utf8');
    if (error instanceof DriverFailure) {
      // The bounded service log stays in this driver's own evidence directory
      // (owner-only, inside the isolated root); it is never copied into the
      // printed receipt.
      error.serviceStderrLog = join(evidenceDir, `service-${label}.stderr.log`);
    }
    throw error;
  }
  writeFileSync(join(evidenceDir, `service-${label}.stderr.log`), stderrBuffer, 'utf8');
  if (discovery === null || typeof discovery !== 'object' || typeof discovery.instance_id !== 'string') {
    throw failed('contract_violation', `service ${label} ready record is missing instance_id`);
  }
  return { child, discovery };
}

/** Port of the published HTTP endpoint of a discovery record. */
function servicePortOf(discovery) {
  const url = discovery?.endpoint?.url;
  if (typeof url !== 'string') {
    throw failed('contract_violation', 'service discovery record has no HTTP endpoint url');
  }
  const match = /:(\d+)\/?$/.exec(url);
  if (!match) throw failed('contract_violation', `cannot read a port from endpoint ${url}`);
  return Number.parseInt(match[1], 10);
}

/**
 * Stop one owned service over the public operator stop path with the exact
 * instance identity, then wait for its exit.
 */
async function stopServiceVia(running, label) {
  const { child, discovery } = running;
  const response = await httpJson(servicePortOf(discovery), 'POST', '/v1/daemon/runtime/stop', {
    body: {
      expected_instance_id: discovery.instance_id,
      expected_engine_epoch: discovery.engine_epoch ?? null,
    },
  });
  if (response.status === 409) {
    throw failed('contract_violation', `service ${label} refused its own stop identity (409 instance_conflict)`);
  }
  requireOk(`service ${label} stop`, response);

  const deadline = Date.now() + SERVICE_STOP_TIMEOUT_MS;
  while (Date.now() < deadline) {
    if (child.exitCode !== null || child.signalCode !== null) {
      owned.children.delete(child);
      return { confirmed: true, code: child.exitCode, signal: child.signalCode ?? null };
    }
    await sleep(50);
  }
  child.kill('SIGTERM');
  await sleep(500);
  if (child.exitCode === null && child.signalCode === null) child.kill('SIGKILL');
  return {
    confirmed: false,
    code: child.exitCode,
    signal: child.signalCode ?? null,
    detail: `service ${label} did not exit within the bounded public stop window`,
  };
}

/**
 * Close one owned Node HTTP server with a checked, bounded confirmation.
 *
 * Node's `server.close(callback)` reports a callback error when the server was
 * not open and otherwise waits for every open connection, so a rejected,
 * throwing or never-calling close must not leave the driver without a verdict.
 * The result is always a resolved `{confirmed, detail}` — never a rejection and
 * never a throw — so a failing shutdown is routed into the cleanup disposition
 * instead of being swallowed. Only the handle passed in is touched: no other
 * process, socket or path is ever signalled or removed.
 */
function boundedServerClose(server, timeoutMs = MODEL_ENDPOINT_CLOSE_TIMEOUT_MS) {
  return new Promise((resolvePromise) => {
    let settled = false;
    const settle = (result) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      resolvePromise(result);
    };
    const dropIdleConnections = () => {
      try {
        server.closeAllConnections?.();
      } catch {
        // best effort: the callback verdict and the bound stay authoritative
      }
    };
    const timer = setTimeout(() => {
      dropIdleConnections();
      settle({ confirmed: false, detail: `the owned server did not confirm shutdown within ${timeoutMs}ms` });
    }, timeoutMs);
    try {
      server.close((error) => {
        if (settled) return;
        if (error) settle({ confirmed: false, detail: `the owned server close callback reported: ${error.message}` });
        else settle({ confirmed: true, detail: null });
      });
    } catch (error) {
      settle({
        confirmed: false,
        detail: `the owned server close threw: ${error instanceof Error ? error.message : String(error)}`,
      });
      return;
    }
    // Release any idle/keep-alive socket the endpoint still holds, so a genuine
    // close is not parked behind a connection nobody is using.
    dropIdleConnections();
  });
}

/**
 * Cleanup is part of the success condition (§3.4 cleanup disposition; §6.1
 * confirmed shutdown; §7 "unconfirmed cleanup is a STOP with retained
 * evidence"). Every owned child contributes one cleanup record; a confirmed
 * shutdown leaves the journey outcome untouched, while any failed or
 * unconfirmed one overrides an earlier `ok` so neither the receipt nor the
 * process exit code can report success while an owned child may still be
 * alive. A journey that already failed or blocked keeps its primary blocker —
 * it exits non-zero either way — and the cleanup failure is recorded beside it.
 */
function applyCleanupDisposition(receipt, cleanups) {
  const records = Array.isArray(cleanups) ? cleanups : [cleanups];
  receipt.cleanup = records;
  const unconfirmed = records.find((entry) => entry.confirmed !== true);
  if (unconfirmed === undefined) return receipt;
  if (receipt.outcome === 'ok') {
    receipt.outcome = 'failed';
    receipt.blocker = {
      outcome: 'failed',
      category: unconfirmed.category ?? 'cleanup_unconfirmed',
      detail: unconfirmed.detail ?? 'the owned child shutdown was not confirmed',
    };
  }
  return receipt;
}

/** Process exit code for a finished receipt (usage errors exit 64 earlier). */
function exitCodeFor(outcome) {
  if (outcome === 'ok') return 0;
  return outcome === 'blocked' ? 2 : 1;
}

// ---------------------------------------------------------------------------
// Controlled loopback model protocol endpoint (§6.1: no egress)
// ---------------------------------------------------------------------------

/**
 * DeepSeek-compatible loopback endpoint. It answers one SSE completion for
 * `POST /chat/completions` and records only non-secret request structure —
 * never bodies, headers or keys.
 */
function startModelEndpoint() {
  const observations = { requests: 0, paths: [], unexpected: 0, authorization_header_present: false };
  const server = createServer((req, res) => {
    observations.requests += 1;
    if (typeof req.url === 'string') observations.paths.push(req.url.split('?')[0]);
    if (req.headers.authorization !== undefined) observations.authorization_header_present = true;
    if (req.method !== 'POST' || req.url?.split('?')[0] !== '/chat/completions') {
      observations.unexpected += 1;
      res.writeHead(404, { 'content-type': 'application/json' });
      res.end(JSON.stringify({ error: { message: 'only POST /chat/completions is served' } }));
      return;
    }
    req.resume();
    req.on('end', () => {
      const chunk = (delta, finishReason) =>
        `data: ${JSON.stringify({
          id: 'chatcmpl-public-first-workflow',
          object: 'chat.completion.chunk',
          created: 1_700_000_000,
          model: 'deepseek-v4-flash',
          choices: [{ index: 0, delta, finish_reason: finishReason }],
        })}\n\n`;
      const usage = `data: ${JSON.stringify({
        id: 'chatcmpl-public-first-workflow',
        object: 'chat.completion.chunk',
        created: 1_700_000_000,
        model: 'deepseek-v4-flash',
        choices: [],
        usage: { prompt_tokens: 10, completion_tokens: 1, total_tokens: 11 },
      })}\n\n`;
      const body =
        chunk({ role: 'assistant', content: MODEL_FINAL_TEXT }, null) +
        chunk({}, 'stop') +
        usage +
        'data: [DONE]\n\n';
      res.writeHead(200, {
        'content-type': 'text/event-stream',
        'cache-control': 'no-cache',
        connection: 'close',
        'content-length': String(Buffer.byteLength(body)),
      });
      res.end(body);
    });
  });
  return new Promise((resolvePromise, rejectPromise) => {
    server.on('error', rejectPromise);
    server.listen(0, '127.0.0.1', () => {
      const address = server.address();
      const port = address && typeof address === 'object' ? address.port : null;
      if (port === null) {
        rejectPromise(failed('model_endpoint', 'could not bind the loopback model endpoint'));
        return;
      }
      resolvePromise({
        port,
        observations,
        close: () => boundedServerClose(server),
      });
    });
  });
}

// ---------------------------------------------------------------------------
// Public CLI
// ---------------------------------------------------------------------------

function runCli(binary, args, childEnv, label) {
  const result = spawnSync(binary, args, {
    env: childEnv,
    encoding: 'utf8',
    timeout: CLI_TIMEOUT_MS,
    maxBuffer: 8 * 1024 * 1024,
  });
  if (result.error) {
    throw blocked('cli_unavailable', `${label}: ${result.error.message}`);
  }
  if (result.status !== 0) {
    const detail = tailLines(result.stderr ?? '', 6).join(' | ');
    throw failed('cli_failed', `${label}: exit ${result.status}${detail ? ` — ${detail}` : ''}`);
  }
  return { stdout: result.stdout ?? '', stderr: result.stderr ?? '' };
}

/** Active non-secret Creator identity from the public CLI surface. */
function readActiveCreator(binary, childEnv) {
  const { stdout } = runCli(binary, ['creator', 'list', '--json'], childEnv, 'nexus42 creator list --json');
  let rows;
  try {
    rows = JSON.parse(stdout);
  } catch (error) {
    throw failed('contract_violation', `creator list --json is not JSON: ${error.message}`);
  }
  if (!Array.isArray(rows)) throw failed('contract_violation', 'creator list --json is not an array');
  const active = rows.filter((row) => row && row.active === true);
  if (active.length !== 1) {
    throw failed('contract_violation', `expected exactly one active Creator, found ${active.length}`);
  }
  if (typeof active[0].creator_id !== 'string' || active[0].creator_id.length === 0) {
    throw failed('contract_violation', 'active Creator row has no creator_id');
  }
  return {
    creator_id: active[0].creator_id,
    display_name: active[0].display_name ?? null,
    origin: active[0].origin ?? null,
  };
}

// ---------------------------------------------------------------------------
// Workflow event stream (O1/O2)
// ---------------------------------------------------------------------------

/**
 * Read a bounded slice of a same-run SSE stream. A non-2xx answer is surfaced
 * with its status so the caller can classify a missing producer.
 */
function readEventStream(port, runId, { lastEventId, maxFrames = MAX_EVENT_FRAMES, timeoutMs = EVENT_STREAM_TIMEOUT_MS } = {}) {
  return new Promise((resolvePromise, rejectPromise) => {
    const headers = { accept: 'text/event-stream' };
    if (lastEventId) headers['last-event-id'] = lastEventId;
    const path = `/v1/daemon/orchestration/sessions/${encodeURIComponent(runId)}/events`;
    const request = httpRequest({ host: '127.0.0.1', port, path, method: 'GET', headers }, (response) => {
      if (response.statusCode !== 200) {
        const chunks = [];
        response.on('data', (chunk) => chunks.push(chunk));
        response.on('end', () => {
          const text = Buffer.concat(chunks).toString('utf8');
          let json = null;
          try {
            json = text.length > 0 ? JSON.parse(text) : null;
          } catch {
            json = null;
          }
          resolvePromise({ status: response.statusCode ?? 0, json, frames: [], closed: true });
        });
        return;
      }
      const frames = [];
      let buffer = '';
      let closed = false;
      let settled = false;
      const finish = () => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        request.destroy();
        resolvePromise({ status: 200, json: null, frames, closed });
      };
      const timer = setTimeout(finish, timeoutMs);
      response.setEncoding('utf8');
      response.on('data', (chunk) => {
        buffer += chunk;
        let separator = buffer.indexOf('\n\n');
        while (separator >= 0) {
          const rawFrame = buffer.slice(0, separator);
          buffer = buffer.slice(separator + 2);
          const frame = { id: null, event: null, data: null };
          for (const line of rawFrame.split('\n')) {
            if (line.startsWith('id:')) frame.id = line.slice(3).trim();
            else if (line.startsWith('event:')) frame.event = line.slice(6).trim();
            else if (line.startsWith('data:')) frame.data = line.slice(5).trim();
          }
          if (frame.id !== null || frame.event !== null || frame.data !== null) frames.push(frame);
          if (frames.length >= maxFrames) {
            closed = true;
            finish();
            return;
          }
          separator = buffer.indexOf('\n\n');
        }
      });
      response.on('end', () => {
        closed = true;
        finish();
      });
      response.on('error', () => {
        closed = true;
        finish();
      });
    });
    request.setTimeout(timeoutMs, () => request.destroy());
    request.on('error', rejectPromise);
    request.end();
  });
}

/**
 * The workspace-commit revision identifier when a routed event carries it
 * (`{"revision":"rev_<uuid>"}`, the `CoreWorkspaceCommitResponse` shape); never
 * invented and never derived from the fixture bytes.
 */
function findCommitRevision(frames) {
  for (const frame of frames) {
    const match = /"revision"\s*:\s*"([^"]+)"/.exec(frame.data ?? '');
    if (match && COMMIT_REVISION_PATTERN.test(match[1])) return match[1];
  }
  return null;
}

/** Durable run-state revision from routed `run_state` frames (real projection, never computed). */
function findRunStateRevision(frames) {
  let revision = null;
  for (const frame of frames) {
    if (frame.event !== 'run_state' || typeof frame.data !== 'string') continue;
    let payload = null;
    try {
      payload = JSON.parse(frame.data);
    } catch {
      continue;
    }
    const value = payload?.state_revision;
    if (Number.isInteger(value) && value >= 0) revision = value;
  }
  return revision;
}

/**
 * Resolve the revision the receipt must carry (Task 1 requires the committed
 * file *and* its commit revision; §6.1 requires the commit revision in the
 * receipt).
 *
 * The only accepted source is a routed same-run frame carrying the canonical
 * workspace-commit response shape (`{"revision":"rev_<id>","committed":…}`,
 * `schemas/core/core-workspace-commit-response.schema.json`) — the identifier
 * the durable workspace-commit authority returned for this run. Nothing is
 * computed from the fixture bytes and no identifier is invented.
 *
 * The durable run-state revision (`RunStateWire.state_revision`, from routed
 * `run_state` frames or the inspect execution projection) proves run state, not
 * the committed workspace, so it is returned as an explicitly labeled
 * informational field and NEVER substituted for the commit revision. A missing
 * commit revision is refused here — it is never represented as an acceptable
 * result — so a caller cannot record the effect as a success.
 *
 * @throws {DriverFailure} `failed`/`missing_commit_revision` when no routed
 *   frame exposed the workspace commit revision.
 */
function resolveEffectRevision({ commitRevision, frames, projectionStateRevision }) {
  const commit =
    typeof commitRevision === 'string' && COMMIT_REVISION_PATTERN.test(commitRevision) ? commitRevision : null;
  const streamed = findRunStateRevision(frames);
  const projected =
    Number.isInteger(projectionStateRevision) && projectionStateRevision >= 0 ? projectionStateRevision : null;
  const durableStateRevision = streamed ?? projected;
  if (commit === null) {
    throw failed(
      'missing_commit_revision',
      'the declared workspace effect landed, but no routed same-run frame exposed the workspace commit revision ' +
        '(no rev_<id> in a CoreWorkspaceCommitResponse-shaped payload; ' +
        `durable run-state revision ${durableStateRevision === null ? 'none observed' : durableStateRevision} ` +
        'proves run state, not the committed workspace and is never substituted); §6.1 and the P3-T1 card require ' +
        'the committed file and its commit revision, so the effect is not a success',
    );
  }
  return {
    commit_revision: commit,
    durable_state_revision: durableStateRevision,
    revision_source: 'run-event-stream-commit-revision',
  };
}

// ---------------------------------------------------------------------------
// Journey
// ---------------------------------------------------------------------------

async function runDeterministic(options) {
  const receipt = {
    schema: 'public-first-workflow-receipt/1',
    mode: options.mode,
    outcome: 'ok',
    blocker: null,
    started_at: new Date().toISOString(),
    finished_at: null,
    isolated_root: null,
    ports: null,
    steps: [],
    facts: {},
  };
  const steps = receipt.steps;
  const facts = receipt.facts;
  const record = (step, status, extra = {}) => steps.push({ step, status, ...extra });

  let model = null;
  let running = null;
  try {
    // 1. Prerequisites — fail, never skip. The fixture is a static input, so
    // it is validated before any temporary directory or child process exists.
    const fixture = readFixture();
    record('fixture', 'ok', {
      preset_id: fixture.presetId,
      prompt_tool_policy: fixture.promptToolPolicy,
      prompt_role: PROMPT_ROLE,
      prompt_role_binding: { [PROMPT_ROLE]: { provider_id: DSH_PROVIDER_ID } },
      scope: fixture.scopePath,
      change_path: fixture.changePath,
      declared_bytes: fixture.declaredBytes.length,
      declared_sha256: fixture.declaredSha256,
    });

    const [nodeMajor, nodeMinor] = process.versions.node.split('.').map(Number);
    if (nodeMajor < MIN_NODE[0] || (nodeMajor === MIN_NODE[0] && nodeMinor < MIN_NODE[1])) {
      throw blocked('missing_prerequisite', `Node >= ${MIN_NODE.join('.')} required, found ${process.versions.node}`);
    }
    if (!existsSync(SERVICE_ENTRY)) {
      throw blocked(
        'missing_prerequisite',
        `prepared service artifact missing at ${SERVICE_ENTRY} — refresh prepared artifacts first (pnpm dev:backend:refresh); this driver never builds`,
      );
    }
    const cliBinary = resolveExecutable(process.env.NEXUS42_BIN, 'nexus42');
    if (!cliBinary) {
      throw blocked('missing_prerequisite', 'nexus42 not found on PATH (set NEXUS42_BIN to an absolute prepared binary)');
    }
    const dshBinary = resolveExecutable(process.env.DSH_RUNTIME_BIN, 'dsh');
    if (!dshBinary) {
      throw blocked('missing_prerequisite', 'no real dsh runtime on PATH (set DSH_RUNTIME_BIN to the installed runtime)');
    }
    record('preflight', 'ok', { node: process.versions.node, cli: basenameOf(cliBinary), dsh: basenameOf(dshBinary) });

    // 2. Isolated root, fixture-derived scope.
    const root = mkdtempSync(join(tmpdir(), 'nexus-public-first-workflow-'));
    owned.paths.push(root);
    receipt.isolated_root = root;
    const home = join(root, 'home');
    const dshHome = join(root, 'dsh-home');
    const workspace = join(root, 'workspace');
    const evidenceDir = join(root, 'evidence');
    for (const path of [home, dshHome, workspace, evidenceDir]) mkdirSync(path, { recursive: true });

    const scopeDir = join(workspace, fixture.scopePath);
    // §6.1: the commit applier walks the opened scope with O_DIRECTORY and
    // creates nothing, so the declared scope must already exist.
    mkdirSync(scopeDir, { recursive: true });
    record('isolate', 'ok', { scope: `${basenameOf(workspace)}/${fixture.scopePath}` });

    model = await startModelEndpoint();
    const servicePort = await reserveLoopbackPort();
    receipt.ports = { service: servicePort, model: model.port };
    const childEnv = buildChildEnv({ home, dshHome, modelPort: model.port, dshRuntimeBin: dshBinary });
    facts.child_env = summarizeChildEnv(Object.keys(process.env), childEnv);
    if (facts.child_env.inherited_credential_keys_forwarded.length > 0) {
      throw failed(
        'contract_violation',
        `an inherited credential-shaped variable survived into the deterministic child: ${facts.child_env.inherited_credential_keys_forwarded.join(', ')}`,
      );
    }
    record('owned_children', 'ok', { model_port: model.port, service_port: servicePort, child_env: facts.child_env });

    // 3. First start on an empty home must be explicit and uninitialized.
    running = await startService({ home, port: servicePort, childEnv, evidenceDir, label: 'boot' });
    if (running.discovery.readiness !== 'uninitialized') {
      throw failed(
        'contract_violation',
        `first start with no profile must report readiness=uninitialized, got ${JSON.stringify(running.discovery.readiness)}`,
      );
    }
    if (
      running.discovery.creator_id !== null ||
      running.discovery.workspace_slug !== null ||
      running.discovery.engine_epoch !== null
    ) {
      throw failed('contract_violation', 'uninitialized shell carried creator/workspace/epoch identity');
    }
    facts.uninitialized_discovery = {
      readiness: running.discovery.readiness,
      instance_id: running.discovery.instance_id,
      creator_id: running.discovery.creator_id,
      workspace_slug: running.discovery.workspace_slug,
      engine_epoch: running.discovery.engine_epoch,
      protocol_version: running.discovery.protocol_version ?? null,
    };
    record('first_start', 'ok', { readiness: running.discovery.readiness });
    facts.first_stop = await stopServiceVia(running, 'boot');
    record('first_stop', 'ok', facts.first_stop);
    running = null;

    // 4. Public clean-home setup: Creator + workspace through the CLI only.
    const register = runCli(
      cliBinary,
      ['creator', 'register', '--local', '--name', CREATOR_DISPLAY_NAME],
      childEnv,
      'nexus42 creator register --local',
    );
    record('creator_register', 'ok', { stdout_lines: register.stdout.split('\n').filter(Boolean).length });
    const workspaceCreate = runCli(
      cliBinary,
      ['creator', 'workspace', 'create', fixture.presetId, '--creative-root', workspace, '--name', WORKSPACE_DISPLAY_NAME],
      childEnv,
      'nexus42 creator workspace create',
    );
    record('workspace_create', 'ok', { stdout_lines: workspaceCreate.stdout.split('\n').filter(Boolean).length });
    const creator = readActiveCreator(cliBinary, childEnv);
    facts.identity = {
      creator_id: creator.creator_id,
      display_name: creator.display_name,
      origin: creator.origin,
      workspace_slug: fixture.presetId,
      creative_root: workspace,
    };
    record('identity', 'ok', { creator_origin: creator.origin });

    // 5. Restart against the selected Creator: readiness must be real, not a shell.
    running = await startService({ home, port: servicePort, childEnv, evidenceDir, label: 'ready' });
    if (running.discovery.readiness !== 'ready') {
      throw blocked(
        'provider_not_ready',
        `service reports readiness=${JSON.stringify(running.discovery.readiness)} after public setup; the selected dsh provider is not admitted`,
      );
    }
    if (running.discovery.creator_id !== creator.creator_id || running.discovery.workspace_slug !== fixture.presetId) {
      throw failed('contract_violation', 'ready discovery record does not name the selected Creator/workspace');
    }
    if (!Number.isInteger(running.discovery.engine_epoch)) {
      throw failed('contract_violation', 'ready discovery record has no integer engine_epoch');
    }
    facts.ready_discovery = {
      readiness: running.discovery.readiness,
      instance_id: running.discovery.instance_id,
      creator_id: running.discovery.creator_id,
      workspace_slug: running.discovery.workspace_slug,
      engine_epoch: running.discovery.engine_epoch,
    };
    record('ready_start', 'ok', { readiness: running.discovery.readiness, engine_epoch: running.discovery.engine_epoch });

    const port = servicePortOf(running.discovery);

    // 6. Preset authoring over the public HTTP surface.
    const scaffold = requireOk(
      'POST /presets',
      await httpJson(port, 'POST', '/v1/daemon/presets', { body: { name: fixture.presetId } }),
    );
    requireFields('POST /presets', scaffold, ['id', 'path']);
    if (scaffold.id !== fixture.presetId) {
      throw failed('contract_violation', `scaffolded preset id ${JSON.stringify(scaffold.id)} does not match the fixture`);
    }
    const patched = requireOk(
      'PATCH /presets/{id}',
      await httpJson(port, 'PATCH', `/v1/daemon/presets/${encodeURIComponent(fixture.presetId)}`, {
        body: { yaml: fixture.yaml },
      }),
    );
    requireFields('PATCH /presets/{id}', patched, ['id', 'updated']);
    if (patched.updated !== true) throw failed('contract_violation', 'preset PATCH did not report updated=true');
    const validated = requireOk(
      'POST /presets:validate',
      await httpJson(port, 'POST', '/v1/daemon/presets:validate', { body: { path: scaffold.path } }),
    );
    requireFields('POST /presets:validate', validated, ['valid', 'errors']);
    if (validated.valid !== true) {
      throw failed('contract_violation', `preset validation refused the fixture: ${JSON.stringify(validated.errors)}`);
    }
    facts.preset = {
      id: scaffold.id,
      validated: true,
      state_count: validated.state_count ?? null,
      warnings: validated.warnings ?? [],
    };
    record('preset_authoring', 'ok', { validated: true });

    // 7. Admit one workflow with the explicit prompt-role binding.
    const admitted = requireOk(
      'POST /orchestration/schedules',
      await httpJson(port, 'POST', '/v1/daemon/orchestration/schedules', {
        body: {
          creator_id: creator.creator_id,
          preset_id: fixture.presetId,
          label: 'public first workflow',
          agent_bindings: { [PROMPT_ROLE]: { provider_id: DSH_PROVIDER_ID } },
        },
      }),
    );
    requireFields('POST /orchestration/schedules', admitted, ['schedule_id', 'status', 'core_context_version']);
    facts.schedule = {
      schedule_id: admitted.schedule_id,
      status: admitted.status,
      core_context_version: admitted.core_context_version,
    };
    record('admit', 'ok', { schedule_id: admitted.schedule_id });

    // 8. W4 inspect with a bounded admission poll. W1 success means the
    // descriptor is durable while the root run identity may still be claimed
    // asynchronously (§3.3), so a pending admission is polled instead of being
    // reported as a missing producer, and a terminal refusal/timeout are their
    // own outcomes. The frozen response shape is
    // `{schedule, depends_on, concurrency_kind}`
    // (`inspect-schedule-response.schema.json` + `schedule-summary.schema.json`).
    const scheduleId = admitted.schedule_id;
    const admission = await awaitRunIdentity(port, scheduleId);
    const runId = admission.run_id;
    facts.inspect = {
      status: admission.summary.status,
      current_core_context_version: admission.summary.current_core_context_version ?? null,
      current_session_id: runId,
      execution_policy: admission.summary.execution_policy ?? null,
      admission_polls: admission.polls,
      execution: executionProjectionOf(admission.summary),
    };
    record('inspect', 'ok', { status: facts.inspect.status, admission_polls: admission.polls });

    // 9. W5/W6 steer, synchronized on durable state (§6.1: W5/W6 are exercised
    // before the final manual wait; S0-3: the append is durable before resume
    // counts as success). Placement comes from the routed execution projection,
    // never from the preceding bounded read: at a durable human wait the A4
    // fence refuses a plain resume, so the driver stops with the exact observed
    // state instead of bypassing the wait or claiming a Steer it did not place.
    const steerBoundary = await awaitSteerBoundary(port, scheduleId);
    if (steerBoundary.state !== 'steerable') throw steerBoundaryStop('steer', steerBoundary);
    const appendResponse = requireOk(
      'PATCH /orchestration/schedules/{id}/core-context',
      await httpJson(port, 'PATCH', `/v1/daemon/orchestration/schedules/${encodeURIComponent(scheduleId)}/core-context`, {
        body: { op: 'append', body: STEER_IDEA },
      }),
    );
    requireFields('PATCH /orchestration/schedules/{id}/core-context', appendResponse, ['new_version']);
    facts.steer = {
      boundary: steerBoundary.observed,
      boundary_polls: steerBoundary.polls,
      appended_version: appendResponse.new_version,
      recheck: null,
      resumed: false,
      resume: null,
    };
    // Re-read the routed boundary before resuming: if the run moved on between
    // the durable append and the resume, a plain resume would land where it is
    // fenced. The Steer then stops with the exact observed state; the appended
    // version stays durable and is never re-appended (S0-3).
    const steerRecheck = classifySteerBoundary(
      executionProjectionOf(
        (await readScheduleInspect(port, scheduleId, 'GET /orchestration/schedules/{id} (steer recheck)')).summary,
      ),
    );
    facts.steer.recheck = steerRecheck.observed;
    if (steerRecheck.state !== 'steerable') {
      const stop = steerBoundaryStop('steer recheck', steerRecheck);
      throw new DriverFailure(
        stop.outcome,
        'steer_boundary_lost',
        `${stop.message} — the appended core-context version ${JSON.stringify(appendResponse.new_version)} remains ` +
          'durable; resume was not issued and the append is never retried',
      );
    }
    const resumeResponse = await httpJson(
      port,
      'POST',
      `/v1/daemon/orchestration/schedules/${encodeURIComponent(scheduleId)}/signal`,
      { body: { signal: 'resume' } },
    );
    facts.steer.resume = {
      status: resumeResponse.status,
      code: resumeResponse.json?.error?.code ?? resumeResponse.json?.code ?? null,
      current_wait_id: resumeResponse.json?.current_wait_id ?? null,
    };
    if (resumeResponse.status < 200 || resumeResponse.status >= 300) {
      // Preserve the exact conflict verbatim (wire status plus coded detail,
      // e.g. `workflow_wait_conflict`); the appended version remains durable and
      // there is no automatic re-append and no retry.
      const refusal = statusFailure('POST /orchestration/schedules/{id}/signal (resume)', resumeResponse);
      throw new DriverFailure(
        refusal.outcome,
        'steer_resume_refused',
        `${refusal.message} — appended core-context version ${JSON.stringify(appendResponse.new_version)} remains ` +
          'durable; no re-append and no retry',
      );
    }
    facts.steer.resumed = true;
    record('steer', 'ok', {
      boundary_recovery_class: steerBoundary.observed.recovery_class,
      boundary_polls: steerBoundary.polls,
      appended_version: appendResponse.new_version,
    });

    // 10. O1/O2: same-run stream against the inspected root session.
    const stream = await readEventStream(port, runId);
    if (stream.status !== 200) {
      throw statusFailure('GET /orchestration/sessions/{run_id}/events', {
        status: stream.status,
        json: stream.json,
        text: '',
      });
    }
    const lastEventId = stream.frames.map((frame) => frame.id).filter(Boolean).pop() ?? null;
    facts.stream = {
      frame_count: stream.frames.length,
      last_event_id: lastEventId,
      closed: stream.closed,
      control_frames: stream.frames
        .filter((frame) => ['gap', 'history_unavailable'].includes(frame.event))
        .map((frame) => frame.event),
    };
    record('stream', 'ok', { frames: stream.frames.length });

    // 11. Bounded revision follow-up reads: the commit frame may land after the
    // first bounded read, and §6.1 requires the committed file *and* revision.
    // Every follow-up is a reconnect from the last observed cursor (O2), never a
    // new run.
    let allFrames = [...stream.frames];
    let observedRevision = findCommitRevision(allFrames);
    let tailReads = 0;
    while (observedRevision === null && tailReads < EFFECT_REVISION_MAX_TAIL_READS) {
      const tail = await readEventStream(port, runId, {
        lastEventId: allFrames.map((frame) => frame.id).filter(Boolean).pop() ?? null,
        maxFrames: 64,
        timeoutMs: EVENT_TAIL_TIMEOUT_MS,
      });
      if (tail.status !== 200) {
        throw statusFailure('GET /orchestration/sessions/{run_id}/events (tail)', {
          status: tail.status,
          json: tail.json,
          text: '',
        });
      }
      tailReads += 1;
      allFrames = [...allFrames, ...tail.frames];
      observedRevision = findCommitRevision(allFrames);
    }
    facts.stream.tail_reads = tailReads;
    facts.stream.tail_frame_count = allFrames.length - stream.frames.length;
    record('stream_tail', 'ok', { reads: tailReads, frames: facts.stream.tail_frame_count });

    // 12. §6.1: the declared workspace effect, read back byte-for-byte.
    const effectPath = join(scopeDir, fixture.changePath);
    if (!existsSync(effectPath)) {
      throw failed(
        'contract_violation',
        `declared workspace effect was not committed at ${fixture.scopePath}/${fixture.changePath}`,
      );
    }
    const landed = readFileSync(effectPath);
    if (!landed.equals(fixture.declaredBytes)) {
      throw failed('contract_violation', 'committed file content does not match the declared fixture manifest content');
    }
    // The receipt must carry the **commit** revision read from a routed
    // same-run frame carrying the canonical workspace-commit response. The
    // durable run-state revision is recorded beside it as information only and
    // never substitutes: it proves run state, not the committed workspace.
    // Nothing is computed from the fixture bytes and no identifier is invented;
    // a missing commit revision refuses the effect (missing_commit_revision),
    // never a success.
    const effectProjection = executionProjectionOf(
      (await readScheduleInspect(port, scheduleId, 'GET /orchestration/schedules/{id} (effect)')).summary,
    );
    facts.effect = {
      relative_path: `${fixture.scopePath}/${fixture.changePath}`,
      bytes: landed.length,
      sha256: sha256(landed),
      declared_content_matches: true,
      commit_revision: null,
      durable_state_revision: null,
      revision_source: null,
    };
    Object.assign(
      facts.effect,
      resolveEffectRevision({
        commitRevision: observedRevision,
        frames: allFrames,
        projectionStateRevision: effectProjection?.state_revision ?? null,
      }),
    );
    record('workspace_effect', 'ok', {
      sha256: facts.effect.sha256,
      commit_revision: facts.effect.commit_revision,
      durable_state_revision: facts.effect.durable_state_revision,
      revision_source: facts.effect.revision_source,
    });

    // 13. W7 cancel: a durable `cancelled` is the only success.
    requireOk(
      'POST /orchestration/schedules/{id}/signal (cancel)',
      await httpJson(port, 'POST', `/v1/daemon/orchestration/schedules/${encodeURIComponent(scheduleId)}/signal`, {
        body: { signal: 'cancel' },
      }),
    );
    const afterCancel = scheduleSummary(
      'GET /orchestration/schedules/{id}',
      requireOk(
        'GET /orchestration/schedules/{id}',
        await httpJson(port, 'GET', `/v1/daemon/orchestration/schedules/${encodeURIComponent(scheduleId)}`),
      ),
    );
    if (afterCancel.status !== 'cancelled') {
      throw failed('contract_violation', `cancel did not settle durably: inspect reports ${JSON.stringify(afterCancel.status)}`);
    }
    facts.cancel = { status: afterCancel.status, schedule_id: scheduleId };
    record('cancel', 'ok', { status: afterCancel.status });

    // 14. O3 restart: same home, same schedule/session, preserved effect.
    facts.ready_stop = await stopServiceVia(running, 'ready');
    // §3.4 / §7: a confirmed owned shutdown is required before a successor
    // process reuses the same home and port.
    if (facts.ready_stop.confirmed !== true) {
      throw failed(
        'cleanup_unconfirmed',
        'the pre-restart stop of the owned ready service was not confirmed; the restart would reuse a home and ' +
          'port that a possibly-live predecessor still owns',
      );
    }
    running = null;
    running = await startService({ home, port: servicePort, childEnv, evidenceDir, label: 'restart' });
    const restartPort = servicePortOf(running.discovery);
    const afterRestart = scheduleSummary(
      'GET /orchestration/schedules/{id} (restart)',
      requireOk(
        'GET /orchestration/schedules/{id} (restart)',
        await httpJson(restartPort, 'GET', `/v1/daemon/orchestration/schedules/${encodeURIComponent(scheduleId)}`),
      ),
    );
    if (afterRestart.current_session_id !== runId) {
      throw failed('contract_violation', 'restart changed the root orchestration session id for the same schedule');
    }
    if (afterRestart.status !== 'cancelled') {
      throw failed(
        'contract_violation',
        `restart changed the durable cancelled status to ${JSON.stringify(afterRestart.status)}`,
      );
    }
    const landedAfterRestart = readFileSync(effectPath);
    if (!landedAfterRestart.equals(fixture.declaredBytes)) {
      throw failed('contract_violation', 'committed workspace effect did not survive the restart');
    }
    const replay = await readEventStream(restartPort, runId, { lastEventId, maxFrames: 16 });
    if (replay.status !== 200) {
      throw statusFailure('GET /orchestration/sessions/{run_id}/events (restart)', {
        status: replay.status,
        json: replay.json,
        text: '',
      });
    }
    facts.restart = {
      schedule_status: afterRestart.status,
      current_session_id: runId,
      effect_sha256: sha256(landedAfterRestart),
      effect_preserved: true,
      history_frames: replay.frames.map((frame) => frame.event).filter(Boolean),
      history_unavailable: replay.frames.some((frame) => frame.event === 'history_unavailable'),
    };
    record('restart', 'ok', { status: afterRestart.status });

    facts.model_endpoint = {
      port: model.port,
      requests: model.observations.requests,
      paths: [...new Set(model.observations.paths)],
      unexpected_requests: model.observations.unexpected,
      authorization_header_present: model.observations.authorization_header_present,
    };
    record('model_endpoint', 'ok', {
      requests: model.observations.requests,
      unexpected: model.observations.unexpected,
    });
  } catch (error) {
    const isDriverFailure = error instanceof DriverFailure;
    receipt.outcome = isDriverFailure ? error.outcome : 'failed';
    receipt.blocker = {
      outcome: receipt.outcome,
      category: isDriverFailure ? error.category : 'internal',
      detail: error instanceof Error ? error.message : String(error),
    };
    if (typeof error?.serviceStderrLog === 'string') receipt.service_stderr_log = error.serviceStderrLog;
  } finally {
    const cleanups = [];
    if (running) {
      // Cleanup disposition: a failed or unconfirmed owned shutdown is retained
      // as evidence and overrides an otherwise `ok` journey, so the receipt and
      // the exit code can never report success while an owned service may still
      // be alive (§3.4 / §7).
      let cleanup = null;
      try {
        const stopped = await stopServiceVia(running, 'cleanup');
        cleanup = {
          subject: 'service',
          confirmed: stopped.confirmed === true,
          code: stopped.code ?? null,
          signal: stopped.signal ?? null,
          category: stopped.confirmed === true ? null : 'cleanup_unconfirmed',
          detail: stopped.detail ?? (stopped.confirmed === true ? null : 'the owned service shutdown was not confirmed'),
        };
      } catch (error) {
        const failure = error instanceof DriverFailure ? error : null;
        cleanup = {
          subject: 'service',
          confirmed: false,
          code: null,
          signal: null,
          category: failure?.category ?? 'cleanup_stop_failed',
          detail: error instanceof Error ? error.message : String(error),
        };
      }
      record('cleanup_stop', cleanup.confirmed ? 'ok' : 'unconfirmed', cleanup);
      cleanups.push(cleanup);
    }
    if (model) {
      // The loopback model endpoint is an owned child too (§6.1 confirmed
      // shutdown; §7: unconfirmed cleanup is a STOP with retained evidence).
      // `close()` always resolves with a checked, bounded verdict — a rejected
      // or throwing close, a close callback error and a close that never calls
      // back inside the bound all arrive here as unconfirmed and take the same
      // disposition as the service stop instead of being swallowed.
      const closed = await model.close();
      const cleanup = {
        subject: 'model_endpoint',
        confirmed: closed.confirmed === true,
        category: closed.confirmed === true ? null : 'cleanup_unconfirmed',
        detail:
          closed.detail ?? (closed.confirmed === true ? null : 'the loopback model endpoint shutdown was not confirmed'),
      };
      record('cleanup_model_endpoint', cleanup.confirmed ? 'ok' : 'unconfirmed', cleanup);
      cleanups.push(cleanup);
    }
    if (cleanups.length > 0) applyCleanupDisposition(receipt, cleanups);
    for (const child of owned.children) child.kill('SIGTERM');
    owned.children.clear();
    receipt.finished_at = new Date().toISOString();
  }
  return receipt;
}

function printSummary(receipt) {
  const lines = [
    `public first-workflow — mode=${receipt.mode} outcome=${receipt.outcome}`,
    `isolated root: ${receipt.isolated_root ?? '(not created)'}`,
    `ports: ${receipt.ports ? `service=${receipt.ports.service} model=${receipt.ports.model}` : '(not allocated)'}`,
  ];
  for (const step of receipt.steps) lines.push(`  [${step.status}] ${step.step}`);
  if (receipt.blocker) {
    lines.push(`blocker: ${receipt.blocker.outcome}/${receipt.blocker.category} — ${receipt.blocker.detail}`);
  }
  lines.push('receipt facts (redacted: no environment, headers or request bodies):');
  lines.push(JSON.stringify(receipt.facts, null, 2));
  process.stdout.write(`${lines.join('\n')}\n`);
}

async function main() {
  let options;
  try {
    options = parseArgs(process.argv.slice(2));
  } catch (error) {
    process.stderr.write(`${error.message}\n\n${USAGE}\n`);
    process.exit(64);
  }
  if (options.help) {
    process.stdout.write(`${USAGE}\n`);
    return;
  }

  const receipt = await runDeterministic(options);

  if (receipt.isolated_root) {
    try {
      writeFileSync(join(receipt.isolated_root, 'evidence', 'receipt.json'), `${JSON.stringify(receipt, null, 2)}\n`, 'utf8');
    } catch {
      // The receipt is also printed; a missing evidence file is not a second failure.
    }
    // Cleanup is ownership-scoped: a completed run removes its own temporary
    // root unless `--keep` was given, while a blocked or failed run retains its
    // evidence directory (and the printed receipt names it) so the STOP can be
    // diagnosed instead of erased.
    const keepRoot = options.keep || receipt.outcome !== 'ok';
    if (!keepRoot) {
      for (const path of owned.paths) rmSync(path, { recursive: true, force: true });
    } else if (receipt.outcome !== 'ok') {
      process.stdout.write(`evidence retained at ${receipt.isolated_root}\n`);
    }
  }

  if (options.json) process.stdout.write(`${JSON.stringify(receipt, null, 2)}\n`);
  else printSummary(receipt);

  process.exit(exitCodeFor(receipt.outcome));
}

/**
 * Scoped-check surface: the pure contract helpers of this driver. Importing the
 * module has no side effects; the journey only runs when the script is invoked
 * directly, so a focused authoring check can exercise the fixture extraction,
 * the child-environment isolation rule, the controlled model protocol, the
 * durable admission/boundary classification, the wire refusal classification,
 * the revision resolution, the bounded server close and the cleanup disposition
 * without starting any service.
 */
export {
  applyCleanupDisposition,
  awaitRunIdentity,
  awaitSteerBoundary,
  boundedServerClose,
  buildChildEnv,
  classifyAdmissionObservation,
  classifySteerBoundary,
  executionProjectionOf,
  exitCodeFor,
  findCommitRevision,
  readFixture,
  resolveEffectRevision,
  resolveExecutable,
  startModelEndpoint,
  statusFailure,
  steerBoundaryStop,
  summarizeChildEnv,
};

if (process.argv[1] !== undefined && resolve(process.argv[1]) === SCRIPT_PATH) {
  await main();
}
