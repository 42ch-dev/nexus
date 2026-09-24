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
 *   * W5/W6 (steer) are issued only at the fixture's own bounded pre-effect
 *     converge gate, identified from the checked-in YAML (the `converge:` state
 *     plus its `timeout_ms` and `on_timeout` target, which must be the effect
 *     state that owns the prompt, the open and the commit) and confirmed
 *     against the routed durable projection: the gate's A7 class is the
 *     parked converge/merge class, the run detail's `current_task_id` is that
 *     gate state, there is no durable human wait and no in-flight marker, and
 *     no model request has been dispatched yet. A plain resume is refused by
 *     the A4 fence at a human wait, so the driver never issues it there, never
 *     writes W5/W6 on a timing assumption and never claims a Steer it could
 *     not place. The gate must be observed QUIET (two reads with the same
 *     durable revision) and must stay parked across its own deadline before
 *     the W5 append and the W6 same-schedule resume; the resume re-drives that
 *     ONE run, whose deadline reroute enters the effect state;
 *   * the first real prompt of the run must observe the appended Idea at that
 *     next execution boundary: the fixture prompt renders
 *     `{{core_context.text}}`, and the driver's own loopback model endpoint
 *     records, per request, only non-secret structure — whether the prompt body
 *     carried the appended Idea, the message role sequence, the message count
 *     and whether the caller advertised tools — never the body, its headers or
 *     its key. Zero requests, a second request or a prompt without the Idea all
 *     fail the journey, and the recorded structure is what makes the sealed
 *     deny-all policy checkable on that same real request;
 *   * the same-run stream is read twice with distinct, asserted meanings: the
 *     bounded initial read (O1) and ONE reconnect from an earlier cursor of the
 *     SAME run (O2), which must replay the successors that cursor promised or
 *     answer with an explicit bounded `gap`. A read that merely idles out its
 *     window is never recorded as a replay, and only the driver's own window
 *     ends a read — a real transport failure rejects as itself. The public
 *     refusals of that route (absent run, malformed cursor, future cursor, a
 *     cursor from another epoch) are asserted case by case;
 *   * the receipt must carry the real **commit** revision of the declared
 *     workspace commit. It is taken from the authorized root session detail's
 *     schema-owned `workspace_commit` projection (contract §4, P1-T2): the
 *     read is the same-run root detail this driver already synchronizes on, and
 *     the member is accepted only as the canonical object
 *     (`revision` matching `rev_<id>` and `committed: true`, exactly those two
 *     keys). An absent, malformed, failed, extra-member, empty, non-`rev_` or
 *     otherwise unverifiable value yields no revision. A routed same-run frame
 *     is the only other admissible source, and only when it is identifiable as
 *     a workspace-commit response whose parsed payload is that same canonical
 *     object (`schemas/core/core-workspace-commit-response.schema.json`) — the
 *     current run-event vocabulary routes no such frame, and no frame text is
 *     ever evidence of a commit, so an unrelated `host_event`/`run_state`/`gap`
 *     payload that merely mentions a revision is refused. The durable
 *     `RunStateWire.state_revision` is recorded beside the commit revision as
 *     an explicitly labeled informational fact and never substitutes for it:
 *     run state is not the committed workspace, and the revision is never
 *     synthesized from file bytes or arbitrary context. A missing commit
 *     revision is a failure (`missing_commit_revision`), never a success;
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
 * stream, same-run cursor replay and refusal controls, steer, cancel, restart,
 * the declared workspace effect and the sealed no-tools policy/absence-of-side-
 * effects checks. The DRIVEN unsolicited `shell`/`editor`/`run_code` denial
 * belongs to the §6.2 adapter controls (a rejected unsolicited call is answered
 * with a follow-up model request, and a second request of the admitted prompt
 * fails this journey by contract), so this driver records which adapter control
 * owns that attempt instead of re-driving it. The separately authorized
 * one-request live action and its fetch guard are P3-T3.
 */

import { createHash } from 'node:crypto';
import { spawn, spawnSync } from 'node:child_process';
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
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
/** Bounded synchronization of W5/W6 against the fixture's bounded converge gate. */
const GATE_BOUNDARY_TIMEOUT_MS = 15_000;
const GATE_BOUNDARY_START_INTERVAL_MS = 100;
const GATE_BOUNDARY_MAX_INTERVAL_MS = 500;
/**
 * Extra time past the fixture's `timeout_ms` the driver waits with nobody
 * driving the run before the W6 resume re-drives it. The deadline is only
 * evaluated by a driver (`join_timeout_tick`), so the wait is what makes the
 * reroute deterministic instead of a race against the deadline.
 */
const GATE_DEADLINE_MARGIN_MS = 750;
/** Sanity bound on the fixture's declared `timeout_ms` (a usable bounded wait). */
const GATE_TIMEOUT_MS_MAX = 60_000;
/** Bounded wait for the post-resume durable boundary (the effect state's manual wait). */
const EFFECT_BOUNDARY_TIMEOUT_MS = 30_000;
const EFFECT_BOUNDARY_START_INTERVAL_MS = 100;
const EFFECT_BOUNDARY_MAX_INTERVAL_MS = 500;
/**
 * Bound of ONE same-run cursor reconnect read (O2/O3). The exclusive successor
 * replay, the refusal controls and the post-restart `history_unavailable` close
 * are all answered inside this window; a live run with nothing new to send
 * simply idles until it elapses — an explicit window of this driver's own, never
 * a socket-timeout race and never reported as a transport failure
 * (see `readEventStream`).
 */
const EVENT_REPLAY_TIMEOUT_MS = 2_000;
/**
 * Hostile-tool marker file NAMES under `<isolated root>/markers/` (the §6.2
 * adapter fixture's paths). `hostile_bodies_create_their_evidence_when_directly_run`
 * is the positive control proving an actually-executed unsolicited
 * `shell`/`str_replace_editor`/`run_code` body creates exactly these markers, so
 * their absence under this driver's isolated root is the end-to-end counterpart
 * of the sealed `deny_all` dispatch rejection
 * (`real_dsh_sealed_deny_all_rejects_unsolicited_tools_without_side_effects`).
 * Only names are compared; nothing is read out of any marker.
 */
const HOSTILE_MARKER_DIR = 'markers';
const HOSTILE_MARKER_NAMES = ['shell-marker', 'shell-marker.pid', 'editor-marker', 'run-code-marker'];
/** The Idea W5 appends before W6 resumes (S0-3 append-before-resume). */
const STEER_IDEA = 'public first-workflow steer';
/** Template the sealed prompt must render so the appended Idea reaches the boundary. */
const CORE_CONTEXT_TEMPLATE = '{{core_context.text}}';
/** Cap on the prompt body the loopback endpoint reads for its non-secret marker check. */
const MODEL_PROMPT_READ_CAP_BYTES = 262_144;
/** Cap on the recorded per-request structures (the journey authorizes ONE prompt request). */
const MODEL_REQUEST_STRUCTURE_CAP = 8;
/**
 * Ceiling for ONE placement observation taken outside a poll (the polls clamp
 * their reads to what is left of their own absolute bound). Same order as
 * `HTTP_TIMEOUT_MS`: a single read is bounded, and overrunning it is its own
 * typed STOP rather than a global CLI timeout.
 */
const PLACEMENT_READ_TIMEOUT_MS = 30_000;
/** Cap on the placement CLI's captured output (a DTO is small). */
const PLACEMENT_OUTPUT_CAP_BYTES = 1_048_576;

/**
 * `creator_schedules.status` terminal values (`crates/nexus-orchestration/src/
 * schedule`). A terminal row without a claimed run can never produce one, so it
 * is a refusal; anything else without a run id is still pending.
 */
const TERMINAL_SCHEDULE_STATUSES = new Set(['cancelled', 'completed', 'failed']);

/**
 * Durable A7 recovery class of the parked bounded converge/merge gate
 * (`crates/nexus-orchestration/src/resume_rules.rs` rule 5: a non-terminal run
 * parked at a scheduler gate with its live `_gate_park_<task>` marker and no
 * human wait token). This is the ONE durable class W5/W6 may be placed at: a
 * fully committed step boundary (`safe_boundary`) has no gate to reroute, a
 * durable human wait is fenced against a plain resume, and a terminal or
 * interrupted run is never steered.
 */
const GATE_PARK_RECOVERY_CLASS = 'converge_merge';
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
  1  unexpected internal failure, a runtime/recovery failure, a failed
     same-run replay/refusal assertion, or an unconfirmed/failed owned-service
     cleanup
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
 * The fixture's top-level state blocks: `{id, start, end}` line ranges, derived
 * from the shallowest `- id:` indent so the split follows the file's own shape.
 */
function stateBlocks(lines) {
  const entries = [];
  lines.forEach((line, index) => {
    const match = /^(\s*)- id:\s*(\S+)\s*$/.exec(line);
    if (match) entries.push({ id: match[2], index, indent: match[1].length });
  });
  if (entries.length === 0) throw failed('fixture_contract', 'fixture declares no states');
  const indent = Math.min(...entries.map((entry) => entry.indent));
  const states = entries.filter((entry) => entry.indent === indent);
  return states.map((entry, position) => ({
    id: entry.id,
    start: entry.index,
    end: position + 1 < states.length ? states[position + 1].index : lines.length,
  }));
}

/** The state block whose line range contains `index`. */
function stateBlockAt(blocks, index) {
  return blocks.find((block) => index >= block.start && index < block.end) ?? null;
}

/** The state that owns the enter action declaring `name: <capability>`. */
function stateOfCapability(lines, blocks, capability) {
  const nameIndex = lines.findIndex((line) => line.trim() === `name: ${capability}`);
  if (nameIndex < 0) {
    throw failed('fixture_contract', `fixture does not declare capability '${capability}'`);
  }
  const block = stateBlockAt(blocks, nameIndex);
  if (block === null) {
    throw failed('fixture_contract', `capability '${capability}' is declared outside a state block`);
  }
  return block;
}

/** Do any of a state block's lines match `pattern`? */
function blockMatches(lines, block, pattern) {
  for (let index = block.start; index < block.end; index += 1) {
    if (pattern.test(lines[index])) return true;
  }
  return false;
}

/** Value of `<key>: <scalar>` inside one state block. */
function blockScalar(lines, block, key) {
  const matches = [];
  for (let index = block.start; index < block.end; index += 1) {
    const match = new RegExp(`^\\s*${key}:\\s*(.+)$`).exec(lines[index]);
    if (match) matches.push(yamlScalar(match[1]));
  }
  return matches;
}

/**
 * The fixture's bounded pre-effect converge gate: the single state carrying a
 * `converge:` block, its `timeout_ms` deadline and the `on_timeout` state the
 * deadline reroutes onto. Every fact W5/W6 are placed with comes from here; the
 * driver never hard-codes a state name.
 */
function extractConvergeGate(lines, blocks) {
  const gateBlocks = blocks.filter((block) => blockMatches(lines, block, /^\s*converge:/));
  if (gateBlocks.length !== 1) {
    throw failed(
      'fixture_contract',
      `fixture must declare exactly one converge gate state (found ${gateBlocks.length})`,
    );
  }
  const block = gateBlocks[0];
  if (blockMatches(lines, block, /^\s*terminal:\s*true\s*$/)) {
    throw failed('fixture_contract', `converge gate '${block.id}' must not be terminal`);
  }
  if (blockMatches(lines, block, /^\s*enter:/)) {
    throw failed(
      'fixture_contract',
      `converge gate '${block.id}' declares enter actions; the bounded gate must be pre-effect (no enter action)`,
    );
  }
  const deadlines = blockScalar(lines, block, 'timeout_ms');
  if (deadlines.length !== 1) {
    throw failed('fixture_contract', `converge gate '${block.id}' must declare exactly one timeout_ms`);
  }
  const timeoutMs = Number(deadlines[0]);
  if (!Number.isInteger(timeoutMs) || timeoutMs <= 0 || timeoutMs > GATE_TIMEOUT_MS_MAX) {
    throw failed(
      'fixture_contract',
      `converge gate '${block.id}' timeout_ms ${JSON.stringify(deadlines[0])} is not a bounded positive deadline ` +
        `(1..${GATE_TIMEOUT_MS_MAX}ms)`,
    );
  }
  const targets = blockScalar(lines, block, 'on_timeout');
  if (targets.length !== 1) {
    throw failed(
      'fixture_contract',
      `converge gate '${block.id}' must declare exactly one on_timeout reroute target`,
    );
  }
  return { stateId: block.id, timeoutMs, onTimeoutStateId: targets[0] };
}

/**
 * The preset YAML file inside a scaffolded preset bundle.
 *
 * `POST /presets` answers with the bundle DIRECTORY (`scaffold_user_preset`
 * returns `bundle.display()`), while every reader of a preset path —
 * `validate_preset_file` for `POST /presets:validate`, `locate_preset` for
 * update/delete, and the runtime loader — resolves `preset.yaml` inside that
 * bundle. Handing the directory to the validator is read as a file, fails with
 * an untyped internal/FILE_READ_ERROR and surfaces as HTTP 500, so the driver
 * derives the file the PATCH above actually wrote.
 */
function presetYamlPath(bundlePath) {
  return join(bundlePath, 'preset.yaml');
}

/**
 * Read the checked-in fixture and extract every fact this driver depends on.
 * The fixture stays authoritative; the driver never hard-codes the scope, the
 * committed path, the committed bytes or the bounded gate's identity.
 */
function readFixture() {
  if (!existsSync(FIXTURE_PATH)) {
    throw failed('fixture_missing', `fixture not found at ${FIXTURE_PATH}`);
  }
  const yaml = readFileSync(FIXTURE_PATH, 'utf8');
  const lines = yaml.split('\n');
  const presetId = extractPresetId(lines);
  const blocks = stateBlocks(lines);
  const promptToolPolicy = extractCapabilityArg(lines, 'acp.prompt', 'tool_policy');
  const promptTemplate = extractCapabilityArg(lines, 'acp.prompt', 'prompt');
  const scopePath = extractCapabilityArg(lines, 'workspace.open', 'path');
  const changePath = extractCapabilityArg(lines, 'workspace.commit', 'path');
  const changeOp = extractCapabilityArg(lines, 'workspace.commit', 'op');
  const contentBase64 = extractCapabilityArg(lines, 'workspace.commit', 'contentBase64');
  const gate = extractConvergeGate(lines, blocks);
  const promptState = stateOfCapability(lines, blocks, 'acp.prompt');
  const openState = stateOfCapability(lines, blocks, 'workspace.open');
  const commitState = stateOfCapability(lines, blocks, 'workspace.commit');

  if (promptToolPolicy !== 'deny_all') {
    throw failed(
      'fixture_contract',
      `the fixture prompt must use the sealed deny-all scope, got ${JSON.stringify(promptToolPolicy)}`,
    );
  }
  if (!promptTemplate.includes(CORE_CONTEXT_TEMPLATE)) {
    throw failed(
      'fixture_contract',
      `the fixture prompt must render ${CORE_CONTEXT_TEMPLATE} so the first real prompt observes the appended Idea, ` +
        `got ${JSON.stringify(promptTemplate)}`,
    );
  }
  if (!(promptState.id === openState.id && promptState.id === commitState.id)) {
    throw failed(
      'fixture_contract',
      'the sealed prompt, workspace.open and the single workspace.commit must be enter actions of ONE effect state ' +
        `(found prompt=${promptState.id}, open=${openState.id}, commit=${commitState.id})`,
    );
  }
  if (gate.onTimeoutStateId !== promptState.id) {
    throw failed(
      'fixture_contract',
      `the gate '${gate.stateId}' must reroute its deadline onto the effect state '${promptState.id}', ` +
        `got ${JSON.stringify(gate.onTimeoutStateId)} — W5/W6 are placed at this gate only because its deadline ` +
        'enters that state',
    );
  }
  if (gate.stateId === promptState.id) {
    throw failed('fixture_contract', 'the bounded gate and the effect state must be distinct states');
  }
  // §4 publishes the session-detail `workspace_commit` projection only for a run
  // whose durable context holds the `workspace.commit` capability output as the
  // LAST capability the graph invoked. The fixture must therefore end its effect
  // state with that capability, or the receipt could never carry the revision.
  const effectStateCapabilities = [];
  for (let index = promptState.start; index < promptState.end; index += 1) {
    const match = /^\s*name:\s*(\S+)\s*$/.exec(lines[index]);
    if (match) effectStateCapabilities.push(match[1]);
  }
  if (effectStateCapabilities.at(-1) !== 'workspace.commit') {
    throw failed(
      'fixture_contract',
      `the effect state's LAST enter capability must be 'workspace.commit' (found ` +
        `${JSON.stringify(effectStateCapabilities)}): the authorized session-detail workspace_commit projection is ` +
        'published only for a run whose checkpointed last capability output is that successful commit (contract §4)',
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
    stateCount: blocks.length,
    scopePath,
    changePath,
    promptToolPolicy,
    promptTemplate,
    declaredBytes,
    declaredSha256: sha256(declaredBytes),
    gate: {
      stateId: gate.stateId,
      timeoutMs: gate.timeoutMs,
      effectStateId: promptState.id,
    },
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

/**
 * Percent-encode ONE path segment for a public request URL.
 *
 * RFC 3986 keeps `:` (and the sub-delims) legal inside a path segment, and this
 * service hands the raw segment to its core owner untouched: encoding a session
 * id's `:` as `%3A` makes the id arrive as a literal `preset%3Auuid` and the
 * authorized read answers `not_found`. So only the characters that would
 * corrupt the path structure are escaped, and the id reaches the owner with its
 * own value.
 */
function encodePathSegment(value) {
  return String(value).replace(/[%\/?#\s\u0000-\u001f\u007f]/g, (char) => {
    const code = char.codePointAt(0);
    return `%${code.toString(16).toUpperCase().padStart(2, '0')}`;
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
// Durable state classification (pure) and the bounded public polls
// ---------------------------------------------------------------------------

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
async function readScheduleInspect(port, scheduleId, step, { timeoutMs = HTTP_TIMEOUT_MS } = {}) {
  const inspected = requireOk(
    step,
    await httpJson(port, 'GET', `/v1/daemon/orchestration/schedules/${encodePathSegment(scheduleId)}`, { timeoutMs }),
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
 * One W4 HTTP observation of the admitted run: the schedule summary (owner of
 * `current_session_id`) plus the authorized root run detail (identity fields and
 * the §4 `workspace_commit` projection). Placement does NOT come from here — the
 * routed projection is empty on this baseline — but the run identity does: both
 * halves must PROVE they describe the run being synchronized, the schedule by
 * owning `runId` and the detail by ANSWERING `runId`. An absent, empty,
 * non-string or different `session_id` is a `wrong_run` STOP — a malformed or
 * misrouted detail must never contribute identity, and the CLI placement read
 * that follows is bound to this same root run.
 *
 * `deadlineMs` clamps each request to what is left of the caller's absolute
 * bound (the shared `HTTP_TIMEOUT_MS` is only a ceiling), so a slow surface can
 * never push the observation past the poll's own deadline.
 */
async function readDurableObservation(port, scheduleId, runId, step, { deadlineMs = null } = {}) {
  const budget = () =>
    deadlineMs === null ? HTTP_TIMEOUT_MS : Math.max(1, Math.min(HTTP_TIMEOUT_MS, deadlineMs - Date.now()));
  const { summary } = await readScheduleInspect(port, scheduleId, `${step} (schedule)`, { timeoutMs: budget() });
  if (summary.current_session_id !== runId) {
    throw failed(
      'wrong_run',
      `${step}: the schedule owns run ${JSON.stringify(summary.current_session_id)} instead of ${JSON.stringify(runId)}; ` +
        'W5/W6 address one admitted schedule/root run and are never re-placed on another',
    );
  }
  const detail = requireOk(
    `${step} (session)`,
    await httpJson(port, 'GET', `/v1/daemon/orchestration/sessions/${encodePathSegment(runId)}`, {
      timeoutMs: budget(),
    }),
  );
  const session = detail?.session;
  if (session === null || typeof session !== 'object') {
    throw failed('contract_violation', `${step}: the run detail response carries no session object`);
  }
  if (session.session_id !== runId) {
    throw failed(
      'wrong_run',
      `${step}: the run detail answered ${JSON.stringify(session.session_id)} for run ${JSON.stringify(runId)}; ` +
        'the gate identity is only ever read from the requested run',
    );
  }
  return {
    summary,
    session,
    detail,
  };
}

/**
 * Bound one already-started observation by an ABSOLUTE deadline. The HTTP
 * client's own timeout is an inactivity timer, so an active/trickling response
 * would outlive it; the guard makes the caller's declared bound real. A read
 * that loses the race is abandoned (its own request timeout still closes the
 * socket) and the caller converts the abort into its own typed STOP through
 * `onDeadline` — a transport fault that is NOT the deadline keeps propagating
 * as itself instead of being relabelled as a missing gate.
 */
async function boundedObservation(promise, deadlineMs, onDeadline) {
  let timer = null;
  const guard = new Promise((_resolve, rejectPromise) => {
    timer = setTimeout(() => rejectPromise(onDeadline()), Math.max(0, deadlineMs - Date.now()));
  });
  try {
    return await Promise.race([promise, guard]);
  } finally {
    clearTimeout(timer);
  }
}

/**
 * The A7 placement record the driver consumes from the public CLI DTO
 * (`nexus42 ops inspect <run> --json`), or `null` when the DTO cannot prove it
 * describes the synchronized root run.
 *
 * The routed W4 HTTP detail does not carry the A7 `execution` projection on this
 * baseline (`crates/nexus-core/src/execution/handle_ops.rs::inspect_schedule`
 * builds `execution: None`), so placement comes from the daemon-free operator
 * surface that DOES compute it — the same classifier the boot re-drive uses
 * (`crates/nexus-orchestration/src/resume_rules.rs`, `apps/nexus42/src/commands/ops.rs`).
 *
 * Fail-closed shape rules: the returned `session_id` must be the run the caller
 * asks about (the DTO's own root-run identity), `recovery_class` and
 * `current_task_id` must be non-empty strings and `state_revision` a
 * non-negative integer. Anything else — absent fields, wrong types, another
 * run's row — yields `null`, so a malformed or foreign placement can never
 * confirm a gate.
 */
function parsePlacementDto(dto, runId) {
  if (dto === null || typeof dto !== 'object' || Array.isArray(dto)) return null;
  if (typeof dto.session_id !== 'string' || dto.session_id !== runId) return null;
  if (typeof dto.recovery_class !== 'string' || dto.recovery_class.length === 0) return null;
  if (typeof dto.current_task_id !== 'string' || dto.current_task_id.length === 0) return null;
  if (!Number.isInteger(dto.state_revision) || dto.state_revision < 0) return null;
  const allowed = Array.isArray(dto.allowed_actions)
    ? dto.allowed_actions.filter((action) => typeof action === 'string')
    : [];
  return {
    session_id: dto.session_id,
    recovery_class: dto.recovery_class,
    allowed_actions: allowed,
    task_id: dto.current_task_id,
    state_revision: dto.state_revision,
    db_status: typeof dto.db_status === 'string' ? dto.db_status : null,
    execution_version: Number.isInteger(dto.execution_version) ? dto.execution_version : null,
    wait_id: null,
    wait_kind: null,
  };
}

/**
 * Run one placement CLI invocation ASYNCHRONOUSLY.
 *
 * `spawnSync` (used by the setup CLI calls) BLOCKS the JS event loop, so a
 * stalled command would outlive any deadline timer a caller armed — a declared
 * 15 s gate poll could be held for the global CLI timeout. This runner spawns
 * the child without blocking, so the poll's absolute bound really fires, and it
 * kills the child on the caller's clamped timeout. A timeout is reported as
 * `timedOut` (never as a spawn error), so it can become the poll's own typed
 * STOP instead of `cli_unavailable`.
 *
 * The child joins the driver's owned children, so it is killed with them if the
 * journey fails, and its captured output is capped.
 *
 * @returns {Promise<{status: number|null, signal: string|null, stdout: string,
 *   stderr: string, timedOut: boolean, spawnError: string|null}>}
 */
function runPlacementCli(cliBinary, args, childEnv, timeoutMs) {
  return new Promise((resolvePromise) => {
    const child = spawn(cliBinary, args, { env: childEnv, stdio: ['ignore', 'pipe', 'pipe'] });
    owned.children.add(child);
    let stdout = '';
    let stderr = '';
    let settled = false;
    const finish = (result) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      owned.children.delete(child);
      resolvePromise(result);
    };
    const timer = setTimeout(() => {
      child.kill('SIGKILL');
      finish({ status: null, signal: 'SIGKILL', stdout, stderr, timedOut: true, spawnError: null });
    }, timeoutMs);
    child.stdout.setEncoding('utf8');
    child.stdout.on('data', (chunk) => {
      if (stdout.length < PLACEMENT_OUTPUT_CAP_BYTES) stdout += chunk;
    });
    child.stderr.setEncoding('utf8');
    child.stderr.on('data', (chunk) => {
      if (stderr.length < PLACEMENT_OUTPUT_CAP_BYTES) stderr += chunk;
    });
    child.once('error', (error) =>
      finish({ status: null, signal: null, stdout, stderr, timedOut: false, spawnError: error.message }),
    );
    // Settle on `close`, NOT `exit`: `exit` fires when the process ends, while
    // its stdio streams may still be delivering buffered data, so a DTO larger
    // than the pipe buffer could be parsed from a truncated string. `close`
    // fires only after stdout/stderr are drained and closed, so the captured
    // output is complete before anything reads it. The timeout path above still
    // wins when the child (or a process holding its pipe) never closes.
    child.once('close', (code, signal) =>
      finish({ status: code, signal, stdout, stderr, timedOut: false, spawnError: null }),
    );
  });
}

/**
 * Read one placement observation for the synchronized root run from the public
 * daemon-free operator surface. The CLI is handed the SAME root run id the W4
 * read resolved, and every failure is a typed, fail-closed STOP: a spawn failure
 * keeps its unmet-prerequisite outcome, an overrun returns the caller's own
 * poll STOP (or `placement_timeout` for a single bounded observation), a
 * non-zero exit and non-JSON stdout are `failed/placement_unreadable`, and a DTO
 * that cannot prove the run (or misses a field the driver consumes) is
 * `failed/placement_contract`.
 *
 * `timeoutMs` must be the caller's remaining budget: inside a poll that is the
 * time left of the poll's absolute deadline, so the placement read can never
 * outlive the declared gate/effect bound.
 */
async function readPlacementViaCli(
  cliBinary,
  childEnv,
  runId,
  step,
  { timeoutMs = PLACEMENT_READ_TIMEOUT_MS, onTimeout = null } = {},
) {
  const label = `${step}: nexus42 ops inspect <run> --json`;
  const result = await runPlacementCli(cliBinary, ['ops', 'inspect', runId, '--json'], childEnv, timeoutMs);
  if (result.timedOut) {
    if (onTimeout !== null) throw onTimeout();
    throw failed(
      'placement_timeout',
      `${label}: the placement read did not answer within ${timeoutMs}ms and was aborted`,
    );
  }
  if (result.spawnError !== null) throw blocked('cli_unavailable', `${label}: ${result.spawnError}`);
  if (result.status !== 0) {
    const detail = tailLines(result.stderr, 6).join(' | ');
    throw failed(
      'placement_unreadable',
      `${label}: exit ${result.status}${detail ? ` — ${detail}` : ''}`,
    );
  }
  const stdout = result.stdout;
  let dto = null;
  try {
    dto = JSON.parse(stdout);
  } catch {
    throw failed('placement_unreadable', `${label}: the CLI answered no parseable A7 placement DTO`);
  }
  const placement = parsePlacementDto(dto, runId);
  if (placement === null) {
    throw failed(
      'placement_contract',
      `${label}: the placement DTO does not prove run ${JSON.stringify(runId)} with a recovery_class, a ` +
        `current_task_id and an integer state_revision (answered ${JSON.stringify({
          session_id: dto?.session_id ?? null,
          recovery_class: dto?.recovery_class ?? null,
          current_task_id: dto?.current_task_id ?? null,
          state_revision: dto?.state_revision ?? null,
        })})`,
    );
  }
  return placement;
}

/**
 * Classify one placement observation against the fixture's bounded converge
 * gate.
 *
 * `gate` is the only placement W5/W6 accept: the A7 class is the parked
 * converge/merge gate (no human wait token, no in-flight marker) AND the run's
 * `current_task_id` is the fixture's gate state. Every other state is decisive:
 *
 *   * `human_wait` — a durable A4 wait exists; a plain `resume` is fenced there
 *     and issuing it would bypass the human wait (§3.3 / §6.1).
 *   * `terminal` — the run already settled.
 *   * `wrong_gate` — a converge/merge park that is NOT the fixture's gate, so
 *     the graph does not match the checked-in journey.
 *   * `transient` — a fully committed step boundary or an in-flight/other
 *     class; the run may legitimately still be walking into the gate.
 *   * `unobservable` — the placement surface returned nothing at all.
 */
function classifyPreEffectGate(placement, gateStateId) {
  if (placement === null) return { state: 'unobservable', observed: null };
  if (placement.recovery_class === 'terminal') return { state: 'terminal', observed: placement };
  if (placement.wait_id !== null || placement.recovery_class === 'human_wait') {
    return { state: 'human_wait', observed: placement };
  }
  if (placement.recovery_class !== GATE_PARK_RECOVERY_CLASS) return { state: 'transient', observed: placement };
  if (placement.task_id !== gateStateId) return { state: 'wrong_gate', observed: placement };
  return { state: 'gate', observed: placement };
}

/**
 * Classify one placement observation against the effect state's manual wait —
 * the boundary the deadline reroute must reach, which proves the reroute
 * happened and that W5/W6 were exercised strictly before the final manual wait.
 *
 *   * `manual_wait` — the effect state rests in a durable human wait.
 *   * `wrong_state` — something else rests in a human wait.
 *   * `terminal` — the run settled without reaching that wait.
 *   * `pending` — still walking; re-read until the bound.
 */
function classifyManualWaitBoundary(placement, effectStateId) {
  if (placement === null) return { state: 'unobservable', observed: null };
  if (placement.recovery_class === 'terminal') return { state: 'terminal', observed: placement };
  if (placement.wait_id !== null || placement.recovery_class === 'human_wait') {
    return placement.task_id === effectStateId
      ? { state: 'manual_wait', observed: placement }
      : { state: 'wrong_state', observed: placement };
  }
  return { state: 'pending', observed: placement };
}

/** Turn a non-gate placement into the exact, typed STOP for that state. */
function preEffectGateStop(step, boundary, gateStateId) {
  const observed = JSON.stringify(boundary.observed);
  switch (boundary.state) {
    case 'unobservable':
      return blocked(
        'gate_unobservable',
        `${step}: the public placement surface answered no A7 record ` +
          '(recovery_class/current_task_id/state_revision), so the fixture-declared pre-effect gate cannot be ' +
          'confirmed; W5/W6 are not issued and no Steer success is claimed',
      );
    case 'human_wait':
      return failed(
        'gate_human_wait',
        `${step}: the run already rests in a durable human wait (${observed}) instead of the fixture's bounded ` +
          `pre-effect gate '${gateStateId}'; the A4 fence makes a plain resume illegal there, so W5/W6 are not ` +
          'issued rather than bypassing the human wait',
      );
    case 'terminal':
      return failed(
        'gate_terminal',
        `${step}: the run is already terminal (${observed}); W5/W6 are not issued`,
      );
    case 'wrong_gate':
      return failed(
        'gate_wrong_state',
        `${step}: the run is parked at a different converge/merge gate than the fixture gate ` +
          `'${gateStateId}' (${observed}); W5/W6 are not issued on a graph the fixture does not declare`,
      );
    default:
      return failed(
        'gate_not_observed',
        `${step}: no steady bounded converge gate '${gateStateId}' was observed within ` +
          `${GATE_BOUNDARY_TIMEOUT_MS}ms (last observation ${observed}); W5/W6 are not issued on a timing assumption`,
      );
  }
}

/**
 * Bounded synchronization on the fixture's bounded pre-effect gate, reading the
 * placement through the public operator surface. The gate is accepted only when
 * it is observed QUIET — two reads at least one poll apart with the SAME integer
 * `state_revision`, still parked at the fixture's gate state and still
 * pre-effect (no model request dispatched). A decisive non-gate observation (a
 * human wait, a terminal run, a foreign gate) STOPS immediately instead of
 * being retried into a timing assumption.
 *
 * The poll bound is an absolute wall-clock deadline: no read starts after it, an
 * observation that does not answer inside it is aborted (`boundedObservation`)
 * and the gate is never accepted after it. A gate observation whose record
 * carries no integer `state_revision` is refused outright — repeated absence is
 * not evidence that the active run stayed at one durable revision, which is
 * exactly what "quiet" claims.
 */
async function awaitPreEffectGate({ readPlacement, runId, gate, model }) {
  const deadline = Date.now() + GATE_BOUNDARY_TIMEOUT_MS;
  let interval = GATE_BOUNDARY_START_INTERVAL_MS;
  let polls = 0;
  let quiet = 0;
  let quietRevision = null;
  let observation = { state: 'transient', observed: null };
  const boundStop = () => preEffectGateStop('gate', observation, gate.stateId);
  for (;;) {
    const remaining = deadline - Date.now();
    if (remaining <= 0) throw boundStop();
    let placement;
    try {
      placement = await boundedObservation(
        // The read is clamped to what is left of the poll's absolute bound, and
        // its own overrun aborts as this poll's typed STOP (never as a global
        // CLI timeout), so the declared bound is the one that holds.
        readPlacement(runId, 'gate', { timeoutMs: Math.max(1, remaining), onTimeout: boundStop }),
        deadline,
        boundStop,
      );
    } catch (error) {
      // An abort that lands at or after the declared bound is this poll's typed
      // STOP, whatever aborted it; a placement failure BEFORE the bound keeps
      // propagating as itself, so a refused CLI is never relabelled as a
      // missing gate.
      if (!(error instanceof DriverFailure) && Date.now() >= deadline) throw boundStop();
      throw error;
    }
    polls += 1;
    observation = classifyPreEffectGate(placement, gate.stateId);
    if (observation.state === 'gate') {
      if (model.observations.requests !== 0) {
        throw failed(
          'gate_not_pre_effect',
          `gate: the run already dispatched ${model.observations.requests} model request(s) before the bounded gate ` +
            'was confirmed; the fixture gate must be pre-effect',
        );
      }
      if (!Number.isInteger(observation.observed.state_revision) || observation.observed.state_revision < 0) {
        throw blocked(
          'gate_revision_unobservable',
          `gate: the parked gate placement carries no integer durable state revision ` +
            `(${JSON.stringify(observation.observed)}), so an unchanged revision cannot be established; a repeated ` +
            'absent revision is not quiet and W5/W6 are not issued',
        );
      }
      const revision = observation.observed.state_revision;
      quiet = quiet > 0 && revision === quietRevision ? quiet + 1 : 1;
      quietRevision = revision;
      // Strict bound: the gate is never accepted after the declared deadline.
      if (quiet >= 2 && Date.now() < deadline) {
        return { ...observation, polls, quiet_polls: quiet, parked_at_ms: Date.now() };
      }
    } else if (observation.state !== 'transient') {
      throw preEffectGateStop('gate', observation, gate.stateId);
    }
    if (Date.now() >= deadline) throw boundStop();
    await sleep(Math.max(0, Math.min(interval, deadline - Date.now())));
    interval = Math.min(interval * 2, GATE_BOUNDARY_MAX_INTERVAL_MS);
  }
}

/**
 * Bounded wait for the durable boundary the deadline reroute must reach: the
 * effect state resting in its manual wait, read through the public operator
 * surface. A human wait at any OTHER state, a settled run or an absent record is
 * a typed STOP, never a pass. The bound is absolute in the same way as the gate
 * poll: no read starts after it, an observation that overruns it is aborted, and
 * the boundary is never accepted after it.
 */
async function awaitEffectBoundary({ readPlacement, runId, effectStateId }) {
  const deadline = Date.now() + EFFECT_BOUNDARY_TIMEOUT_MS;
  let interval = EFFECT_BOUNDARY_START_INTERVAL_MS;
  let polls = 0;
  let observation = { state: 'pending', observed: null };
  const boundStop = () =>
    failed(
      'effect_boundary_timeout',
      `effect boundary: '${effectStateId}' did not reach its manual wait within ${EFFECT_BOUNDARY_TIMEOUT_MS}ms ` +
        `across ${polls} reads (last observation ${JSON.stringify(observation.observed)})`,
    );
  for (;;) {
    const remaining = deadline - Date.now();
    if (remaining <= 0) throw boundStop();
    let placement;
    try {
      placement = await boundedObservation(
        // Same clamp as the gate poll: the placement read cannot outlive this
        // poll's absolute bound, and its overrun is this poll's typed STOP.
        readPlacement(runId, 'effect boundary', { timeoutMs: Math.max(1, remaining), onTimeout: boundStop }),
        deadline,
        boundStop,
      );
    } catch (error) {
      if (!(error instanceof DriverFailure) && Date.now() >= deadline) throw boundStop();
      throw error;
    }
    polls += 1;
    observation = classifyManualWaitBoundary(placement, effectStateId);
    if (observation.state === 'manual_wait') {
      if (Date.now() < deadline) return { ...observation, polls };
      throw boundStop();
    }
    if (observation.state !== 'pending') {
      const observed = JSON.stringify(observation.observed);
      if (observation.state === 'terminal') {
        throw failed('effect_boundary_terminal', `effect boundary: the run settled without reaching '${effectStateId}' (${observed})`);
      }
      throw failed(
        `effect_boundary_${observation.state}`,
        `effect boundary: the durable boundary is not the effect state's manual wait '${effectStateId}' (${observed})`,
      );
    }
    if (Date.now() >= deadline) throw boundStop();
    await sleep(Math.max(0, Math.min(interval, deadline - Date.now())));
    interval = Math.min(interval * 2, EFFECT_BOUNDARY_MAX_INTERVAL_MS);
  }
}

/**
 * The sealed no-tools policy must govern the observed prompt of the REAL
 * installed runtime (S3-3 as this driver can see it end-to-end): every model
 * request the sealed composition made advertised NO tools (the `tools` key
 * absent, not an empty array — the dispatch-level mechanism §6.2's adapter
 * control asserts), the admitted turn carried no tool result and no assistant
 * tool call, and its first request was the sealed `[system, user]` shape.
 *
 * The driven unsolicited-tool-call attempt itself belongs to the §6.2 adapter
 * controls (`real_dsh_sealed_deny_all_rejects_unsolicited_tools_without_side_effects`
 * scripts three hostile calls and asserts the rejections plus the marker
 * absence). This driver deliberately does NOT re-drive it: an unsolicited call
 * is rejected at dispatch and answered with a follow-up model request, and a
 * second request of the admitted prompt fails the journey by contract
 * ("Zero requests, a second request … fail the journey"). What it proves here
 * instead is the other half of the same claim — the policy really is sealed,
 * and no unsolicited tool side effect exists anywhere in the isolated root
 * ({@link assertNoHostileMarkers}, {@link assertScopeEffectOnly}).
 *
 * @throws {DriverFailure} `failed`/`sealed_policy_violation` when any request
 *   of the admitted prompt advertised or carried tool traffic.
 */
function assertSealedToolPolicy(observations) {
  const structures = Array.isArray(observations?.structures) ? observations.structures : [];
  if (structures.length !== observations?.requests) {
    throw failed(
      'sealed_policy_unrecorded',
      `the loopback endpoint recorded ${structures.length} request structure(s) for ${observations?.requests} ` +
        'request(s); the sealed-policy check would be partial',
    );
  }
  if (structures.length === 0) {
    throw failed('sealed_policy_unrecorded', 'the sealed prompt made no model request, so its tool policy is unproven');
  }
  for (const [index, structure] of structures.entries()) {
    const request = index + 1;
    if (structure.parsed !== true) {
      throw failed('sealed_policy_unrecorded', `model request ${request} was not a parseable JSON body`);
    }
    if (structure.has_tools_key !== false || structure.tools_len !== null) {
      throw failed(
        'sealed_policy_violation',
        `model request ${request} advertised tools (has_tools_key=${structure.has_tools_key}, ` +
          `tools_len=${structure.tools_len}); the sealed deny_all scope must advertise no tools at all`,
      );
    }
    if (structure.tool_role_messages !== 0 || structure.assistant_tool_calls !== 0) {
      throw failed(
        'sealed_policy_violation',
        `model request ${request} carried tool traffic (tool role messages=${structure.tool_role_messages}, ` +
          `assistant tool calls=${structure.assistant_tool_calls})`,
      );
    }
  }
  const first = structures[0];
  if (first.roles.length !== 2 || first.roles[0] !== 'system' || first.roles[1] !== 'user') {
    throw failed(
      'sealed_policy_violation',
      `the admitted prompt's first model request must be the sealed [system, user] shape, got ` +
        `${JSON.stringify(first.roles)}`,
    );
  }
  return {
    model_requests: structures.length,
    advertised_tools: false,
    tool_role_messages: 0,
    assistant_tool_calls: 0,
    first_request_roles: first.roles,
    model: first.model,
  };
}

/**
 * The bounded inventory of one directory tree: relative path, kind, byte size
 * and SHA-256 for files. No file content is retained, the walk is depth- and
 * entry-capped, and the order is sorted so a receipt is comparable across runs.
 * `truncated` reports that a cap stopped the walk, so a caller that asserts
 * "nothing else exists" can refuse instead of reading a partial listing as a
 * clean one.
 *
 * @returns {{entries: Array<{path: string, kind: string, bytes?: number, sha256?: string}>, truncated: boolean}}
 */
function directoryInventory(root, { maxDepth = 3, maxEntries = 64 } = {}) {
  const entries = [];
  let truncated = false;
  const walk = (dir, prefix, depth) => {
    if (entries.length >= maxEntries) {
      truncated = true;
      return;
    }
    if (depth > maxDepth) {
      truncated = true;
      return;
    }
    let dirents = [];
    try {
      dirents = readdirSync(dir, { withFileTypes: true });
    } catch {
      truncated = true;
      return;
    }
    const sorted = [...dirents].sort((left, right) => (left.name < right.name ? -1 : left.name > right.name ? 1 : 0));
    for (const dirent of sorted) {
      if (entries.length >= maxEntries) {
        truncated = true;
        return;
      }
      const relative = prefix === '' ? dirent.name : `${prefix}/${dirent.name}`;
      if (dirent.isDirectory()) {
        entries.push({ path: relative, kind: 'dir' });
        walk(join(dir, dirent.name), relative, depth + 1);
      } else if (dirent.isFile()) {
        const bytes = readFileSync(join(dir, dirent.name));
        entries.push({ path: relative, kind: 'file', bytes: bytes.length, sha256: sha256(bytes) });
      } else {
        entries.push({ path: relative, kind: 'other' });
      }
    }
  };
  walk(root, '', 1);
  return { entries, truncated };
}

/**
 * Marker files that would exist if an unsolicited `shell`/`str_replace_editor`/
 * `run_code` body had actually executed against this isolated root (§6.2's
 * hostile-marker paths). Only presence is checked; no marker is read.
 */
function hostileMarkersPresent(isolatedRoot) {
  const dir = join(isolatedRoot, HOSTILE_MARKER_DIR);
  const present = [];
  for (const name of HOSTILE_MARKER_NAMES) {
    if (existsSync(join(dir, name))) present.push(name);
  }
  return { checked: [...HOSTILE_MARKER_NAMES], present, directory: HOSTILE_MARKER_DIR };
}

/**
 * @throws {DriverFailure} `failed`/`unsolicited_side_effect` when a hostile
 *   marker exists — an executed unsolicited tool body, never an acceptable
 *   result.
 */
function assertNoHostileMarkers(isolatedRoot) {
  const markers = hostileMarkersPresent(isolatedRoot);
  if (markers.present.length > 0) {
    throw failed(
      'unsolicited_side_effect',
      `the sealed run left hostile tool marker(s) ${JSON.stringify(markers.present)} under ` +
        `${HOSTILE_MARKER_DIR}/: an unsolicited shell/editor/run_code body really executed`,
    );
  }
  return markers;
}

/**
 * The opened scope must carry EXACTLY the changes the checked-in fixture
 * declares — nothing else. This is the driver's end-to-end side-effect check on
 * the real admitted run: a tool that had produced any unsolicited write inside
 * the scope would appear here as an entry no fixture change declares, and the
 * declared file's bytes/sha are already compared against the same fixture in
 * step 12.
 */
function assertScopeEffectOnly(scopeDir, fixture) {
  const { entries: inventory, truncated } = directoryInventory(scopeDir, { maxDepth: 2, maxEntries: 32 });
  if (truncated) {
    throw failed(
      'side_effect_inventory_incomplete',
      `the inventory of the opened scope ${JSON.stringify(scopeDir)} hit its depth/entry cap, so "nothing else was ` +
        'written" cannot be asserted from it',
    );
  }
  const expected = fixture.changePath;
  const unexpected = inventory.filter((entry) => entry.kind !== 'file' || entry.path !== expected);
  if (unexpected.length > 0) {
    throw failed(
      'unsolicited_side_effect',
      `the opened scope carries ${unexpected.length} entry/entries the checked-in fixture never declared ` +
        `(${unexpected.map((entry) => `${entry.path}:${entry.kind}`).join(', ')}); the only authorized effect is ` +
        JSON.stringify(expected),
    );
  }
  return inventory;
}

/**
 * The sealed-prompt cardinality contract of the effect boundary: the ONE
 * admitted schedule/run crosses the effect state exactly once, so exactly one
 * model request is authorized, it must carry the Idea W5 appended (the
 * boundary's own evidence that it consumed the committed core-context version),
 * and there is no second dispatch. Returns the checked counts for the receipt.
 */
function assertSealedPromptCardinality(observations) {
  const requests = observations?.requests;
  if (requests === 0) {
    throw failed(
      'effect_missing',
      'the effect state reached its manual wait without dispatching the sealed prompt; no model request was performed',
    );
  }
  if (requests > 1) {
    throw failed(
      'duplicate_effect',
      `the sealed prompt was dispatched ${requests} times across one admitted schedule/run; ` +
        'exactly one pre-manual prompt is authorized',
    );
  }
  if (observations.prompt_with_idea !== 1) {
    throw failed(
      'steer_idea_not_observed',
      'the first real prompt of the run did not carry the appended Idea; the next execution boundary did not ' +
        'consume the committed core-context version W5 appended',
    );
  }
  return { requests, prompt_with_idea: observations.prompt_with_idea };
}

/**
 * The resumed schedule must still own exactly ONE run for this preset: the
 * deadline reroute re-drives the admitted run, so a resume that minted a second
 * run is a failure rather than a passing journey.
 */
function assertSingleRun(items, presetId, runId) {
  if (!Array.isArray(items)) {
    throw failed('contract_violation', 'GET /orchestration/sessions carries no items array');
  }
  const runs = items.filter((row) => row?.preset_id === presetId);
  if (runs.length !== 1 || runs[0].session_id !== runId) {
    throw failed(
      'duplicate_run',
      `the resumed schedule must own exactly one run: found ${runs.length} run(s) for preset ` +
        `${JSON.stringify(presetId)} (${runs.map((row) => JSON.stringify(row?.session_id)).join(', ')}), ` +
        `expected [${JSON.stringify(runId)}]`,
    );
  }
  return { runs: runs.length, run_id: runs[0].session_id };
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
  // The service's own stdout beyond the ready line carries its request/SSE
  // diagnostics; the bounded tail is retained as evidence so a stream symptom
  // is diagnosable from the receipt instead of only from the console.
  const stdoutLog = join(evidenceDir, `service-${label}.stdout.log`);
  const writeStdoutTail = () => {
    try {
      writeFileSync(stdoutLog, `${tailLines(stdoutBuffer, 200).join('\n')}\n`, 'utf8');
    } catch {
      // Evidence capture never fails the journey.
    }
  };
  writeStdoutTail();
  if (discovery === null || typeof discovery !== 'object' || typeof discovery.instance_id !== 'string') {
    throw failed('contract_violation', `service ${label} ready record is missing instance_id`);
  }
  return { child, discovery, evidence: { stdoutLog, writeStdoutTail } };
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
 * Non-secret STRUCTURE of one model request body: the message role sequence,
 * the message count, whether the caller advertised tools at all and how many,
 * whether a tool result or an assistant tool call is already in the
 * conversation, and the model name the runtime named. Nothing else is derived
 * from the body: no content, no header and no key is retained, echoed or written
 * anywhere, and no value out of the body is compared except the driver's own
 * non-secret Steer Idea (checked by the caller).
 *
 * These are the facts that make the sealed no-tools policy checkable on the
 * real admitted run: §6.2's adapter control asserts exactly this shape for the
 * sealed composition's first request (roles `[system, user]`, the `tools` key
 * ABSENT, `tools_len == 0`).
 */
function modelRequestStructure(text) {
  const empty = {
    parsed: false,
    roles: [],
    message_count: 0,
    has_tools_key: null,
    tools_len: null,
    tool_role_messages: 0,
    assistant_tool_calls: 0,
    model: null,
  };
  let payload = null;
  try {
    payload = JSON.parse(text);
  } catch {
    return empty;
  }
  if (payload === null || typeof payload !== 'object' || Array.isArray(payload)) return empty;
  const messages = Array.isArray(payload.messages) ? payload.messages : [];
  const roles = messages.map((message) =>
    message !== null && typeof message === 'object' && typeof message.role === 'string' ? message.role : '<missing>',
  );
  const toolRoleMessages = roles.filter((role) => role === 'tool').length;
  const assistantToolCalls = messages.filter(
    (message) => Array.isArray(message?.tool_calls) && message.tool_calls.length > 0,
  ).length;
  return {
    parsed: true,
    roles,
    message_count: messages.length,
    has_tools_key: Object.hasOwn(payload, 'tools'),
    tools_len: Array.isArray(payload.tools) ? payload.tools.length : null,
    tool_role_messages: toolRoleMessages,
    assistant_tool_calls: assistantToolCalls,
    model: typeof payload.model === 'string' ? payload.model : null,
  };
}

/**
 * DeepSeek-compatible loopback endpoint. It answers one SSE completion for
 * `POST /chat/completions` and records only the non-secret STRUCTURE of each
 * request ({@link modelRequestStructure}) plus one boolean per request: whether
 * the prompt body carried the appended Steer Idea, which is the boundary's own
 * evidence that the first real prompt of the run consumed the committed
 * core-context version (W5). The body is read into a bounded buffer for those
 * comparisons only and is never stored, echoed or written anywhere.
 */
function startModelEndpoint() {
  const observations = {
    requests: 0,
    paths: [],
    unexpected: 0,
    authorization_header_present: false,
    prompt_with_idea: 0,
    structures: [],
  };
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
    let promptBytes = 0;
    let promptHasIdea = false;
    const chunks = [];
    req.on('data', (chunk) => {
      if (promptBytes >= MODEL_PROMPT_READ_CAP_BYTES) return;
      promptBytes += chunk.length;
      // The marker comparison reads the request the sealed preset rendered; the
      // matched substring is the driver's own non-secret Idea, and nothing is
      // retained beyond this boolean.
      if (!promptHasIdea && chunk.includes(STEER_IDEA)) promptHasIdea = true;
      chunks.push(chunk);
    });
    req.on('end', () => {
      if (promptHasIdea) observations.prompt_with_idea += 1;
      // Bounded: the journey authorizes ONE prompt request, so the cap only
      // keeps a misbehaving runtime from growing the receipt.
      if (observations.structures.length < MODEL_REQUEST_STRUCTURE_CAP) {
        observations.structures.push(modelRequestStructure(Buffer.concat(chunks).toString('utf8')));
      }
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
 * Read a bounded slice of a same-run SSE stream.
 *
 * The read window is this driver's OWN timer and the only authority for it: the
 * socket is closed by the driver when the window elapses, so a live run with
 * nothing new to send can never be read as a transport failure. (The previous
 * shape armed `request.setTimeout(timeoutMs, request.destroy)` as well, which is
 * a socket-idle timer: on an idle reconnect — no frame after the response
 * headers — that timer fires BEFORE the response-armed window, the destroy
 * surfaces as `ECONNRESET` / `socket hang up`, and an ordinary no-new-frame
 * window was reported as a failure.)
 *
 * The three outcomes are exact and mutually exclusive:
 *
 *   * `timed_out: true`, `closed: false` — the window elapsed with the stream
 *     still open: a normal no-new-frame window;
 *   * `closed: true`, `timed_out: false` — the SERVER ended the stream (terminal
 *     ring, the `history_unavailable` close, subscriber eviction);
 *   * a rejected promise — a REAL transport failure (a refused connection, an
 *     aborted/reset response) surfaced as itself instead of being read as an
 *     ordinary short stream.
 *
 * The read settles BEFORE it destroys the socket, so its own teardown can never
 * be reported as a transport failure. A non-2xx answer is surfaced with its
 * status so the caller can classify a refusal.
 */
function readEventStream(port, runId, { lastEventId, maxFrames = MAX_EVENT_FRAMES, timeoutMs = EVENT_STREAM_TIMEOUT_MS } = {}) {
  return new Promise((resolvePromise, rejectPromise) => {
    const headers = { accept: 'text/event-stream' };
    if (lastEventId) headers['last-event-id'] = lastEventId;
    const path = `/v1/daemon/orchestration/sessions/${encodePathSegment(runId)}/events`;
    const frames = [];
    let buffer = '';
    let status = 0;
    let json = null;
    let closed = false;
    let settled = false;
    let timer = null;
    const settle = (timedOut) => {
      if (settled) return false;
      settled = true;
      clearTimeout(timer);
      resolvePromise({ status, json, frames, closed, timed_out: timedOut });
      return true;
    };
    const refuse = (error) => {
      if (settled) return false;
      settled = true;
      clearTimeout(timer);
      rejectPromise(error);
      return true;
    };
    const request = httpRequest({ host: '127.0.0.1', port, path, method: 'GET', headers }, (response) => {
      status = response.statusCode ?? 0;
      if (status !== 200) {
        const chunks = [];
        response.on('data', (chunk) => chunks.push(chunk));
        response.on('end', () => {
          const text = Buffer.concat(chunks).toString('utf8');
          try {
            json = text.length > 0 ? JSON.parse(text) : null;
          } catch {
            json = null;
          }
          closed = true;
          settle(false);
        });
        response.on('error', (error) => refuse(error));
        return;
      }
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
            if (settle(false)) request.destroy();
            return;
          }
          separator = buffer.indexOf('\n\n');
        }
      });
      response.on('end', () => {
        closed = true;
        settle(false);
      });
      // A response-level failure is the transport's own and is surfaced, unless
      // this read already ended on its own window (which settles first).
      response.on('error', (error) => refuse(error));
    });
    timer = setTimeout(() => {
      if (settle(true)) request.destroy();
    }, timeoutMs);
    request.on('error', (error) => refuse(error));
    request.end();
  });
}

/**
 * ONE bounded SSE read with a real transport failure reported as its own typed
 * STOP, so a broken connection can never be mistaken for an idle window (the
 * `timed_out` shape an ordinary no-new-frame read returns).
 */
async function readEventStreamOrStop(step, attempt) {
  try {
    return await attempt();
  } catch (error) {
    throw failed(
      'event_stream_transport',
      `${step}: the same-run SSE read failed at the transport level: ${error instanceof Error ? error.message : String(error)}`,
    );
  }
}

/** Require a 2xx SSE answer; anything else is the public refusal it is. */
function requireEventStreamStatus(step, read) {
  if (read.status < 200 || read.status >= 300) {
    if (read.timed_out === true && read.status === 0) {
      throw failed('event_stream_unavailable', `${step}: no SSE response inside the read window`);
    }
    throw statusFailure(step, { status: read.status, json: read.json, text: '' });
  }
  return read;
}

/**
 * The `<UUID epoch>:<decimal sequence>` cursor of one SSE frame id (contract §4:
 * the run-event ring owns it; this driver never renumbers or re-derives it).
 *
 * @returns {{epoch: string, sequence: number}|null} `null` for anything that is
 *   not a cursor — including the empty id the `history_unavailable` control
 *   frame deliberately carries.
 */
function parseEventCursor(id) {
  if (typeof id !== 'string' || id.length === 0) return null;
  const separator = id.lastIndexOf(':');
  if (separator <= 0) return null;
  const epoch = id.slice(0, separator);
  const sequence = id.slice(separator + 1);
  if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(epoch)) return null;
  if (!/^\d+$/.test(sequence)) return null;
  const value = Number.parseInt(sequence, 10);
  return Number.isSafeInteger(value) ? { epoch, sequence: value } : null;
}

/** The bounded `GapWire` payload of one `gap` control frame (contract §4). */
function parseGapFrame(frame) {
  if (frame?.event !== 'gap' || typeof frame.data !== 'string') return null;
  let payload = null;
  try {
    payload = JSON.parse(frame.data);
  } catch {
    return null;
  }
  if (payload === null || typeof payload !== 'object' || Array.isArray(payload)) return null;
  if (typeof payload.run_id !== 'string' || payload.run_id.length === 0) return null;
  if (typeof payload.epoch !== 'string' || payload.epoch.length === 0) return null;
  if (!Number.isInteger(payload.from_sequence) || payload.from_sequence < 0) return null;
  if (!Number.isInteger(payload.to_sequence) || payload.to_sequence < payload.from_sequence) return null;
  return {
    run_id: payload.run_id,
    epoch: payload.epoch,
    from_sequence: payload.from_sequence,
    to_sequence: payload.to_sequence,
  };
}

/** The bounded `HistoryUnavailableWire` payload of one `history_unavailable` control frame. */
function parseHistoryUnavailableFrame(frame) {
  if (frame?.event !== 'history_unavailable' || typeof frame.data !== 'string') return null;
  let payload = null;
  try {
    payload = JSON.parse(frame.data);
  } catch {
    return null;
  }
  if (payload === null || typeof payload !== 'object' || Array.isArray(payload)) return null;
  if (typeof payload.run_id !== 'string' || payload.run_id.length === 0) return null;
  if (typeof payload.inspect_url !== 'string' || payload.inspect_url.length === 0) return null;
  return { run_id: payload.run_id, inspect_url: payload.inspect_url };
}

/**
 * Classify ONE same-run cursor reconnect against the frames this driver already
 * observed (§4: "same epoch, retained cursor → exclusive replay then tail
 * without duplicate handoff"). Exactly two honest outcomes are accepted, and an
 * idle window with no successor frame is NOT one of them:
 *
 *   * `successors` — every successor the cursor promised replayed, each from the
 *     cursor's own epoch and strictly AFTER it (a frame at or before the cursor
 *     would be the duplicate handoff the contract forbids), with no explicit gap
 *     narrowing the record;
 *   * `gap` — the ring answered with an explicit bounded `gap` whose range
 *     covers every successor the cursor promised: recorded verbatim
 *     (run/epoch/from/to) instead of an invented history.
 *
 * Everything else refuses: a replayed frame outside the cursor's epoch, a
 * cursor-less data frame, a `history_unavailable` close for a run whose ring is
 * live, a malformed `gap`, or successors the replay silently dropped.
 *
 * @throws {DriverFailure} `failed`/<replay category> on any other outcome.
 */
function classifySameRunReplay({ runId, cursor, observedIds, read }) {
  const origin = parseEventCursor(cursor);
  if (origin === null) {
    throw failed(
      'replay_unprovable',
      `the observed cursor ${JSON.stringify(cursor)} is not a <UUID epoch>:<decimal sequence> cursor, so no ` +
        'same-run replay can be addressed from it',
    );
  }
  const expected = [];
  for (const id of observedIds) {
    const parsed = parseEventCursor(id);
    if (parsed === null || parsed.epoch !== origin.epoch || parsed.sequence <= origin.sequence) continue;
    if (!expected.includes(id)) expected.push(id);
  }
  if (expected.length === 0) {
    throw failed(
      'replay_unprovable',
      `cursor ${cursor} has no successor among the ${observedIds.length} observed frame(s); a same-run replay proof ` +
        'needs the frames the cursor promised, and an empty read is never evidence that they replayed',
    );
  }
  const sequences = new Map(expected.map((id) => [id, parseEventCursor(id).sequence]));
  const replayed = [];
  const gaps = [];
  for (const frame of read.frames) {
    if (frame.event === 'history_unavailable') {
      throw failed(
        'replay_history_unavailable',
        'the cursor reconnect answered `history_unavailable` for a run whose ring this driver just read; the ' +
          'retained history was not lost and its loss must not be claimed',
      );
    }
    if (frame.event === 'gap') {
      const gap = parseGapFrame(frame);
      if (gap === null) {
        throw failed('replay_contract_violation', 'a `gap` frame did not carry {run_id, epoch, from_sequence, to_sequence}');
      }
      if (gap.run_id !== runId || gap.epoch !== origin.epoch) {
        throw failed(
          'replay_foreign_gap',
          `a \`gap\` frame named run ${JSON.stringify(gap.run_id)} / epoch ${JSON.stringify(gap.epoch)}, not this ` +
            `run's ${JSON.stringify(runId)} / ${JSON.stringify(origin.epoch)}`,
        );
      }
      gaps.push(gap);
      continue;
    }
    const parsed = parseEventCursor(frame.id);
    if (parsed === null) {
      throw failed(
        'replay_contract_violation',
        `a replayed data frame carries no <epoch>:<sequence> cursor: ${JSON.stringify(frame.id)}`,
      );
    }
    if (parsed.epoch !== origin.epoch) {
      throw failed(
        'replay_foreign_epoch',
        `a replayed frame belongs to epoch ${JSON.stringify(parsed.epoch)}, not the cursor's ` +
          `${JSON.stringify(origin.epoch)}`,
      );
    }
    if (parsed.sequence <= origin.sequence) {
      throw failed(
        'replay_duplicate_handoff',
        `replayed frame ${JSON.stringify(frame.id)} is at or before the cursor ${cursor}: the replay must be exclusive`,
      );
    }
    replayed.push({ id: frame.id, event: frame.event ?? null, sequence: parsed.sequence });
  }
  const replayedIds = new Set(replayed.map((frame) => frame.id));
  const missing = expected.filter((id) => !replayedIds.has(id));
  const uncovered = missing.filter(
    (id) => !gaps.some((gap) => gap.from_sequence <= sequences.get(id) && gap.to_sequence >= sequences.get(id)),
  );
  if (uncovered.length > 0) {
    throw failed(
      'replay_incomplete',
      `the cursor reconnect did not replay ${uncovered.length} observed successor frame(s) of ${cursor} ` +
        `(${uncovered.join(', ')}) and no explicit gap covers them`,
    );
  }
  return {
    kind: missing.length === 0 ? 'successors' : 'gap',
    cursor,
    epoch: origin.epoch,
    observed_successors: expected.length,
    replayed_frames: replayed.length,
    replayed_ids: replayed.map((frame) => frame.id),
    new_frames: replayed.filter((frame) => !sequences.has(frame.id)).map((frame) => frame.id),
    missing_successors: missing,
    gaps: gaps.map((gap) => ({ from_sequence: gap.from_sequence, to_sequence: gap.to_sequence })),
    exclusive: true,
    duplicate_handoff: false,
    closed: read.closed === true,
    timed_out: read.timed_out === true,
  };
}

/**
 * Prove the same-run cursor reconnect over the public route: read the frames the
 * earliest observed cursor promised and classify them
 * ({@link classifySameRunReplay}). Fails closed when the stream read carried
 * fewer than two cursor-bearing frames — the run is an admitted, gated, steered
 * and prompted one, so a single-frame read is itself the defect that makes a
 * replay unprovable, never a reason to record an idle window as a pass.
 */
async function proveSameRunReplay({ port, runId, observed }) {
  const observedIds = [];
  for (const frame of observed) {
    if (typeof frame.id === 'string' && frame.id.length > 0) observedIds.push(frame.id);
  }
  if (observedIds.length < 2) {
    throw failed(
      'replay_unprovable',
      `the same-run stream read observed ${observedIds.length} cursor-bearing frame(s); the exclusive successor ` +
        'replay cannot be proved from fewer than two, and an idle window is not a replay proof',
    );
  }
  const cursor = observedIds[0];
  const step = 'GET /orchestration/sessions/{run_id}/events (cursor replay)';
  const read = requireEventStreamStatus(
    step,
    await readEventStreamOrStop(step, () =>
      readEventStream(port, runId, { lastEventId: cursor, maxFrames: MAX_EVENT_FRAMES, timeoutMs: EVENT_REPLAY_TIMEOUT_MS }),
    ),
  );
  return { replay: classifySameRunReplay({ runId, cursor, observedIds, read }), frames: read.frames };
}

/**
 * The union of the observed frame sets, de-duplicated by identity and kept in
 * delivery order (the initial read's frames first, then the cursor reconnect's).
 * The receipt carries only ids and control-frame names — this union exists to be
 * scanned for an admissible workspace-commit response and is never printed.
 */
function mergeObservedFrames(observed, replayed) {
  const merged = [];
  const seen = new Set();
  for (const frame of [...observed, ...replayed]) {
    const key = `${frame.id ?? ''}|${frame.event ?? ''}|${frame.data ?? ''}`;
    if (seen.has(key)) continue;
    seen.add(key);
    merged.push(frame);
  }
  return merged;
}

/**
 * The public refusal controls of the same-run stream (contract §4). Every case
 * is asserted against its exact wire answer — never tolerated approximately —
 * and recorded verbatim (status, code, frame count), so the receipt shows what
 * the route actually refused:
 *
 *   * an ABSENT run is `404 not_found` before any SSE header (the ownership
 *     check runs before any ring/epoch/cursor lookup, so no existence leaks);
 *   * a MALFORMED cursor and a FUTURE cursor are `400 invalid_input` before any
 *     SSE header;
 *   * a cursor from ANOTHER epoch against a live run is the one
 *     `history_unavailable` control frame with an empty id (the caller's
 *     `Last-Event-ID` is never advanced to a sequence that was never retained),
 *     naming this run and its inspect URL, and the stream closes with it —
 *     no invented history, no data frame.
 *
 * @throws {DriverFailure} `failed`/`refusal_contract_violation` (or the read's
 *   own typed transport STOP) on any deviation.
 */
async function collectStreamRefusals({ port, runId, observedIds }) {
  const origin = parseEventCursor(observedIds[0]);
  if (origin === null) {
    throw failed('refusal_unprovable', `the observed cursor ${JSON.stringify(observedIds[0])} is not a cursor`);
  }
  let maxSequence = origin.sequence;
  for (const id of observedIds) {
    const parsed = parseEventCursor(id);
    if (parsed !== null && parsed.epoch === origin.epoch) maxSequence = Math.max(maxSequence, parsed.sequence);
  }
  const cases = [
    { control: 'absent_run', run: 'public-first-workflow-absent-run', expectStatus: 404, expectCode: 'not_found' },
    { control: 'malformed_cursor', run: runId, cursor: 'not-a-cursor', expectStatus: 400, expectCode: 'invalid_input' },
    {
      control: 'future_cursor',
      run: runId,
      cursor: `${origin.epoch}:${maxSequence + 1000}`,
      expectStatus: 400,
      expectCode: 'invalid_input',
    },
  ];
  const recorded = [];
  for (const entry of cases) {
    const step = `GET /orchestration/sessions/{run_id}/events (${entry.control})`;
    const read = await readEventStreamOrStop(step, () =>
      readEventStream(port, entry.run, {
        lastEventId: entry.cursor,
        maxFrames: 4,
        timeoutMs: EVENT_REPLAY_TIMEOUT_MS,
      }),
    );
    const code = read.json?.error?.code ?? read.json?.code ?? null;
    if (read.status !== entry.expectStatus || code !== entry.expectCode) {
      throw failed(
        'refusal_contract_violation',
        `${entry.control}: expected HTTP ${entry.expectStatus} (${entry.expectCode}) before any SSE header, got HTTP ` +
          `${read.status}${code ? ` (${code})` : ''}`,
      );
    }
    if (read.frames.length > 0) {
      throw failed(
        'refusal_contract_violation',
        `${entry.control}: a refused subscription answered ${read.frames.length} SSE frame(s)`,
      );
    }
    recorded.push({
      control: entry.control,
      status: read.status,
      code,
      frames: read.frames.length,
      closed: read.closed === true,
      timed_out: read.timed_out === true,
    });
  }
  // A cursor whose epoch is not the live ring's: the run IS owned, so this is
  // not a refusal — it is the explicit history-loss control frame, and reading
  // it as a short stream or a 400 would both be wrong.
  const historyStep = 'GET /orchestration/sessions/{run_id}/events (prior epoch cursor)';
  const historyCursor = `00000000-0000-4000-8000-000000000000:${maxSequence}`;
  const historyRead = requireEventStreamStatus(
    historyStep,
    await readEventStreamOrStop(historyStep, () =>
      readEventStream(port, runId, {
        lastEventId: historyCursor,
        maxFrames: 4,
        timeoutMs: EVENT_REPLAY_TIMEOUT_MS,
      }),
    ),
  );
  if (historyRead.frames.length !== 1 || historyRead.frames[0].event !== 'history_unavailable') {
    throw failed(
      'refusal_contract_violation',
      `prior_epoch_cursor: expected exactly one history_unavailable control frame, got ` +
        `${JSON.stringify(historyRead.frames.map((frame) => frame.event))}`,
    );
  }
  const wire = parseHistoryUnavailableFrame(historyRead.frames[0]);
  if (wire === null || wire.run_id !== runId) {
    throw failed(
      'refusal_contract_violation',
      `prior_epoch_cursor: the control frame did not name this run (${JSON.stringify(wire?.run_id ?? null)}) with an ` +
        'inspect URL',
    );
  }
  if (historyRead.frames[0].id !== null && historyRead.frames[0].id !== '') {
    throw failed(
      'refusal_contract_violation',
      `prior_epoch_cursor: the control frame must carry no cursor, got ${JSON.stringify(historyRead.frames[0].id)}`,
    );
  }
  if (historyRead.closed !== true || historyRead.timed_out === true) {
    throw failed(
      'refusal_contract_violation',
      'prior_epoch_cursor: the stream must close with the history_unavailable control frame rather than idle until ' +
        'the read window elapses',
    );
  }
  recorded.push({
    control: 'prior_epoch_cursor',
    status: historyRead.status,
    code: null,
    frames: historyRead.frames.length,
    control_frame: 'history_unavailable',
    inspect_url: wire.inspect_url,
    cursor_present: false,
    closed: true,
    timed_out: false,
  });
  return recorded;
}

/**
 * ONE cursor reconnect against a run whose retained history is gone (the
 * post-restart case, contract §4: "retained run, prior epoch or evicted ring /
 * restart → one `history_unavailable` control frame with the same run id and
 * inspect URL, then close; no invented historical events").
 *
 * The answer is asserted, not merely observed: exactly that one control frame,
 * carrying THIS run id and an inspect URL, with NO cursor and no data frame,
 * and the stream closed by the server inside the window. A retained history
 * replayed as if nothing happened is as much a failure as an idle read.
 */
function assertHistoryLossExplicit({ runId, cursor, read }) {
  const frames = read.frames;
  if (frames.length !== 1 || frames[0].event !== 'history_unavailable') {
    throw failed(
      'history_loss_not_explicit',
      `${frames.length} frame(s) ${JSON.stringify(frames.map((frame) => frame.event))} answered the pre-restart ` +
        `cursor ${cursor}; the lost retained history must be stated as one history_unavailable control frame`,
    );
  }
  const wire = parseHistoryUnavailableFrame(frames[0]);
  if (wire === null || wire.run_id !== runId) {
    throw failed(
      'history_loss_not_explicit',
      `the history_unavailable control frame did not name this run (${JSON.stringify(wire?.run_id ?? null)}) with an ` +
        'inspect URL',
    );
  }
  if (frames[0].id !== null && frames[0].id !== '') {
    throw failed(
      'history_loss_not_explicit',
      `the history_unavailable control frame must carry no cursor, got ${JSON.stringify(frames[0].id)}`,
    );
  }
  if (read.timed_out === true && read.closed !== true) {
    throw failed(
      'history_loss_not_explicit',
      'the history_unavailable stream idled until the read window elapsed instead of closing with its control frame',
    );
  }
  return {
    control_frame: 'history_unavailable',
    run_id: wire.run_id,
    inspect_url: wire.inspect_url,
    cursor_present: false,
    data_frames: 0,
    closed: read.closed === true,
    timed_out: read.timed_out === true,
  };
}

/**
 * Frame `event:` identities that carry a `CoreWorkspaceCommitResponse` — the
 * only admissible source of the receipt's commit revision (§6.1).
 *
 * Empty **by measurement, not as a placeholder**. `RunEventRegistry`
 * (`crates/nexus-core/src/execution/run_events.rs`) publishes exactly three
 * frame identities — `run_state`, `host_event` and `gap` — plus the
 * `history_unavailable` subscription refusal; `HostEvent`
 * (`crates/nexus-agent-host/src/capability/model.rs`) has no
 * commit/capability-output variant; and the `workspace.commit` capability's
 * `CoreWorkspaceCommitResponse` is returned in-process
 * (`crates/nexus-core/src/execution/workspace.rs`). No producer routes that
 * response onto a run ring, so no frame of the current vocabulary is evidence
 * of a commit. Wiring that producer (P3-T2) adds its identity here; loosening
 * the matcher below is never the fix.
 */
const COMMIT_RESPONSE_EVENT_IDENTITIES = new Set();

/**
 * The workspace-commit revision carried by a routed same-run **workspace-commit
 * response** frame; `null` when no such frame exists. Never invented and never
 * derived from frame text or from the fixture bytes.
 *
 * Admission rules (the accepting branch lands with the P3-T2 producer, together
 * with its own evidence):
 *
 *   * `event:` must be an identity in {@link COMMIT_RESPONSE_EVENT_IDENTITIES}
 *     — an unrelated `host_event`/`run_state`/`gap`/`history_unavailable` frame
 *     is never a commit response, so its payload is never scanned;
 *   * `data:` must parse as the canonical response object
 *     (`schemas/core/core-workspace-commit-response.schema.json`,
 *     `additionalProperties: false`) with exactly `revision` (a `rev_<id>`
 *     identifier) and `committed: true`; a nested or double-encoded string, an
 *     extra field and an unsuccessful commit are all inadmissible.
 *
 * Until that producer exists the answer is always `null`, the effect stays
 * refused as `missing_commit_revision`, and no frame text substitutes for it.
 */
function findCommitRevision(frames) {
  for (const frame of frames) {
    if (!COMMIT_RESPONSE_EVENT_IDENTITIES.has(frame?.event)) continue;
    return parseCommitResponseRevision(frame);
  }
  return null;
}

/**
 * Payload predicate for the admission rules above. Kept beside the gate so the
 * P3-T2 producer change only adds its identity; the branch is unreachable while
 * no identity is admissible, so it is asserted against the schema directly
 * rather than through a fabricated frame.
 *
 * @returns {string|null} the canonical revision, or `null` when the frame
 *   payload is not a successful workspace-commit response.
 */
function parseCommitResponseRevision(frame) {
  if (typeof frame?.data !== 'string') return null;
  let payload = null;
  try {
    payload = JSON.parse(frame.data);
  } catch {
    return null;
  }
  if (payload === null || typeof payload !== 'object' || Array.isArray(payload)) return null;
  const keys = Object.keys(payload);
  if (keys.length !== 2 || !keys.includes('revision') || !keys.includes('committed')) return null;
  if (payload.committed !== true) return null;
  if (typeof payload.revision !== 'string' || !COMMIT_REVISION_PATTERN.test(payload.revision)) return null;
  return payload.revision;
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
 * The workspace-commit revision carried by the AUTHORIZED root session detail.
 *
 * P1-T2 extends `GET /v1/daemon/orchestration/sessions/{run_id}` with an
 * optional top-level `workspace_commit: {revision: string, committed: true}`
 * (`schemas/daemon-api/orchestration/sessions/session-detail-response.schema.json`,
 * `additionalProperties: false`; contract §4). It is the durable projection of
 * THIS run's own checkpointed `workspace.commit` capability output, read on the
 * already-authorized root row, so it is the canonical commit-revision proof the
 * P3 first workflow consumes — an existing authorized detail read, not an
 * invented event frame.
 *
 * The shape is enforced here, not assumed: the member must be an object whose
 * keys are exactly `revision` and `committed`, with `committed === true` and a
 * `rev_<id>` revision (the identifier the durable commit authority mints). An
 * absent, malformed, failed, extra-member, empty or non-`rev_` value yields
 * `null`, so a missing/fake/foreign result can never reach the receipt — and
 * the caller has already bound the detail to the same root run
 * (`readDurableObservation`), which is the only foreign-run guard such a value
 * can have: the DTO carries no run id of its own.
 *
 * @returns {string|null} the revision, or `null` when the detail does not
 *   project a canonical successful workspace commit.
 */
function parseSessionWorkspaceCommit(detail) {
  const commit = detail?.workspace_commit;
  if (commit === null || typeof commit !== 'object' || Array.isArray(commit)) return null;
  const keys = Object.keys(commit);
  if (keys.length !== 2 || !keys.includes('revision') || !keys.includes('committed')) return null;
  if (commit.committed !== true) return null;
  if (typeof commit.revision !== 'string' || !COMMIT_REVISION_PATTERN.test(commit.revision)) return null;
  return commit.revision;
}

/**
 * Resolve the revision the receipt must carry (Task 1 requires the committed
 * file *and* its commit revision; §6.1 requires the commit revision in the
 * receipt).
 *
 * Two strictly validated, same-run sources are admissible, in this order:
 *
 *   1. the authorized root session detail's projected `workspace_commit`
 *      ({@link parseSessionWorkspaceCommit}) — the P1-T2 producer of §4;
 *   2. a routed same-run frame identified as a workspace-commit response whose
 *      parsed payload is the canonical object ({@link findCommitRevision}) —
 *      admissible identities are empty by measurement today.
 *
 * Nothing is computed from the fixture bytes, nothing is read out of unrelated
 * frame text, no identifier is invented, and the revision is never inferred
 * from run state: the durable `RunStateWire.state_revision` (routed frames or
 * the inspect projection) proves run state, not the committed workspace, so it
 * is returned as an explicitly labeled informational field and NEVER
 * substituted. A missing commit revision is refused here — it is never
 * represented as an acceptable result — so a caller cannot record the effect as
 * a success.
 *
 * @throws {DriverFailure} `failed`/`missing_commit_revision` when neither
 *   source carries a commit revision for this run.
 */
function resolveEffectRevision({
  commitRevision,
  sessionCommitRevision = null,
  frames,
  projectionStateRevision,
}) {
  const routed =
    typeof commitRevision === 'string' && COMMIT_REVISION_PATTERN.test(commitRevision) ? commitRevision : null;
  const projectedCommit =
    typeof sessionCommitRevision === 'string' && COMMIT_REVISION_PATTERN.test(sessionCommitRevision)
      ? sessionCommitRevision
      : null;
  const commit = projectedCommit ?? routed;
  const revisionSource =
    projectedCommit !== null ? 'session-detail-workspace-commit' : routed !== null ? 'run-event-stream-commit-revision' : null;
  const streamed = findRunStateRevision(frames);
  const projected =
    Number.isInteger(projectionStateRevision) && projectionStateRevision >= 0 ? projectionStateRevision : null;
  const durableStateRevision = streamed ?? projected;
  if (commit === null) {
    throw failed(
      'missing_commit_revision',
      'the declared workspace effect landed, but neither the authorized root session detail projected a ' +
        'workspace-commit revision (no canonical {revision: rev_<id>, committed: true} member) nor was a routed ' +
        'same-run frame an identifiable successful workspace-commit response; ' +
        `durable run-state revision ${durableStateRevision === null ? 'none observed' : durableStateRevision} ` +
        'proves run state, not the committed workspace and is never substituted; §6.1 and the P3-T1 card require ' +
        'the committed file and its commit revision, so the effect is not a success',
    );
  }
  return {
    commit_revision: commit,
    durable_state_revision: durableStateRevision,
    revision_source: revisionSource,
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
      prompt_renders_core_context: fixture.promptTemplate.includes(CORE_CONTEXT_TEMPLATE),
      scope: fixture.scopePath,
      change_path: fixture.changePath,
      declared_bytes: fixture.declaredBytes.length,
      declared_sha256: fixture.declaredSha256,
      gate_state: fixture.gate.stateId,
      gate_timeout_ms: fixture.gate.timeoutMs,
      gate_effect_state: fixture.gate.effectStateId,
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
      await httpJson(port, 'PATCH', `/v1/daemon/presets/${encodePathSegment(fixture.presetId)}`, {
        body: { yaml: fixture.yaml },
      }),
    );
    requireFields('PATCH /presets/{id}', patched, ['id', 'updated']);
    if (patched.updated !== true) throw failed('contract_violation', 'preset PATCH did not report updated=true');
    // `POST /presets` answers with the preset BUNDLE directory
    // (`scaffold_user_preset` returns `bundle.display()`), while
    // `POST /presets:validate` takes the preset YAML FILE it reads and parses
    // (`validate_preset_file` rejects a directory with an untyped
    // internal/FILE_READ_ERROR, i.e. HTTP 500). The driver therefore validates
    // the very file the PATCH above wrote, derived with the retained bundle
    // convention (`preset.yaml`), and refuses locally if the bundle it was told
    // about does not contain it.
    const presetFile = presetYamlPath(scaffold.path);
    if (!existsSync(presetFile)) {
      throw failed(
        'contract_violation',
        `the scaffolded preset bundle ${JSON.stringify(scaffold.path)} does not contain ` +
          `${JSON.stringify(basenameOf(presetFile))}, so the authored fixture cannot be validated`,
      );
    }
    const validated = requireOk(
      'POST /presets:validate',
      await httpJson(port, 'POST', '/v1/daemon/presets:validate', { body: { path: presetFile } }),
    );
    requireFields('POST /presets:validate', validated, ['valid', 'errors']);
    if (validated.valid !== true) {
      throw failed('contract_violation', `preset validation refused the fixture: ${JSON.stringify(validated.errors)}`);
    }
    if (validated.id !== fixture.presetId) {
      throw failed(
        'contract_violation',
        `preset validation answered id ${JSON.stringify(validated.id)} for the authored fixture ` +
          `${JSON.stringify(fixture.presetId)}`,
      );
    }
    if (validated.state_count !== fixture.stateCount) {
      throw failed(
        'contract_violation',
        `preset validation counted ${JSON.stringify(validated.state_count)} states for a fixture the driver reads as ` +
          `${fixture.stateCount}; the validated artifact is not the checked-in graph`,
      );
    }
    facts.preset = {
      id: scaffold.id,
      bundle_path: scaffold.path,
      validated_file: presetFile,
      validated: true,
      state_count: validated.state_count ?? null,
      warnings: validated.warnings ?? [],
    };
    record('preset_authoring', 'ok', { validated: true, state_count: validated.state_count ?? null });

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
    // The A7 placement is read from the public daemon-free operator surface for
    // THIS root run (the W4 detail above resolved its identity); every placement
    // read below is bound to the same id and fails closed on a refused CLI, a
    // non-JSON answer, or a DTO that does not prove the run.
    const readPlacement = (targetRunId, step, options) =>
      readPlacementViaCli(cliBinary, childEnv, targetRunId, step, options);
    facts.inspect = {
      status: admission.summary.status,
      current_core_context_version: admission.summary.current_core_context_version ?? null,
      current_session_id: runId,
      execution_policy: admission.summary.execution_policy ?? null,
      admission_polls: admission.polls,
      placement_source: 'nexus42 ops inspect <run> --json',
    };
    record('inspect', 'ok', { schedule_status: facts.inspect.status, admission_polls: admission.polls });

    // 9. W5/W6 on the fixture's bounded pre-effect gate (§6.1: W5/W6 are
    // exercised before the final manual wait; S0-3: the append is durable
    // before resume counts as success). The gate is the state the checked-in
    // fixture declares — its `converge:`/`timeout_ms` state, whose `on_timeout`
    // reroute must be the effect state. Placement is confirmed on the public A7
    // record (`recovery_class`/`current_task_id`/`state_revision`) of the SAME
    // root run the W4 read resolved; a durable human wait (where the A4 fence
    // refuses a plain resume) or any other placement is a typed STOP, never a
    // bypassed wait and never a Steer claimed on a timing assumption.
    const gateBoundary = await awaitPreEffectGate({ readPlacement, runId, gate: fixture.gate, model });
    // Nobody drives a parked gate: the deadline is only evaluated by a driver
    // (`join_timeout_tick`), so waiting it out HERE, with no signal issued, is
    // what makes the W6 resume deterministic instead of a race with the
    // deadline. `timeout_ms` comes from the fixture; the margin is the driver's
    // own, not the fixture's.
    const deadlineAtMs = gateBoundary.parked_at_ms + fixture.gate.timeoutMs + GATE_DEADLINE_MARGIN_MS;
    const deadlineWaitedMs = Math.max(0, deadlineAtMs - Date.now());
    if (deadlineWaitedMs > 0) await sleep(deadlineWaitedMs);
    // The gate must still be the same quiet park on the same run, and still
    // pre-effect: crossing the deadline alone performs no work. Both the W4
    // identity read (same schedule/root) and the public placement read are
    // re-taken.
    await readDurableObservation(port, scheduleId, runId, 'gate after deadline');
    const afterDeadlineBoundary = classifyPreEffectGate(
      await readPlacement(runId, 'gate after deadline'),
      fixture.gate.stateId,
    );
    if (afterDeadlineBoundary.state !== 'gate') {
      throw preEffectGateStop('gate after deadline', afterDeadlineBoundary, fixture.gate.stateId);
    }
    if (model.observations.requests !== 0) {
      throw failed(
        'gate_not_pre_effect',
        `gate after deadline: ${model.observations.requests} model request(s) were dispatched across the gate ` +
          'deadline; the fixture gate must stay pre-effect until the W6 resume',
      );
    }
    facts.steer = {
      gate_state: fixture.gate.stateId,
      gate_effect_state: fixture.gate.effectStateId,
      gate_timeout_ms: fixture.gate.timeoutMs,
      gate_polls: gateBoundary.polls,
      gate_quiet_polls: gateBoundary.quiet_polls,
      gate_observed: gateBoundary.observed,
      deadline_waited_ms: deadlineWaitedMs,
      after_deadline: afterDeadlineBoundary.observed,
      pre_effect_requests: model.observations.requests,
      appended_version: null,
      recheck: null,
      resumed: false,
      resume: null,
      post_resume_boundary: null,
      run_identity: null,
    };
    const appendResponse = requireOk(
      'PATCH /orchestration/schedules/{id}/core-context',
      await httpJson(port, 'PATCH', `/v1/daemon/orchestration/schedules/${encodePathSegment(scheduleId)}/core-context`, {
        body: { op: 'append', body: STEER_IDEA },
      }),
    );
    requireFields('PATCH /orchestration/schedules/{id}/core-context', appendResponse, ['new_version']);
    facts.steer.appended_version = appendResponse.new_version;
    // Re-read the W4 identity and the public placement before resuming: if the
    // run moved on between the durable append and the resume, a plain resume
    // would land where it is fenced. The Steer then stops with the exact
    // observed state; the appended version stays durable and is never
    // re-appended (S0-3).
    await readDurableObservation(port, scheduleId, runId, 'gate recheck');
    const gateRecheck = classifyPreEffectGate(
      await readPlacement(runId, 'gate recheck'),
      fixture.gate.stateId,
    );
    facts.steer.recheck = gateRecheck.observed;
    if (gateRecheck.state !== 'gate') {
      const stop = preEffectGateStop('gate recheck', gateRecheck, fixture.gate.stateId);
      throw new DriverFailure(
        stop.outcome,
        'gate_lost',
        `${stop.message} — the appended core-context version ${JSON.stringify(appendResponse.new_version)} remains ` +
          'durable; resume was not issued and the append is never retried',
      );
    }
    // W6 resumes THIS schedule, whose admitted run is the gated run: the
    // resume re-drives that run and its deadline reroute enters the effect
    // state. It is not a new admission and not a manual-wait bypass.
    const resumeResponse = await httpJson(
      port,
      'POST',
      `/v1/daemon/orchestration/schedules/${encodePathSegment(scheduleId)}/signal`,
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
    // The reroute must reach the effect state's manual wait on the SAME run:
    // that is the observable proof that the deadline reroute ran, that the same
    // schedule/root was re-driven, and that W5/W6 were exercised strictly
    // before the final manual wait.
    const effectBoundary = await awaitEffectBoundary({
      readPlacement,
      runId,
      effectStateId: fixture.gate.effectStateId,
    });
    facts.steer.post_resume_boundary = effectBoundary.observed;
    // The resume must not have minted a second run: the Creator's durable page
    // still carries exactly this preset's ONE run, and it is the gated run.
    const sessionsPage = requireOk(
      'GET /orchestration/sessions',
      await httpJson(port, 'GET', '/v1/daemon/orchestration/sessions'),
    );
    const singleRun = assertSingleRun(sessionsPage.items, fixture.presetId, runId);
    facts.steer.run_identity = {
      run_id: runId,
      preset_runs: singleRun.runs,
      same_run: true,
    };
    record('steer', 'ok', {
      gate_state: fixture.gate.stateId,
      gate_polls: gateBoundary.polls,
      deadline_waited_ms: deadlineWaitedMs,
      appended_version: appendResponse.new_version,
      post_resume_state: effectBoundary.observed.task_id,
    });

    // 10. O1/O2: same-run stream against the inspected root session. The read
    // is bounded by this driver's own window, so `timed_out` (nothing new to
    // send while the run stays live) and a server-side close are distinct
    // recorded facts, and a real transport failure rejects instead of being
    // read as a short stream.
    const streamStep = 'GET /orchestration/sessions/{run_id}/events';
    const stream = requireEventStreamStatus(
      streamStep,
      await readEventStreamOrStop(streamStep, () => readEventStream(port, runId)),
    );
    const observedIds = stream.frames.map((frame) => frame.id).filter((id) => typeof id === 'string' && id.length > 0);
    const lastEventId = observedIds.at(-1) ?? null;
    const streamEpoch = parseEventCursor(observedIds[0] ?? null)?.epoch ?? null;
    facts.stream = {
      frame_count: stream.frames.length,
      ids: observedIds,
      epoch: streamEpoch,
      last_event_id: lastEventId,
      closed: stream.closed,
      timed_out: stream.timed_out === true,
      control_frames: stream.frames
        .filter((frame) => ['gap', 'history_unavailable'].includes(frame.event))
        .map((frame) => frame.event),
      replay: null,
      refusals: null,
      commit_frame_producer: null,
    };
    record('stream', 'ok', {
      frames: stream.frames.length,
      closed: stream.closed,
      timed_out: stream.timed_out === true,
    });

    // 11. O2 same-run cursor reconnect: the exclusive successor replay the
    // cursor contract promises, from the EARLIEST observed cursor of THIS run
    // (a read that merely idles out its window is never a replay proof), or the
    // explicit bounded gap the ring answers with when it cannot replay. The one
    // reconnect also appends to the observed frame set, so a routed
    // workspace-commit response — the SECONDARY revision source; the primary is
    // the authorized root session detail read in step 12 — is still scanned
    // without a second reconnect and without waiting on a frame identity the
    // current run ring cannot route (`COMMIT_RESPONSE_EVENT_IDENTITIES` is
    // empty by measurement in `crates/nexus-core/src/execution/run_events.rs`).
    const { replay, frames: replayedFrames } = await proveSameRunReplay({ port, runId, observed: stream.frames });
    facts.stream.replay = replay;
    record('stream_replay', replay.kind === 'successors' ? 'ok' : 'history_gap', {
      cursor: replay.cursor,
      replayed: replay.replayed_frames,
      expected: replay.observed_successors,
      new_frames: replay.new_frames.length,
      gaps: replay.gaps.length,
    });
    const allFrames = mergeObservedFrames(stream.frames, replayedFrames);
    const observedRevision = findCommitRevision(allFrames);
    facts.stream.commit_frame_producer = {
      admissible_event_identities: [...COMMIT_RESPONSE_EVENT_IDENTITIES],
      routed_commit_frames: observedRevision === null ? 0 : 1,
      revision_source: 'authorized-root-session-detail',
    };
    facts.stream.replay_added_frames = allFrames.length - stream.frames.length;

    // 11b. Public refusal controls of the same-run stream (§4): an absent run
    // is refused before any SSE header, a malformed and a future cursor are
    // typed invalid-input refusals before any SSE header, and a cursor from
    // another epoch against a live run is the explicit history-loss control
    // frame. Each is asserted against its exact wire answer.
    const refusals = await collectStreamRefusals({ port, runId, observedIds });
    facts.stream.refusals = refusals;
    record('stream_refusals', 'ok', { controls: refusals.map((entry) => entry.control) });

    // 12. §6.1: the declared workspace effect, read back byte-for-byte. The
    // effect is the FIRST real prompt of the run crossing into the effect
    // state, so its cardinality is asserted here: exactly one sealed prompt,
    // carrying the appended Idea, and no second dispatch. Its sealed no-tools
    // policy is asserted on the same real request, and the isolated root is
    // checked for unsolicited tool side effects.
    const sealedPolicy = assertSealedToolPolicy(model.observations);
    assertSealedPromptCardinality(model.observations);
    const markers = assertNoHostileMarkers(receipt.isolated_root);
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
    const scopeInventory = assertScopeEffectOnly(scopeDir, fixture);
    facts.sealed_denial = {
      network: 'loopback-only',
      prompt_tool_policy: fixture.promptToolPolicy,
      driven_unsolicited_tool_calls: 0,
      driven_attempt_owner: 'real_dsh_sealed_deny_all_rejects_unsolicited_tools_without_side_effects',
      sealed_policy: sealedPolicy,
      hostile_markers: markers,
      scope_entries: scopeInventory.map((entry) => entry.path),
      side_effect_marker: null,
    };
    record('sealed_denial', 'ok', {
      advertised_tools: sealedPolicy.advertised_tools,
      hostile_markers: markers.present.length,
      scope_entries: scopeInventory.length,
    });
    // The receipt must carry the **commit** revision of this run: the
    // authorized root session detail's projected `workspace_commit` (contract
    // §4, P1-T2), strictly parsed here, or — if that projection is absent — a
    // routed same-run frame carrying the canonical workspace-commit response.
    // The read is a root-run read: `readDurableObservation` already refused any
    // detail that is not the synchronized run, which is the only foreign-result
    // guard such a projection can have (the DTO carries no run id of its own).
    // The durable run-state revision is recorded beside the commit revision as
    // information only and never substitutes: it proves run state, not the
    // committed workspace. Nothing is computed from the fixture bytes and no
    // identifier is invented; a missing/malformed/failed/foreign revision
    // refuses the effect (missing_commit_revision), never a success.
    const effectObservation = await readDurableObservation(port, scheduleId, runId, 'effect');
    const placementAtEffect = await readPlacement(runId, 'effect');
    const sessionCommitRevision = parseSessionWorkspaceCommit(effectObservation.detail);
    const detailWorkspaceCommit = effectObservation.detail?.workspace_commit ?? null;
    facts.effect = {
      relative_path: `${fixture.scopePath}/${fixture.changePath}`,
      bytes: landed.length,
      sha256: sha256(landed),
      declared_content_matches: true,
      prompt_requests: model.observations.requests,
      prompt_with_idea: model.observations.prompt_with_idea,
      session_detail_workspace_commit: detailWorkspaceCommit,
      session_commit_revision_accepted: sessionCommitRevision,
      commit_revision: null,
      durable_state_revision: null,
      revision_source: null,
    };
    Object.assign(
      facts.effect,
      resolveEffectRevision({
        commitRevision: observedRevision,
        sessionCommitRevision,
        frames: allFrames,
        // The `state_revision` beside the commit revision is the informational
        // run-state value from the same placement surface, never a substitute.
        projectionStateRevision: placementAtEffect?.state_revision ?? null,
      }),
    );
    record('workspace_effect', 'ok', {
      sha256: facts.effect.sha256,
      commit_revision: facts.effect.commit_revision,
      durable_state_revision: facts.effect.durable_state_revision,
      revision_source: facts.effect.revision_source,
    });

    // 13. W7 cancel: a durable `cancelled` is the only success, and the cancel
    // must not have committed anything: the authorized root detail still
    // projects the SAME commit revision and the opened scope is byte-identical.
    requireOk(
      'POST /orchestration/schedules/{id}/signal (cancel)',
      await httpJson(port, 'POST', `/v1/daemon/orchestration/schedules/${encodePathSegment(scheduleId)}/signal`, {
        body: { signal: 'cancel' },
      }),
    );
    const cancelObservation = await readDurableObservation(port, scheduleId, runId, 'cancel');
    if (cancelObservation.summary.status !== 'cancelled') {
      throw failed(
        'contract_violation',
        `cancel did not settle durably: inspect reports ${JSON.stringify(cancelObservation.summary.status)}`,
      );
    }
    const cancelCommitRevision = parseSessionWorkspaceCommit(cancelObservation.detail);
    if (cancelCommitRevision !== facts.effect.commit_revision) {
      throw failed(
        'duplicate_effect',
        `the cancelled run projects workspace-commit revision ${JSON.stringify(cancelCommitRevision)} instead of the ` +
          `observed effect revision ${JSON.stringify(facts.effect.commit_revision)}; cancellation must not commit again`,
      );
    }
    const scopeAfterCancel = assertScopeEffectOnly(scopeDir, fixture);
    facts.cancel = {
      status: cancelObservation.summary.status,
      schedule_id: scheduleId,
      commit_revision: cancelCommitRevision,
      scope_entries: scopeAfterCancel.map((entry) => entry.path),
    };
    record('cancel', 'ok', { schedule_status: facts.cancel.status });

    // 14. O3 restart: same home, same schedule/session, preserved effect, no
    // repeated effect, and the LOST retained history stated explicitly. Every
    // half is asserted: the restarted producer must answer the pre-restart
    // cursor with exactly the one `history_unavailable` control frame (no
    // invented history, no silent empty stream), the same root run must still
    // own exactly this preset's ONE run, the effect bytes and ITS commit
    // revision must be identical, and the scope must carry nothing new.
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
        await httpJson(restartPort, 'GET', `/v1/daemon/orchestration/schedules/${encodePathSegment(scheduleId)}`),
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
    const restartObservation = await readDurableObservation(restartPort, scheduleId, runId, 'restart');
    const restartCommitRevision = parseSessionWorkspaceCommit(restartObservation.detail);
    if (restartCommitRevision !== facts.effect.commit_revision) {
      throw failed(
        'duplicate_effect',
        `the restarted run projects workspace-commit revision ${JSON.stringify(restartCommitRevision)} instead of the ` +
          `observed effect revision ${JSON.stringify(facts.effect.commit_revision)}; the restart must not repeat the effect`,
      );
    }
    const scopeAfterRestart = assertScopeEffectOnly(scopeDir, fixture);
    const restartSessions = requireOk(
      'GET /orchestration/sessions (restart)',
      await httpJson(restartPort, 'GET', '/v1/daemon/orchestration/sessions'),
    );
    const restartRuns = assertSingleRun(restartSessions.items, fixture.presetId, runId);
    const historyStep = 'GET /orchestration/sessions/{run_id}/events (restart)';
    const historyLoss = assertHistoryLossExplicit({
      runId,
      cursor: lastEventId,
      read: await readEventStreamOrStop(historyStep, () =>
        readEventStream(restartPort, runId, { lastEventId, maxFrames: 16, timeoutMs: EVENT_REPLAY_TIMEOUT_MS }),
      ),
    });
    facts.restart = {
      schedule_status: afterRestart.status,
      current_session_id: runId,
      effect_sha256: sha256(landedAfterRestart),
      effect_preserved: true,
      commit_revision: restartCommitRevision,
      commit_revision_repeated: false,
      scope_entries: scopeAfterRestart.map((entry) => entry.path),
      preset_runs: restartRuns.runs,
      history_loss: historyLoss,
    };
    record('restart', 'ok', {
      schedule_status: afterRestart.status,
      commit_revision: restartCommitRevision,
      history_loss: historyLoss.control_frame,
    });

    facts.model_endpoint = {
      port: model.port,
      requests: model.observations.requests,
      paths: [...new Set(model.observations.paths)],
      unexpected_requests: model.observations.unexpected,
      authorization_header_present: model.observations.authorization_header_present,
      prompt_with_idea: model.observations.prompt_with_idea,
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
      // Retain the service's own post-ready stdout (request/SSE diagnostics)
      // before the child is stopped, so a stream symptom is diagnosable.
      running.evidence?.writeStdoutTail?.();
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
 * directly, so a focused authoring check can exercise the fixture extraction
 * (including the bounded pre-effect gate facts), the child-environment
 * isolation rule, the controlled model protocol and its request structure, the
 * durable admission/boundary classification, the wire refusal classification,
 * the cursor/frame classification of the same-run replay and refusal controls,
 * the bounded SSE read, the side-effect inventory, the revision resolution, the
 * bounded server close and the cleanup disposition without starting any
 * service.
 */
export {
  applyCleanupDisposition,
  assertHistoryLossExplicit,
  assertNoHostileMarkers,
  assertScopeEffectOnly,
  assertSealedPromptCardinality,
  assertSealedToolPolicy,
  assertSingleRun,
  awaitEffectBoundary,
  awaitPreEffectGate,
  awaitRunIdentity,
  boundedObservation,
  boundedServerClose,
  buildChildEnv,
  classifyAdmissionObservation,
  classifyManualWaitBoundary,
  classifyPreEffectGate,
  classifySameRunReplay,
  collectStreamRefusals,
  directoryInventory,
  encodePathSegment,
  hostileMarkersPresent,
  mergeObservedFrames,
  modelRequestStructure,
  parseEventCursor,
  parseGapFrame,
  parseHistoryUnavailableFrame,
  parsePlacementDto,
  proveSameRunReplay,
  readEventStream,
  readEventStreamOrStop,
  readPlacementViaCli,
  requireEventStreamStatus,
  exitCodeFor,
  findCommitRevision,
  parseCommitResponseRevision,
  parseSessionWorkspaceCommit,
  preEffectGateStop,
  presetYamlPath,
  readDurableObservation,
  readFixture,
  resolveEffectRevision,
  resolveExecutable,
  startModelEndpoint,
  statusFailure,
  summarizeChildEnv,
};

if (process.argv[1] !== undefined && resolve(process.argv[1]) === SCRIPT_PATH) {
  await main();
}
