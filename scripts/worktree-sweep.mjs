#!/usr/bin/env node
/**
 * Repository worktree and shared-cache sweeper — read-only inventory, capacity and
 * guard/check half of the G2 contract.
 *
 *   node scripts/worktree-sweep.mjs \
 *     --repo <absolute-main-root> --harness <absolute-control-harness> \
 *     --workflow <id> --inventory <absolute-json> \
 *     [--check-exit <track-id> | --check-convergence]
 *
 * Authority, deliberately narrow:
 *   * The inventory is a NON-authoritative ownership receipt for one scheduling checkpoint.
 *     The workflow snapshot stays the only claim source, and `mstar-harness worktree cleanup`
 *     stays the only mechanism allowed to delete a worktree or a branch.
 *   * This script never deletes, never writes to the snapshot/register, never scans all home
 *     directories and never expands a wildcard. Faults are refused, never repaired.
 *   * `--apply` is refused until the guarded apply half lands (P1-T3); the dry run already
 *     invokes and records the real installed engine cleanup command.
 *   * Snapshot data is projected through an allowlist: session ids, lease holders and session
 *     labels are never copied into output or diagnostics.
 *   * A worktree whose index holds submodule gitlinks is reported as `blocked`, not as
 *     permission: on this repository a non-forced `git worktree remove` is inadmissible there,
 *     and the refusal survives `git submodule deinit --all` (measured by PM on Git 2.54). Each
 *     worktree therefore reports its measured gitlink count and how many initialized submodule
 *     checkouts point at an unresolvable gitdir — the diagnosis for this repository's linked
 *     checkouts. This script never deinitializes (that mutates shared configuration) and never
 *     forces; any recovery remains a separately authorized decision.
 *
 * Exit codes: 0 valid dry run / passing check; 1 unreadable facts (snapshot, sibling
 * declarations, git, engine, path) or a failing requested check; 2 invalid invocation or inventory.
 */
import { execFile } from 'node:child_process';
import { lstat, readFile, readdir, realpath, statfs } from 'node:fs/promises';
import { availableParallelism, cpus, homedir, tmpdir } from 'node:os';
import { basename, isAbsolute, join, normalize, resolve, sep } from 'node:path';
import { promisify } from 'node:util';
import { pathToFileURL } from 'node:url';

const exec = promisify(execFile);

const INVENTORY_VERSION = 1;
const PLAN_STATUSES = new Set(['Todo', 'InProgress', 'InReview', 'Blocked', 'Done']);
const TERMINAL_PLAN_STATUS = 'Done';
const TRACK_STATES = new Set(['active', 'completed']);
const WORKFLOW_ID = /^[A-Za-z0-9._-]+$/;

const ENGINE_BINARY = 'mstar-harness';
const ENGINE_TIMEOUT_MS = 30_000;
const CAPTURE_LIMIT = 64 * 1024;
const GIT_BUFFER = 4 * 1024 * 1024;
const SIZE_WALK_CONCURRENCY = 64;

const GIB = 1024 ** 3;
const WATERMARK_ROOT_FREE_MIN_BYTES = 90 * GIB;
const WATERMARK_FEATURE_TARGETS_MAX_BYTES = 120 * GIB;

const EXIT_OK = 0;
const EXIT_FACTS = 1;
const EXIT_INVALID = 2;

const CANONICAL_TARGET_NAME = 'nexus-target';
const FEATURE_TARGET_PREFIX = 'nexus-target-';
const INTEGRATION_CHECKOUT_PREFIX = 'iteration-';

class UsageError extends Error {}

// --- small helpers -----------------------------------------------------------------------

const plainRow = value => typeof value === 'object' && value !== null && !Array.isArray(value);
const nonEmptyString = value => typeof value === 'string' && value.trim() !== '';
const absolutePath = value => nonEmptyString(value) && isAbsolute(value);
const safeCount = value => Number.isSafeInteger(value) && value >= 0;
const sorted = (values, key = value => value) => [...values].sort((a, b) => (key(a) < key(b) ? -1 : key(a) > key(b) ? 1 : 0));
const unique = values => [...new Set(values)];

/** Strict descendant test on resolved paths; never follows links. */
function isWithin(parent, child) {
  const root = resolve(parent);
  const candidate = resolve(child);
  return candidate !== root && candidate.startsWith(`${root}${sep}`);
}

/** Mirrors the engine's own path identity: realpath when the path exists, resolved text otherwise. */
async function pathKey(path) {
  try {
    return await realpath(path);
  } catch {
    return resolve(path);
  }
}

/**
 * Existence probe for paths whose contents are none of this tool's business (worktrees).
 * Only `ENOENT` proves absence: any other failure is an unreadable fact, and every caller must
 * refuse it instead of reading the failure as "nothing is there".
 */
async function describePath(path) {
  try {
    const stats = await lstat(path);
    return { exists: true, is_symlink: stats.isSymbolicLink(), unreadable: null };
  } catch (error) {
    if (error.code === 'ENOENT') return { exists: false, is_symlink: false, unreadable: null };
    return { exists: false, is_symlink: false, unreadable: error.code ?? 'lstat-failed' };
  }
}

/**
 * Split a resolved receipt path into the canonical root it names and the segments below that root.
 * A path already spelled through the canonical root is a plain prefix slice; any other spelling is
 * canonicalized one ancestor at a time from the filesystem root until an ancestor lands exactly on
 * `root`. Only ancestors of the root are ever followed (the walk stops the moment it reaches the
 * root), so a link *below* the root is never traversed here — the caller's segment walk refuses it.
 * `tail` is null when the path never lands on the root, which includes the root itself and any
 * path whose alias prefix does not exist; `unreadable` carries the failure code of a canonical
 * walk that failed for a reason other than `ENOENT`.
 */
async function canonicalAlias(root, target) {
  if (target === root) return { tail: null, unreadable: null };
  if (target.startsWith(`${root}${sep}`)) return { tail: target.slice(root.length + 1), unreadable: null };
  const segments = target.split(sep);
  let current = segments[0] === '' ? sep : `${segments[0]}${sep}`;
  for (const [index, segment] of segments.slice(1).entries()) {
    current = join(current, segment);
    let canonical;
    try {
      canonical = await realpath(current);
    } catch (error) {
      if (error.code !== 'ENOENT') return { tail: null, unreadable: error.code ?? 'realpath-failed' };
      return { tail: null, unreadable: null };
    }
    if (canonical === root) {
      const tail = segments.slice(index + 2).join(sep);
      return { tail: tail === '' ? null : tail, unreadable: null };
    }
  }
  return { tail: null, unreadable: null };
}

/**
 * Canonical containment for an ownership receipt. The candidate's own root alias is canonicalized
 * first — a receipt may spell the temp root through a system alias (`/var/...` versus canonical
 * `/private/var/...` on Darwin, or any `TMPDIR` form that traverses a link) while `tmpdir()` is not
 * canonical on every platform — and only then are the candidate's existing components below the
 * root `lstat`ed one segment at a time, so a linked parent is never traversed to reach a location
 * the receipt never described, and a `..`-free resolved prefix is all that can be claimed.
 * `within` is false for a receipt that is not a plain descendant of the root; `unreadable` carries
 * the failure code when the walk itself cannot be completed (a non-`ENOENT` failure), which the
 * caller must refuse as an unreadable fact rather than as a stale claim — and which a deeper
 * `ENOENT` never is: the tail simply does not exist yet.
 */
async function canonicalWithin(parent, child) {
  const root = await pathKey(parent);
  const alias = await canonicalAlias(root, resolve(child));
  if (alias.unreadable !== null) return { within: false, unreadable: alias.unreadable };
  if (alias.tail === null) return { within: false, unreadable: null };
  let current = root;
  for (const segment of alias.tail.split(sep)) {
    current = join(current, segment);
    let stats;
    try {
      stats = await lstat(current);
    } catch (error) {
      if (error.code === 'ENOENT') return { within: true, unreadable: null };
      return { within: false, unreadable: error.code ?? 'lstat-failed' };
    }
    if (stats.isSymbolicLink()) return { within: false, unreadable: null };
  }
  return { within: true, unreadable: null };
}

function refusal(code, detail, exitCode) {
  return { code, detail, exit_code: exitCode };
}

function cap(text) {
  const value = String(text ?? '');
  if (value.length <= CAPTURE_LIMIT) return { text: value, truncated: false };
  return { text: value.slice(0, CAPTURE_LIMIT), truncated: true };
}

// --- arguments ---------------------------------------------------------------------------

function parseArgs(argv) {
  const options = { apply: false, checkExit: null, checkConvergence: false, help: false };
  const named = { '--repo': 'repo', '--harness': 'harness', '--workflow': 'workflow', '--inventory': 'inventory' };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '-h' || arg === '--help') {
      if (options.help) throw new UsageError('duplicate argument: --help');
      options.help = true;
      continue;
    }
    if (arg === '--apply') {
      if (options.apply) throw new UsageError('duplicate argument: --apply');
      options.apply = true;
      continue;
    }
    if (arg === '--check-convergence') {
      if (options.checkConvergence) throw new UsageError('duplicate argument: --check-convergence');
      options.checkConvergence = true;
      continue;
    }
    if (arg === '--check-exit' || Object.hasOwn(named, arg)) {
      const value = argv[index + 1];
      if (value === undefined || value.startsWith('-')) throw new UsageError(`missing value for ${arg}`);
      index += 1;
      if (arg === '--check-exit') {
        if (options.checkExit !== null) throw new UsageError('duplicate argument: --check-exit');
        options.checkExit = value;
        continue;
      }
      const key = named[arg];
      if (options[key] !== undefined) throw new UsageError(`duplicate argument: ${arg}`);
      options[key] = value;
      continue;
    }
    throw new UsageError(`unknown argument: ${arg}`);
  }
  if (options.help) return options;
  for (const [flag, key] of Object.entries(named)) {
    const value = options[key];
    if (value === undefined) throw new UsageError(`missing required argument: ${flag}`);
    if (flag !== '--workflow' && !absolutePath(value)) throw new UsageError(`${flag} must be an absolute path`);
  }
  if (!WORKFLOW_ID.test(options.workflow) || options.workflow === '.' || options.workflow === '..') {
    throw new UsageError(`--workflow must be a single path segment without traversal (got ${JSON.stringify(options.workflow)})`);
  }
  const checkModes = Number(options.checkExit !== null) + Number(options.checkConvergence);
  if (checkModes > 1) throw new UsageError('--check-exit and --check-convergence are mutually exclusive');
  if (options.apply && checkModes > 0) throw new UsageError('--apply is mutually exclusive with --check-exit/--check-convergence');
  if (options.apply) {
    throw new UsageError('--apply is unavailable: this round delivers the read-only sweeper only, and the guarded apply half lands in P1-T3');
  }
  return options;
}

function usage() {
  return [
    'Usage: node scripts/worktree-sweep.mjs --repo <absolute-main-root> --harness <absolute-control-harness>',
    '       --workflow <id> --inventory <absolute-json> [--check-exit <track-id> | --check-convergence]',
    '',
    'Read-only worktree/cache sweep: parses the version-1 ownership inventory, reconciles it with',
    'the workflow snapshot and real Git facts, measures capacity, and proposes ordered reclamation',
    'actions. Nothing is deleted by this script.',
    '',
    'Options:',
    '  --repo <path>        Absolute main checkout root (its Git worktree list is the main root).',
    '  --harness <path>     Absolute control harness directory holding workflows/<id>/snapshot.json.',
    '  --workflow <id>      Workflow id whose snapshot drives claims and protection.',
    '  --inventory <path>   Absolute version-1 ownership receipt document.',
    '  --check-exit <id>    Read-only: assert this completed track keeps no target/temp/worktree footprint.',
    '  --check-convergence  Read-only: assert main + integration are the only worktrees listed and no unclaimed footprint remains.',
    '  --apply              Refused: the guarded apply half lands in P1-T3.',
    '  -h, --help           Print this usage.',
    '',
    'Exit codes: 0 valid dry run or passing check; 1 unreadable facts or failing check; 2 invalid invocation/inventory.',
    '',
    'A worktree whose index holds submodule gitlinks is reported as `blocked` rather than proposed:',
    'a non-forced removal is inadmissible here, and this tool never forces and never runs',
    '`git submodule deinit` (that mutates shared configuration). Recovery stays an authorized decision',
    'taken with merged, clean and released proof.',
    '',
  ].join('\n');
}

// --- capacity policy (pure; G3) -----------------------------------------------------------

/** K = min(ready independent tasks, floor(budget / per-track estimate), max(1, cores/2)). */
export function computeAvailableK({ readyIndependentTasks, diskBudgetBytes, perTrackTargetEstimateBytes, cores }) {
  if (!safeCount(readyIndependentTasks) || readyIndependentTasks === 0) return 0;
  if (!safeCount(diskBudgetBytes)) return 0;
  if (!safeCount(perTrackTargetEstimateBytes) || perTrackTargetEstimateBytes === 0) return 0;
  if (!safeCount(cores)) return 0;
  const byDisk = Math.floor(diskBudgetBytes / perTrackTargetEstimateBytes);
  const byCores = Math.max(1, Math.floor(cores / 2));
  return Math.max(0, Math.min(readyIndependentTasks, byDisk, byCores));
}

/** Root-free and feature-target watermarks are measured independently; either failure demands reclamation. */
export function evaluateWatermarks({ rootFreeBytes, featureTargetBytes }) {
  const rootFreeOk = safeCount(rootFreeBytes) && rootFreeBytes >= WATERMARK_ROOT_FREE_MIN_BYTES;
  const featureTargetsOk = safeCount(featureTargetBytes) && featureTargetBytes <= WATERMARK_FEATURE_TARGETS_MAX_BYTES;
  return {
    units: 'bytes',
    root_free_min_bytes: WATERMARK_ROOT_FREE_MIN_BYTES,
    feature_targets_max_bytes: WATERMARK_FEATURE_TARGETS_MAX_BYTES,
    root_free_ok: rootFreeOk,
    feature_targets_ok: featureTargetsOk,
    reclamation_required: !(rootFreeOk && featureTargetsOk),
  };
}

// --- filesystem facts --------------------------------------------------------------------

/**
 * Measured size of an explicit owned path. Symlinks are never followed and never counted.
 * A non-`ENOENT` failure — at the path itself, at a directory read, or at a descendant stat —
 * is reported as `unreadable` rather than as absence or as a silently partial total, because an
 * unmeasured footprint must never be certified gone or read as enough free capacity. A descendant
 * that vanishes mid-walk is `ENOENT` and simply contributes no bytes.
 */
async function pathBytes(path) {
  let stats;
  try {
    stats = await lstat(path);
  } catch (error) {
    if (error.code === 'ENOENT') return { exists: false, is_symlink: false, bytes: 0, unreadable: null };
    return { exists: false, is_symlink: false, bytes: 0, unreadable: error.code ?? 'lstat-failed' };
  }
  if (stats.isSymbolicLink()) return { exists: true, is_symlink: true, bytes: 0, unreadable: null };
  if (!stats.isDirectory()) return { exists: true, is_symlink: false, bytes: stats.size, unreadable: null };
  let total = 0;
  let unreadable = null;
  const noteUnreadable = error => {
    if (unreadable === null && error.code !== 'ENOENT') unreadable = error.code ?? 'read-failed';
  };
  let queue = [path];
  while (queue.length > 0) {
    const next = [];
    const files = [];
    for (const directory of queue) {
      let entries;
      try {
        entries = await readdir(directory, { withFileTypes: true });
      } catch (error) {
        noteUnreadable(error);
        continue;
      }
      for (const entry of entries) {
        if (entry.isSymbolicLink()) continue;
        if (entry.isDirectory()) next.push(join(directory, entry.name));
        else if (entry.isFile()) files.push(join(directory, entry.name));
      }
    }
    for (let index = 0; index < files.length; index += SIZE_WALK_CONCURRENCY) {
      const batch = files.slice(index, index + SIZE_WALK_CONCURRENCY);
      const sizes = await Promise.all(batch.map(async file => {
        try {
          return (await lstat(file)).size;
        } catch (error) {
          noteUnreadable(error);
          return 0;
        }
      }));
      for (const size of sizes) total += size;
    }
    queue = next;
  }
  return { exists: true, is_symlink: false, bytes: total, unreadable };
}

/**
 * Read-only submodule shape of one worktree: the gitlink count Git itself inspects, plus how
 * many initialized submodule checkouts point at a gitdir that does not resolve. The second fact
 * is the diagnosis for this repository's linked checkouts; the first is what makes a non-forced
 * `git worktree remove` inadmissible here.
 */
async function submoduleState(worktreePath, environment) {
  const staged = await runGit(['ls-files', '--stage'], worktreePath, environment);
  if (staged.exit_code !== 0) {
    return { error: `git ls-files failed (${staged.spawn_error ?? staged.stderr.trim()})` };
  }
  const gitlinks = staged.stdout
    .split('\n')
    .filter(line => line.startsWith('160000 '))
    .map(line => line.split('\t')[1])
    .filter(path => nonEmptyString(path));
  let unresolved = 0;
  for (const relative of gitlinks) {
    const pointer = join(worktreePath, relative, '.git');
    let text;
    try {
      text = await readFile(pointer, 'utf8');
    } catch {
      continue; // Uninitialized checkout: no pointer to resolve, nothing to diagnose.
    }
    const match = /^gitdir:\s*(.+)$/m.exec(text);
    if (match === null) {
      unresolved += 1;
      continue;
    }
    const gitdir = resolve(join(worktreePath, relative), match[1].trim());
    if (!(await describePath(gitdir)).exists) unresolved += 1;
  }
  return { gitlinks: gitlinks.length, unresolved };
}

async function freeBytes(path) {
  try {
    const stats = await statfs(path);
    return { bytes: stats.bavail * stats.bsize, error: null };
  } catch (error) {
    return { bytes: null, error: error.code ?? String(error.message) };
  }
}

function cacheRootFrom(environment) {
  const explicit = environment.XDG_CACHE_HOME;
  if (explicit !== undefined && explicit !== '') {
    if (!isAbsolute(explicit)) return { error: `XDG_CACHE_HOME must be absolute (got ${JSON.stringify(explicit)})` };
    return { path: normalize(explicit) };
  }
  return { path: join(homedir(), '.cache') };
}

/** Exact `.envrc` mapping: `iteration-*` shares the canonical target, every feature gets `nexus-target-<dirname>`. */
function expectedTargetFor(cacheRoot, worktreePath) {
  const name = basename(worktreePath);
  return join(cacheRoot, name.startsWith(INTEGRATION_CHECKOUT_PREFIX) ? CANONICAL_TARGET_NAME : `${FEATURE_TARGET_PREFIX}${name}`);
}

// --- git facts ---------------------------------------------------------------------------

async function runGit(args, cwd, environment) {
  try {
    const { stdout, stderr } = await exec('git', args, {
      cwd,
      env: environment,
      maxBuffer: GIT_BUFFER,
      timeout: ENGINE_TIMEOUT_MS,
    });
    return { exit_code: 0, stdout, stderr, spawn_error: null };
  } catch (error) {
    const code = typeof error.code === 'number' ? error.code : null;
    return {
      exit_code: code,
      stdout: String(error.stdout ?? ''),
      stderr: String(error.stderr ?? error.message ?? ''),
      spawn_error: code === null ? String(error.code ?? 'spawn-failed') : null,
    };
  }
}

function parseWorktreeList(porcelain) {
  const records = [];
  let current = null;
  for (const line of String(porcelain).split('\n')) {
    if (line.startsWith('worktree ')) {
      if (current !== null) records.push(current);
      current = { path: line.slice('worktree '.length), head: null, branch: null, detached: false, locked: false, prunable: false, bare: false };
      continue;
    }
    if (current === null || line === '') continue;
    if (line.startsWith('HEAD ')) current.head = line.slice('HEAD '.length);
    else if (line.startsWith('branch refs/heads/')) current.branch = line.slice('branch refs/heads/'.length);
    else if (line === 'detached') current.detached = true;
    else if (line === 'bare') current.bare = true;
    else if (line.startsWith('locked')) current.locked = true;
    else if (line.startsWith('prunable')) current.prunable = true;
  }
  if (current !== null) records.push(current);
  return records.map((record, index) => ({ ...record, is_main: index === 0 }));
}

// --- snapshot declarations ---------------------------------------------------------------

async function readSnapshotFile(path) {
  let text;
  try {
    text = await readFile(path, 'utf8');
  } catch (error) {
    return { error: `snapshot unreadable (${error.code ?? 'read-failed'})` };
  }
  let doc;
  try {
    doc = JSON.parse(text);
  } catch {
    return { error: 'snapshot is not valid JSON' };
  }
  if (!plainRow(doc)) return { error: 'snapshot is not a JSON object' };
  return { doc };
}

/** Project a raw snapshot onto the declaration fields this tool is allowed to reason about. */
function declarationOf(doc, fallbackId) {
  const rawPlans = doc.plans === undefined ? [] : doc.plans;
  if (!Array.isArray(rawPlans)) return { error: 'snapshot plans field is not an array' };
  const plans = [];
  for (const row of rawPlans) {
    if (!plainRow(row)) return { error: 'snapshot plan declaration is not an object' };
    const id = nonEmptyString(row.id) ? row.id : nonEmptyString(row.plan_id) ? row.plan_id : undefined;
    if (id === undefined) return { error: 'snapshot plan declaration carries no id' };
    if (typeof row.status !== 'string' || !PLAN_STATUSES.has(row.status)) {
      return { error: `snapshot plan ${id} carries no known status` };
    }
    const lease = plainRow(row.execution_lease) ? row.execution_lease : null;
    const metadata = plainRow(row.metadata) ? row.metadata : null;
    const branches = new Set();
    const worktreePaths = [];
    if (nonEmptyString(lease?.working_branch)) branches.add(lease.working_branch);
    if (metadata !== null) {
      if (nonEmptyString(metadata.working_branch)) branches.add(metadata.working_branch);
      for (const branch of Array.isArray(metadata.track_branches) ? metadata.track_branches : []) {
        if (nonEmptyString(branch)) branches.add(branch);
      }
      if (nonEmptyString(metadata.worktree_path)) worktreePaths.push(metadata.worktree_path);
    }
    if (nonEmptyString(lease?.worktree_path)) worktreePaths.push(lease.worktree_path);
    plans.push({ id, status: row.status, branches, worktreePaths, leased: lease !== null });
  }
  const branch = plainRow(doc.branch) ? doc.branch : {};
  return {
    id: nonEmptyString(doc.id) ? doc.id : fallbackId,
    type: doc.type === 'iteration' ? 'iteration' : 'plan',
    integration_branch: nonEmptyString(branch.integration) ? branch.integration : undefined,
    integration_worktree_path: nonEmptyString(doc.integration_worktree_path)
      ? doc.integration_worktree_path
      : nonEmptyString(doc.control_worktree_path) ? doc.control_worktree_path : undefined,
    plans,
  };
}

/**
 * Read the explicit workflow declaration plus the sibling declarations under the harness.
 * Sibling snapshots are bounded to `<harness>/workflows/*​/snapshot.json` — never a home scan —
 * and an unreadable sibling withholds every removal because its claims are unknown.
 */
async function readDeclarations(harnessDir, workflowId) {
  const workflowsDir = join(harnessDir, 'workflows');
  let entries;
  try {
    entries = await readdir(workflowsDir, { withFileTypes: true });
  } catch (error) {
    return { error: `harness workflows directory unreadable (${error.code ?? 'read-failed'})` };
  }
  let own = null;
  let ownError = 'snapshot is missing';
  const unreadable = [];
  const siblings = [];
  for (const entry of sorted(entries, candidate => candidate.name)) {
    if (!entry.isDirectory()) continue;
    const id = entry.name;
    const read = await readSnapshotFile(join(workflowsDir, id, 'snapshot.json'));
    const declaration = read.error === undefined ? declarationOf(read.doc, id) : { error: read.error };
    if (declaration.error !== undefined) {
      if (id === workflowId) ownError = declaration.error;
      else unreadable.push({ workflow_id: id, detail: declaration.error });
      continue;
    }
    if (id === workflowId) own = declaration;
    else siblings.push(declaration);
  }
  if (own === null) return { error: ownError, unreadable };
  return { own, siblings, unreadable };
}

function branchClaimers(declaration, branch) {
  const claimers = [];
  if (declaration.integration_branch === branch) claimers.push({ plan_id: null, source: 'integration-branch' });
  for (const plan of declaration.plans) {
    if (plan.branches.has(branch)) claimers.push({ plan_id: plan.id, source: 'plan-branch-claim' });
  }
  return claimers;
}

// --- inventory ---------------------------------------------------------------------------

async function readInventory(path) {
  let text;
  try {
    text = await readFile(path, 'utf8');
  } catch (error) {
    return { error: `inventory unreadable (${error.code ?? 'read-failed'})` };
  }
  try {
    return { raw: JSON.parse(text) };
  } catch {
    return { error: 'inventory is not valid JSON' };
  }
}

function validateScheduling(raw, refuse) {
  if (!plainRow(raw)) {
    refuse('sweeper.refuse.inventory-shape', 'scheduling must be an object');
    return null;
  }
  const fields = [
    ['ready_independent_tasks', value => safeCount(value), 'nonnegative safe integer'],
    ['disk_budget_bytes', value => safeCount(value), 'nonnegative safe integer'],
    ['per_track_target_estimate_bytes', value => safeCount(value) && value > 0, 'positive safe integer'],
  ];
  for (const [field, valid, expectation] of fields) {
    if (!valid(raw[field])) {
      refuse('sweeper.refuse.inventory-scheduling', `scheduling.${field} must be a ${expectation} (got ${JSON.stringify(raw[field])})`);
    }
  }
  return {
    ready_independent_tasks: raw.ready_independent_tasks,
    disk_budget_bytes: raw.disk_budget_bytes,
    per_track_target_estimate_bytes: raw.per_track_target_estimate_bytes,
  };
}

/**
 * Structural + semantic inventory validation. Every failure is recorded (never thrown) so the
 * operator sees the whole refusal set; any refusal withholds proposals and engine invocations.
 */
async function reconcileInventory({ raw, workflowId, declaration, foreignClaims, worktrees, mainRoot, cacheRoot, tempRoot }) {
  const refusals = [];
  const refuse = (code, detail, exitCode = EXIT_INVALID) => refusals.push(refusal(code, detail, exitCode));
  const result = { refusals, tracks: [], scheduling: null };

  if (!plainRow(raw)) {
    refuse('sweeper.refuse.inventory-shape', 'inventory document is not a JSON object');
    return result;
  }
  if (raw.version !== INVENTORY_VERSION) {
    refuse('sweeper.refuse.inventory-version', `inventory version must be ${INVENTORY_VERSION} (got ${JSON.stringify(raw.version)})`);
  }
  if (raw.workflow_id !== workflowId) {
    refuse('sweeper.refuse.inventory-workflow', `inventory workflow_id ${JSON.stringify(raw.workflow_id)} does not match --workflow ${JSON.stringify(workflowId)}`);
  }
  result.scheduling = validateScheduling(raw.scheduling, refuse);

  let activePlanIds = new Set();
  if (!Array.isArray(raw.active_plan_ids) || raw.active_plan_ids.some(id => !nonEmptyString(id))) {
    refuse('sweeper.refuse.inventory-active-plans', 'active_plan_ids must be an array of non-empty ids');
  } else {
    activePlanIds = new Set(raw.active_plan_ids);
    if (activePlanIds.size !== raw.active_plan_ids.length) {
      refuse('sweeper.refuse.inventory-active-plans', 'active_plan_ids carries duplicate ids');
    }
    const required = declaration.plans.filter(plan => plan.status !== TERMINAL_PLAN_STATUS || plan.leased).map(plan => plan.id);
    const missing = required.filter(id => !activePlanIds.has(id));
    if (missing.length > 0) {
      refuse('sweeper.refuse.inventory-omission', `active_plan_ids omits claimed plan(s): ${missing.join(', ')}`);
    }
    const known = new Set(declaration.plans.map(plan => plan.id));
    const unknown = sorted(unique([...activePlanIds].filter(id => !known.has(id))));
    if (unknown.length > 0) {
      refuse('sweeper.refuse.stale-plan-claim', `active_plan_ids names plan(s) the snapshot does not declare: ${unknown.join(', ')}`);
    }
  }

  if (!Array.isArray(raw.tracks)) {
    refuse('sweeper.refuse.inventory-tracks', 'tracks must be an array');
    return result;
  }

  const worktreeByKey = new Map();
  for (const worktree of worktrees) worktreeByKey.set(await pathKey(worktree.path), worktree);
  const seen = { track_id: new Map(), worktree: new Map(), target: new Map(), branch: new Map(), temporary: new Map() };

  for (const [index, entry] of raw.tracks.entries()) {
    const where = `tracks[${index}]`;
    if (!plainRow(entry)) {
      refuse('sweeper.refuse.track-shape', `${where} is not an object`);
      continue;
    }
    const shape = [
      ['track_id', nonEmptyString(entry.track_id)],
      ['plan_id', nonEmptyString(entry.plan_id)],
      ['worktree', absolutePath(entry.worktree)],
      ['branch', nonEmptyString(entry.branch)],
      ['target', absolutePath(entry.target)],
      ['producer_stopped', typeof entry.producer_stopped === 'boolean'],
    ];
    let shaped = true;
    for (const [field, valid] of shape) {
      if (!valid) {
        shaped = false;
        refuse('sweeper.refuse.track-shape', `${where}.${field} is missing or invalid (got ${JSON.stringify(entry[field])})`);
      }
    }
    if (!nonEmptyString(entry.state)) {
      shaped = false;
      refuse('sweeper.refuse.track-shape', `${where}.state is missing`);
    } else if (!TRACK_STATES.has(entry.state)) {
      refuse('sweeper.refuse.unknown-state', `${where} (${entry.track_id}) declares unknown state ${JSON.stringify(entry.state)}`);
      shaped = false;
    }
    if (!Array.isArray(entry.temporary_paths) || entry.temporary_paths.some(path => !absolutePath(path))) {
      refuse('sweeper.refuse.track-shape', `${where}.temporary_paths must be an array of absolute receipt paths`);
      shaped = false;
    }
    if (!shaped) continue;

    for (const field of ['track_id', 'worktree', 'target', 'branch']) {
      const value = field === 'worktree' || field === 'target' ? await pathKey(entry[field]) : entry[field];
      const previous = seen[field].get(value);
      if (previous !== undefined) {
        refuse('sweeper.refuse.duplicate-track', `${where} duplicates ${field} of track ${previous}`);
        continue;
      }
      seen[field].set(value, entry.track_id);
    }

    // A temporary receipt is exclusive ownership: two tracks naming the same path mean at least
    // one of them is wrong, and the completed one would otherwise propose reclaiming a path an
    // active or foreign track still claims.
    for (const temporary of entry.temporary_paths) {
      const key = await pathKey(temporary);
      const previous = seen.temporary.get(key);
      if (previous !== undefined) {
        refuse('sweeper.refuse.duplicate-temporary', `${where} (${entry.track_id}) lists temporary ${temporary} already claimed by track ${previous}`);
        continue;
      }
      seen.temporary.set(key, entry.track_id);
    }

    const claimers = branchClaimers(declaration, entry.branch);
    const planClaimers = claimers.filter(claimer => claimer.plan_id === entry.plan_id);
    if (claimers.length === 0) {
      refuse('sweeper.refuse.stale-branch-claim', `track ${entry.track_id} declares branch ${JSON.stringify(entry.branch)} that the workflow snapshot does not claim`);
    } else if (claimers.length > 1) {
      refuse('sweeper.refuse.ambiguous-claim', `branch ${JSON.stringify(entry.branch)} is claimed by more than one snapshot row — treated as unowned`);
    } else if (planClaimers.length === 0) {
      refuse('sweeper.refuse.stale-owner-pair', `track ${entry.track_id} pairs plan ${entry.plan_id} with a branch claimed by a different row`);
    }
    if (!activePlanIds.has(entry.plan_id)) {
      refuse('sweeper.refuse.inventory-omission', `track ${entry.track_id} names plan ${entry.plan_id} that active_plan_ids omits`);
    }

    const expectedTarget = expectedTargetFor(cacheRoot, entry.worktree);
    if (resolve(entry.target) !== resolve(expectedTarget)) {
      refuse('sweeper.refuse.stale-target', `track ${entry.track_id} declares target ${entry.target}; the .envrc mapping for worktree ${basename(entry.worktree)} is ${expectedTarget}`);
    }
    if (resolve(entry.target) === resolve(join(cacheRoot, CANONICAL_TARGET_NAME))) {
      refuse('sweeper.refuse.protected-target', `track ${entry.track_id} declares the shared canonical cache as its reclamation target`);
    }

    const worktreeKey = await pathKey(entry.worktree);
    const worktree = worktreeByKey.get(worktreeKey) ?? null;
    if (worktree === null) {
      const onDisk = await describePath(entry.worktree);
      if (onDisk.unreadable !== null) {
        refuse('sweeper.refuse.path-unreadable', `track ${entry.track_id} worktree ${entry.worktree} cannot be read (${onDisk.unreadable})`, EXIT_FACTS);
      } else if (onDisk.exists || entry.state === 'active') {
        refuse('sweeper.refuse.stale-worktree', `track ${entry.track_id} declares worktree ${entry.worktree} that Git does not list`);
      }
    } else if (worktree.branch !== entry.branch) {
      refuse('sweeper.refuse.stale-worktree-pair', `worktree ${entry.worktree} has ${worktree.branch} checked out, not the declared ${entry.branch}`);
    }

    const foreignBranch = foreignClaims.branches.get(entry.branch);
    if (foreignBranch !== undefined) {
      refuse('sweeper.refuse.foreign-claim', `branch ${entry.branch} is claimed by foreign workflow ${foreignBranch}`);
    }
    const foreignPath = foreignClaims.paths.get(worktreeKey) ?? foreignClaims.paths.get(await pathKey(entry.target));
    if (foreignPath !== undefined) {
      refuse('sweeper.refuse.foreign-claim', `track ${entry.track_id} names a path claimed by foreign workflow ${foreignPath}`);
    }

    for (const temporary of entry.temporary_paths) {
      const containment = await canonicalWithin(tempRoot, temporary);
      if (containment.unreadable !== null) {
        refuse('sweeper.refuse.path-unreadable', `track ${entry.track_id} temporary ${temporary} cannot be resolved (${containment.unreadable})`, EXIT_FACTS);
      } else if (!containment.within) {
        refuse('sweeper.refuse.stale-temporary', `track ${entry.track_id} temporary ${temporary} is not an unlinked receipt path inside the system temp root`);
      } else if (isWithin(cacheRoot, temporary) || isWithin(mainRoot, temporary)) {
        refuse('sweeper.refuse.stale-temporary', `track ${entry.track_id} temporary ${temporary} is inside a protected shared location`);
      }
    }

    result.tracks.push({
      track_id: entry.track_id,
      plan_id: entry.plan_id,
      state: entry.state,
      producer_stopped: entry.producer_stopped,
      branch: entry.branch,
      worktree_path: entry.worktree,
      worktree_key: worktreeKey,
      target_path: entry.target,
      expected_target: expectedTarget,
      temporary_paths: [...entry.temporary_paths],
      listed_worktree: worktree,
      claim: planClaimers[0] ?? null,
    });
  }
  return result;
}

// --- engine evidence ---------------------------------------------------------------------

async function invokeEngine({ mainRoot, workflowId, harnessDir, worktreePath, environment }) {
  const argv = [ENGINE_BINARY, 'worktree', 'cleanup', '--workflow', workflowId, '--harness', harnessDir, '--worktree', worktreePath];
  const record = { argv, exit_code: null, spawn_error: null, stdout: '', stderr: '', truncated: false };
  try {
    const { stdout, stderr } = await exec(argv[0], argv.slice(1), {
      cwd: mainRoot,
      env: environment,
      timeout: ENGINE_TIMEOUT_MS,
      maxBuffer: GIT_BUFFER,
    });
    const out = cap(stdout);
    const err = cap(stderr);
    record.exit_code = 0;
    record.stdout = out.text;
    record.stderr = err.text;
    record.truncated = out.truncated || err.truncated;
  } catch (error) {
    const code = typeof error.code === 'number' ? error.code : null;
    const out = cap(error.stdout);
    const err = cap(error.stderr ?? error.message);
    record.exit_code = code;
    record.spawn_error = code === null ? String(error.code ?? 'spawn-failed') : null;
    record.stdout = out.text;
    record.stderr = err.text;
    record.truncated = out.truncated || err.truncated;
  }
  return record;
}

/** The engine prints `verdict | kind | ref | reason`; a valid dry run may still refuse every row. */
async function engineDecision(record, worktreeKey) {
  const rows = String(record.stdout)
    .split('\n')
    .map(line => line.split('|').map(part => part.trim()))
    .filter(parts => parts.length >= 4);
  const mine = [];
  for (const parts of rows) {
    if ((await pathKey(parts[2])) === worktreeKey) mine.push(parts);
  }
  if (mine.some(parts => parts[0] === 'remove')) return { verdict: 'propose', reason: 'sweeper.propose.engine-remove' };
  const refused = mine.find(parts => parts[0] === 'refuse');
  if (refused !== undefined) return { verdict: 'refuse', reason: refused[3] === '' ? 'sweeper.refuse.engine-row' : refused[3] };
  return { verdict: 'refuse', reason: 'sweeper.refuse.engine-no-candidate' };
}

// --- sweep ------------------------------------------------------------------------------

function addAction(actions, kind, ref, verdict, reason) {
  actions.push({ order: actions.length + 1, kind, ref, verdict, reason });
}

/** G3 own-exit rule: producer stopped, own target/temporaries absent, worktree gone and unlisted. */
function exitReasons(track) {
  const reasons = [];
  const unreadable = [
    track.target.unreadable === null ? null : `target ${track.target.path}`,
    ...track.temporary_paths.filter(temporary => temporary.unreadable !== null).map(temporary => `temporary ${temporary.path}`),
    track.worktree.unreadable === null ? null : `worktree ${track.worktree.path}`,
  ].filter(entry => entry !== null);
  if (unreadable.length > 0) {
    // An unreadable fact is not an absent footprint: nothing is ever certified gone from it.
    reasons.push({ code: 'sweeper.check.path-unreadable', detail: `track ${track.track_id} facts cannot be read: ${unreadable.join(', ')}` });
  }
  if (track.state !== 'completed') reasons.push({ code: 'sweeper.check.track-not-completed', detail: `track ${track.track_id} is ${track.state}` });
  if (track.producer_stopped !== true) reasons.push({ code: 'sweeper.check.producer-running', detail: `track ${track.track_id} holds no producer-stopped receipt` });
  if (track.target.exists) reasons.push({ code: 'sweeper.check.target-present', detail: `track ${track.track_id} target ${track.target.path} still exists` });
  for (const temporary of track.temporary_paths) {
    if (temporary.exists) reasons.push({ code: 'sweeper.check.temporary-present', detail: `track ${track.track_id} temporary ${temporary.path} still exists` });
  }
  if (track.worktree.listed) {
    reasons.push({ code: 'sweeper.check.worktree-listed', detail: `track ${track.track_id} worktree ${track.worktree.path} is still listed by Git` });
    if (track.worktree.removal_blocked_by_submodules === true) {
      reasons.push({ code: 'sweeper.check.worktree-blocked-submodules', detail: `track ${track.track_id} worktree ${track.worktree.path} holds submodule gitlinks, so Git refuses its removal without a forced or shared-config-mutating shortcut` });
    }
  }
  if (track.worktree.exists) reasons.push({ code: 'sweeper.check.worktree-present', detail: `track ${track.track_id} worktree path ${track.worktree.path} still exists` });
  return reasons;
}

function buildTrackActions(fact, decision) {
  const actions = [];
  const track = fact.track;
  if (track.state !== 'completed') {
    addAction(actions, 'protected', track.worktree_path, 'protected', 'sweeper.protected.track-active');
    return actions;
  }
  const stopped = track.producer_stopped === true;
  const held = 'sweeper.refuse.producer-running';
  if (fact.target.exists) {
    addAction(actions, 'reclaim-target', fact.target.path, stopped ? 'propose' : 'refuse', stopped ? 'sweeper.propose.reclaim-owned-target' : held);
  } else {
    addAction(actions, 'reclaim-target', fact.target.path, 'absent', 'sweeper.absent.idempotent');
  }
  for (const temporary of fact.temporary_paths) {
    if (!temporary.exists) continue;
    addAction(actions, 'reclaim-temporary', temporary.path, stopped ? 'propose' : 'refuse', stopped ? 'sweeper.propose.reclaim-owned-temporary' : held);
  }
  if (fact.worktree.listed) {
    if (fact.worktree.removal_blocked_by_submodules) {
      // A submodule-bearing worktree is not removable here without force, and that refusal
      // survives `git submodule deinit --all` (PM measurement on Git 2.54). The engine's raw
      // dry-run row is still recorded above as evidence; the action is reported as blocked so no
      // caller can read permission out of it, no forced or shared-config-mutating shortcut is
      // taken, and the worktree's `submodule_unresolved_pointers` fact carries the diagnosis.
      addAction(actions, 'engine-worktree-removal', fact.worktree.path, 'blocked', 'sweeper.blocked.submodule-gitlinks');
    } else {
      const resolved = decision ?? { verdict: 'refuse', reason: 'sweeper.refuse.engine-no-plan' };
      const verdict = resolved.verdict === 'propose' && !stopped ? 'refuse' : resolved.verdict;
      addAction(actions, 'engine-worktree-removal', fact.worktree.path, verdict, verdict === 'refuse' ? resolved.reason : 'sweeper.propose.engine-remove');
      if (verdict === 'propose') addAction(actions, 'prune-dry-run', fact.main_root, 'propose', 'sweeper.propose.prune-after-removal');
    }
  } else {
    addAction(actions, 'engine-worktree-removal', fact.worktree.path, 'absent', 'sweeper.absent.idempotent');
  }
  return actions;
}

export async function sweepWorktreeInventory(options, environment = process.env) {
  const refusals = [];
  const refuse = (code, detail, exitCode = EXIT_FACTS) => refusals.push(refusal(code, detail, exitCode));

  const mode = options.checkExit !== null ? 'check-exit' : options.checkConvergence ? 'check-convergence' : 'dry-run';
  const cacheResolution = cacheRootFrom(environment);
  if (cacheResolution.error !== undefined) refuse('sweeper.refuse.cache-root', cacheResolution.error, EXIT_INVALID);
  const cacheRoot = cacheResolution.path ?? join(homedir(), '.cache');

  const listing = await runGit(['worktree', 'list', '--porcelain'], options.repo, environment);
  const worktrees = listing.exit_code === 0 ? parseWorktreeList(listing.stdout) : [];
  const mainRoot = worktrees.length > 0 ? await pathKey(worktrees[0].path) : null;
  const repoRoot = mainRoot ?? options.repo;
  if (listing.exit_code !== 0) {
    refuse('sweeper.refuse.git-unreadable', `git worktree list failed in ${options.repo} (${listing.spawn_error ?? listing.stderr.trim()})`);
  } else if (mainRoot !== await pathKey(options.repo)) {
    refuse('sweeper.refuse.repo-not-main-root', `--repo ${options.repo} is not the repository's main worktree (${worktrees[0].path})`, EXIT_INVALID);
  }

  const declarations = await readDeclarations(options.harness, options.workflow);
  if (declarations.error !== undefined) {
    refuse('sweeper.refuse.snapshot-unreadable', `workflow ${options.workflow}: ${declarations.error}`);
  }
  for (const sibling of declarations.unreadable ?? []) {
    refuse('sweeper.refuse.sibling-snapshot-unreadable', `sibling workflow ${sibling.workflow_id}: ${sibling.detail} — its declarations are unknown, so no removal is proposed`);
  }
  const declaration = declarations.own ?? null;
  if (declaration !== null && declaration.id !== options.workflow) {
    refuse('sweeper.refuse.snapshot-identity', `snapshot id ${declaration.id} does not match --workflow ${options.workflow}`, EXIT_INVALID);
  }

  const read = await readInventory(options.inventory);
  if (read.error !== undefined) refuse('sweeper.refuse.inventory-unreadable', read.error, EXIT_INVALID);

  const foreignClaims = { branches: new Map(), paths: new Map() };
  for (const sibling of declarations.siblings ?? []) {
    for (const plan of sibling.plans) {
      for (const branch of plan.branches) if (!foreignClaims.branches.has(branch)) foreignClaims.branches.set(branch, sibling.id);
    }
    if (sibling.integration_branch !== undefined && !foreignClaims.branches.has(sibling.integration_branch)) {
      foreignClaims.branches.set(sibling.integration_branch, sibling.id);
    }
    if (sibling.integration_worktree_path !== undefined) {
      foreignClaims.paths.set(await pathKey(sibling.integration_worktree_path), sibling.id);
    }
    // A sibling plan's declared worktree — and the feature target its `.envrc` mapping implies —
    // is a foreign claim even while that worktree is gone: a path a sibling plan still claims must
    // never read as free just because the branch happens to be claimed locally as well.
    for (const plan of sibling.plans) {
      for (const worktreePath of plan.worktreePaths) {
        for (const claimed of [worktreePath, expectedTargetFor(cacheRoot, worktreePath)]) {
          const key = await pathKey(claimed);
          if (!foreignClaims.paths.has(key)) foreignClaims.paths.set(key, sibling.id);
        }
      }
    }
  }

  const tempRoot = await pathKey(tmpdir());
  const reconciliation = declaration === null || read.raw === undefined
    ? { refusals: [], tracks: [], scheduling: null }
    : await reconcileInventory({
      raw: read.raw,
      workflowId: options.workflow,
      declaration,
      foreignClaims,
      worktrees,
      mainRoot: repoRoot,
      cacheRoot,
      tempRoot,
    });
  refusals.push(...reconciliation.refusals);

  // Protected checkouts: the main checkout plus the snapshot-recorded integration checkout.
  const canonicalTarget = join(cacheRoot, CANONICAL_TARGET_NAME);
  const protectedCheckouts = [];
  if (mainRoot !== null) {
    protectedCheckouts.push({ role: 'main', path: worktrees[0].path, branch: worktrees[0].branch, target: canonicalTarget, shared_canonical: true });
  }
  if (declaration !== null) {
    if (declaration.type === 'iteration' && declaration.integration_worktree_path === undefined) {
      refuse('sweeper.refuse.integration-unrecorded', `iteration workflow ${options.workflow} records no integration worktree path, so its occupancy is unknown`);
    }
    if (declaration.integration_worktree_path !== undefined) {
      protectedCheckouts.push({
        role: 'integration',
        path: declaration.integration_worktree_path,
        branch: declaration.integration_branch ?? null,
        target: canonicalTarget,
        shared_canonical: true,
      });
    }
  }
  const canonical = await pathBytes(canonicalTarget);
  if (canonical.unreadable !== null) refuse('sweeper.refuse.path-unreadable', `canonical shared target ${canonicalTarget} cannot be read (${canonical.unreadable})`);
  if (canonical.is_symlink) refuse('sweeper.refuse.canonical-symlink', `the canonical shared target ${canonicalTarget} is a symlink`);
  protectedCheckouts.forEach((checkout, index) => {
    checkout.target_present = canonical.exists;
    checkout.target_is_symlink = canonical.is_symlink;
    // Shared canonical occupancy is counted once, on the first protected checkout.
    checkout.target_bytes = index === 0 ? canonical.bytes : null;
    if (index > 0) checkout.target_bytes_counted_by = protectedCheckouts[0].role;
  });

  // Track facts.
  const trackFacts = [];
  for (const track of reconciliation.tracks) {
    const worktreeProbe = await describePath(track.worktree_path);
    if (worktreeProbe.unreadable !== null) {
      refuse('sweeper.refuse.path-unreadable', `track ${track.track_id} worktree ${track.worktree_path} cannot be read (${worktreeProbe.unreadable})`);
    }
    const target = await pathBytes(track.target_path);
    if (target.unreadable !== null) refuse('sweeper.refuse.path-unreadable', `track ${track.track_id} target ${track.target_path} cannot be read (${target.unreadable})`);
    if (target.is_symlink) refuse('sweeper.refuse.symlink-path', `track ${track.track_id} target ${track.target_path} is a symlink`);
    const temporaries = [];
    for (const path of track.temporary_paths) {
      const fact = await pathBytes(path);
      if (fact.unreadable !== null) refuse('sweeper.refuse.path-unreadable', `track ${track.track_id} temporary ${path} cannot be read (${fact.unreadable})`);
      if (fact.is_symlink) refuse('sweeper.refuse.symlink-path', `track ${track.track_id} temporary ${path} is a symlink`);
      temporaries.push({ path, exists: fact.exists, is_symlink: fact.is_symlink, bytes: fact.bytes, unreadable: fact.unreadable });
    }
    const listed = track.listed_worktree;
    let dirtyTracked = null;
    let submodules = { gitlinks: 0, unresolved: 0 };
    if (listed !== null) {
      const status = await runGit(['status', '--porcelain', '--untracked-files=no'], listed.path, environment);
      if (status.exit_code === 0) dirtyTracked = status.stdout.trim() !== '';
      else refuse('sweeper.refuse.git-unreadable', `git status failed in ${listed.path} (${status.spawn_error ?? status.stderr.trim()})`);
      submodules = await submoduleState(listed.path, environment);
      if (submodules.error !== undefined) refuse('sweeper.refuse.git-unreadable', `submodule probe failed in ${listed.path} (${submodules.error})`);
    }
    trackFacts.push({
      track,
      main_root: repoRoot,
      target: { path: track.target_path, expected_path: track.expected_target, exists: target.exists, is_symlink: target.is_symlink, bytes: target.bytes, unreadable: target.unreadable },
      temporary_paths: temporaries,
      worktree: {
        path: track.worktree_path,
        listed: listed !== null,
        exists: worktreeProbe.exists,
        unreadable: worktreeProbe.unreadable,
        checked_out_branch: listed?.branch ?? null,
        head: listed?.head ?? null,
        locked: listed?.locked ?? false,
        dirty_tracked: dirtyTracked,
        submodule_gitlinks: submodules.gitlinks ?? 0,
        submodule_unresolved_pointers: submodules.unresolved ?? 0,
        removal_blocked_by_submodules: (submodules.gitlinks ?? 0) > 0,
      },
    });
  }

  // Cache-root feature targets: measured aggregate, plus unclaimed leftovers no receipt explains.
  let aggregateFeatureTargetBytes = 0;
  let featureTargetCount = 0;
  const unknownPaths = [];
  const cacheEntries = { error: null, names: [] };
  try {
    const entries = await readdir(cacheRoot, { withFileTypes: true });
    cacheEntries.names = entries.map(entry => entry.name);
  } catch (error) {
    if (error.code !== 'ENOENT') {
      cacheEntries.error = error.code ?? 'read-failed';
      refuse('sweeper.refuse.cache-unreadable', `cache root ${cacheRoot} unreadable (${cacheEntries.error})`);
    }
  }
  const ownedTargetKeys = new Set(await Promise.all(trackFacts.map(fact => pathKey(fact.target.path))));
  for (const name of cacheEntries.names) {
    if (!name.startsWith(FEATURE_TARGET_PREFIX)) continue;
    const path = join(cacheRoot, name);
    const measured = await pathBytes(path);
    // An unreadable descendant must not silently shrink the aggregate: the measured share is
    // incomplete, so it is refused rather than reported as capacity the host still owns.
    if (measured.unreadable !== null) {
      refuse('sweeper.refuse.path-unreadable', `feature target ${path} cannot be measured (${measured.unreadable})`);
    }
    aggregateFeatureTargetBytes += measured.bytes;
    featureTargetCount += 1;
    if (ownedTargetKeys.has(await pathKey(path))) continue;
    const integrationNamed = name.slice(FEATURE_TARGET_PREFIX.length).startsWith(INTEGRATION_CHECKOUT_PREFIX);
    unknownPaths.push({
      path,
      kind: integrationNamed ? 'iteration-named-feature-target' : 'feature-target',
      bytes: measured.bytes,
      is_symlink: measured.is_symlink,
      verdict: integrationNamed ? 'protected' : 'unknown',
      reason: integrationNamed ? 'sweeper.unknown.integration-named' : 'sweeper.unknown.unclaimed-feature-target',
    });
  }

  const extraWorktrees = [];
  const protectedWorktreeKeys = new Set(await Promise.all(protectedCheckouts.map(checkout => pathKey(checkout.path))));
  const ownedWorktreeKeys = new Set(trackFacts.filter(fact => fact.worktree.listed).map(fact => fact.track.worktree_key));
  for (const worktree of worktrees) {
    const key = await pathKey(worktree.path);
    if (protectedWorktreeKeys.has(key) || ownedWorktreeKeys.has(key)) continue;
    extraWorktrees.push({ path: worktree.path, branch: worktree.branch, head: worktree.head });
    const integrationNamed = basename(worktree.path).startsWith(INTEGRATION_CHECKOUT_PREFIX);
    unknownPaths.push({
      path: worktree.path,
      kind: integrationNamed ? 'iteration-checkout' : 'worktree',
      bytes: null,
      is_symlink: false,
      verdict: integrationNamed ? 'protected' : 'unknown',
      reason: integrationNamed ? 'sweeper.unknown.integration-named' : 'sweeper.unknown.unclaimed-worktree',
    });
  }

  // Capacity: measured observations and configured policy stay distinct fields.
  const free = await freeBytes(repoRoot);
  if (free.error !== null) refuse('sweeper.refuse.free-space-unreadable', `statfs failed for ${repoRoot} (${free.error})`);
  const scheduling = reconciliation.scheduling;
  const schedulingValid = plainRow(scheduling)
    && safeCount(scheduling.ready_independent_tasks)
    && safeCount(scheduling.disk_budget_bytes)
    && safeCount(scheduling.per_track_target_estimate_bytes)
    && scheduling.per_track_target_estimate_bytes > 0;
  const watermarks = evaluateWatermarks({ rootFreeBytes: free.bytes, featureTargetBytes: aggregateFeatureTargetBytes });
  const computedK = schedulingValid
    ? computeAvailableK({
      readyIndependentTasks: scheduling.ready_independent_tasks,
      diskBudgetBytes: scheduling.disk_budget_bytes,
      perTrackTargetEstimateBytes: scheduling.per_track_target_estimate_bytes,
      cores: cpus().length,
    })
    : null;
  const capacity = {
    units: 'bytes',
    root_free_bytes: free.bytes,
    root_free_measured_on: repoRoot,
    aggregate_feature_target_bytes: aggregateFeatureTargetBytes,
    feature_target_count: featureTargetCount,
    cores: cpus().length,
    available_parallelism: typeof availableParallelism === 'function' ? availableParallelism() : cpus().length,
    ready_independent_tasks: schedulingValid ? scheduling.ready_independent_tasks : null,
    configured_disk_budget_bytes: schedulingValid ? scheduling.disk_budget_bytes : null,
    per_track_target_estimate_bytes: schedulingValid ? scheduling.per_track_target_estimate_bytes : null,
    computed_k: computedK,
    admissible_new_tracks: computedK === null ? null : watermarks.reclamation_required ? 0 : computedK,
    watermarks,
  };

  // Engine evidence, only for a still-valid inventory and only for completed tracks that still
  // hold a listed worktree. A dry run that refuses every row is still recorded verbatim.
  const commands = [];
  const decisions = new Map();
  const engineRecords = new Map();
  if (refusals.length === 0) {
    for (const fact of sorted(trackFacts, candidate => candidate.track.track_id)) {
      if (fact.track.state !== 'completed' || !fact.worktree.listed) continue;
      const record = await invokeEngine({
        mainRoot: repoRoot,
        workflowId: options.workflow,
        harnessDir: options.harness,
        worktreePath: fact.worktree.path,
        environment,
      });
      commands.push(record);
      engineRecords.set(fact.track.track_id, record);
      decisions.set(fact.track.track_id, await engineDecision(record, fact.track.worktree_key));
      if (record.spawn_error !== null) {
        refuse('sweeper.refuse.engine-unavailable', `engine cleanup could not be executed (${record.spawn_error})`);
      } else if (record.exit_code === EXIT_INVALID) {
        refuse('sweeper.refuse.engine-usage', `engine cleanup rejected the invocation for ${fact.worktree.path}`);
      } else if (record.exit_code !== 0) {
        refuse('sweeper.refuse.engine-probe', `engine cleanup could not probe ${fact.worktree.path} (exit ${record.exit_code})`);
      }
    }
  }
  const blocked = refusals.length > 0;

  const tracks = [];
  for (const fact of sorted(trackFacts, candidate => candidate.track.track_id)) {
    const { track } = fact;
    const actions = blocked
      ? [{ order: 1, kind: 'refused', ref: track.worktree_path, verdict: 'refuse', reason: 'sweeper.refuse.inventory-blocked' }]
      : buildTrackActions(fact, decisions.get(track.track_id) ?? null);
    const reasons = exitReasons({ track_id: track.track_id, state: track.state, producer_stopped: track.producer_stopped, target: fact.target, temporary_paths: fact.temporary_paths, worktree: fact.worktree });
    tracks.push({
      track_id: track.track_id,
      plan_id: track.plan_id,
      state: track.state,
      producer_stopped: track.producer_stopped,
      branch: track.branch,
      worktree: fact.worktree,
      target: fact.target,
      temporary_paths: fact.temporary_paths,
      ownership: track.claim === null ? null : { workflow_id: options.workflow, plan_id: track.claim.plan_id, source: track.claim.source },
      actions,
      engine: engineRecords.get(track.track_id) ?? null,
      exit_clean: reasons.length === 0,
    });
  }

  const checkTrack = mode === 'check-exit' ? tracks.find(track => track.track_id === options.checkExit) ?? null : null;
  if (mode === 'check-exit' && checkTrack === null) {
    refuse('sweeper.refuse.unknown-track', `--check-exit names track ${JSON.stringify(options.checkExit)} that the inventory does not declare`, EXIT_INVALID);
  }
  let checks = null;
  if (mode !== 'dry-run' && refusals.length === 0) {
    const reasons = mode === 'check-exit'
      ? exitReasons(checkTrack)
      : [
        ...sorted(tracks, track => track.track_id).flatMap(track => exitReasons(track)),
        ...extraWorktrees.map(worktree => ({ code: 'sweeper.check.worktree-extra', detail: `Git still lists non-protected worktree ${worktree.path}` })),
        ...unknownPaths.filter(entry => entry.verdict === 'unknown').map(entry => ({ code: 'sweeper.check.unknown-path', detail: `unclaimed footprint ${entry.path} remains` })),
      ];
    checks = { mode, track_id: mode === 'check-exit' ? options.checkExit : null, passed: reasons.length === 0, reasons };
  }

  const exitCode = refusals.length > 0
    ? Math.max(...refusals.map(entry => entry.exit_code))
    : checks !== null && !checks.passed ? EXIT_FACTS : EXIT_OK;
  return {
    document: {
      version: INVENTORY_VERSION,
      workflow_id: options.workflow,
      mode,
      capacity,
      protected_checkouts: protectedCheckouts,
      tracks,
      unknown_paths: sorted(unknownPaths, entry => `${entry.kind}:${entry.path}`),
      commands,
      ok: refusals.length === 0 && (checks === null || checks.passed),
      refusals,
      checks,
    },
    exitCode,
  };
}

// --- entry ---------------------------------------------------------------------------------

async function main(argv) {
  let options;
  try {
    options = parseArgs(argv);
  } catch (error) {
    if (!(error instanceof UsageError)) throw error;
    process.stderr.write(`worktree-sweep: ${error.message}\n\n${usage()}`);
    return EXIT_INVALID;
  }
  if (options.help) {
    process.stdout.write(usage());
    return EXIT_OK;
  }
  const { document, exitCode } = await sweepWorktreeInventory(options);
  process.stdout.write(`${JSON.stringify(document, null, 2)}\n`);
  return exitCode;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main(process.argv.slice(2)).then(
    code => {
      process.exitCode = code;
    },
    error => {
      process.stderr.write(`worktree-sweep: ${error?.stack ?? error}\n`);
      process.exitCode = EXIT_FACTS;
    },
  );
}
