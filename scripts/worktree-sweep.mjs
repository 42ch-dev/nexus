#!/usr/bin/env node
/**
 * Authority, deliberately narrow:
 *   * The inventory is a NON-authoritative ownership receipt for one scheduling checkpoint.
 *     The workflow snapshot stays the only claim source, and `mstar-harness worktree cleanup`
 *     stays the only mechanism allowed to delete a worktree or a branch.
 *   * The dry run proposes; it never authorizes. `--apply` reclaims only a completed track whose
 *     snapshot release and repository ancestry are re-proved at that moment, whose producer has
 *     stopped, and whose exact worktree removal the installed engine itself still permits. The full
 *     re-verification gate runs immediately before EVERY mutating step — each target or receipt
 *     removal, the engine handover, and the non-force fallback removal — and re-proves the same
 *     facts there (re-canonicalised temp/cache root, exact identity, owned and non-symlink and
 *     in-root containment, a linked checkout under `<repo>/.worktrees/<name>` on its declared
 *     branch, not main or protected), so a fact that changed since the planning pass aborts that
 *     step and every later one fail-closed, with zero further mutation. The same re-read also
 *     covers the AUTHORIZATION facts — the snapshot's plan status, execution lease, branch claim
 *     pair and retained worktree path, the repository ancestry, and the receipt's producer/state —
 *     which are proof of one moment and are read from disk again immediately before each of those
 *     steps, never carried over from the planning pass. The sibling declarations are re-read there
 *     too, because the plan's foreign-claim projection is a planning-pass fact as well: a peer that
 *     starts claiming this track's branch, worktree or feature target only afterwards aborts the
 *     step (`sweeper.refuse.foreign-claim`), and a sibling whose declaration became unreadable
 *     withholds it (`sweeper.refuse.sibling-snapshot-unreadable`).
 *   * Ownership is proven, never inferred from a branch claim. A receipt is non-authoritative: a
 *     track's worktree is owned when a live `git worktree list` shows it as this repository's linked
 *     checkout of that path on the claimed branch, and when no such record exists — an absent
 *     worktree, which is the idempotent-retry case — the workflow snapshot itself must retain that
 *     exact path for that plan row (`sweeper.refuse.unproven-ownership` otherwise). A branch-claim
 *     match alone never authorizes the deletion of a target or a temporary receipt, and the engine's
 *     own authorization stays an independent gate on top of the proof rather than a substitute for
 *     it.
 *   * A track's mutations are a chain: each step reports whether it COMPLETED, and the next step
 *     runs only while the previous one did. The first refusal or gate failure therefore ends that
 *     track's mutations there — later receipts are untouched, the engine is never handed the
 *     worktree, the prune is skipped and no branch is asked for — and the row that step appended
 *     carries that step's `kind`/`ref`/`reason`/`detail`, so the aggregate names exactly which step
 *     stopped what. One continuation rule, no per-step exception.
 *   * An inventory track's `worktree` is that track's canonical `<repo>/.worktrees/<name>` linked
 *     checkout of the inventory's repository, on the branch the snapshot claims for it. That shape
 *     is part of the version-1 inventory contract, not a preference, and ONE predicate decides it:
 *     the dry run refuses any other shape at reconciliation (`sweeper.refuse.non-canonical-worktree`)
 *     exactly as `--apply` refuses it at action time (`sweeper.refuse.reverify-worktree`), so a
 *     proposal never implies authorization for a path the action path would refuse.
 *   * This script deletes only the exact scoped target/temporary footprint of such a track and
 *     never a wildcard, never the shared canonical cache, never a branch, and never with a force
 *     flag. It never writes to the snapshot/register, never scans all home directories and never
 *     repairs a fault.
 *   * Snapshot data is projected through an allowlist: session ids, lease holders and session
 *     labels are never copied into output or diagnostics.
 *   * MEASUREMENT CONVENTION: every size this tool reports (`capacity.aggregate_feature_target_bytes`,
 *     `capacity.root_free_bytes`, an enumerated ignored footprint) is a sum of APPARENT byte sizes
 *     read with `lstat` per path — symlinks are never followed and never counted, and every hardlink
 *     is counted once per link. Hardlink-rich build trees therefore read higher than a block-based
 *     `du -sh` would, which is the conservative direction for the 120 GiB feature-target watermark:
 *     it can demand reclamation earlier, never later. `root_free_bytes` is the real filesystem
 *     `statfs` availability, not an apparent-size sum.
 *   * A worktree whose index holds submodule gitlinks is reported as `blocked`, not as
 *     permission: on this repository a non-forced `git worktree remove` is inadmissible there,
 *     and the refusal survives `git submodule deinit --all` (measured by PM on Git 2.54), which
 *     unregisters the submodule in the SHARED superproject config. Each worktree therefore
 *     reports its measured gitlink count and how many initialized submodule checkouts point at an
 *     unresolvable gitdir — the diagnosis for this repository's linked checkouts. On that exact
 *     measured refusal `--apply` records the refusal verbatim and takes the documented non-force
 *     route (`rm -rf <exact worktree path>` + `git worktree prune`, then re-observes). That route
 *     removes the checkout the engine refused AS IT STOOD, so it re-measures the working tree
 *     immediately before its own `rm -rf` — any tracked modification, and any untracked path that is
 *     not ignored (`sweeper.refuse.reverify-submodule-fallback`) — with the same discipline the
 *     ignored-only route applies below. This script never deinitializes and never forces.
 *   * The engine's own refusal is never overridden silently. That measured submodule refusal and a
 *     `cleanup.refuse.dirty-worktree` refusal that is purely an ignored build-output footprint
 *     (tracked tree clean, no untracked non-ignored path, owner merged and released, and the exact
 *     ignored paths with their measured sizes enumerated through the NUL-delimited porcelain form,
 *     which is the only spelling in which Git never C-quotes a path) are both reported as `blocked`
 *     with that evidence, and only then reclaimed through the documented exact-path non-force route.
 *     The ignored-only footprint is a measurement of one moment, never a permission: it is
 *     re-derived again immediately before the fallback removal (engine re-asked, tracked tree and
 *     untracked/ignored paths re-enumerated, every enumerated path re-measured as present) and must
 *     still hold, or the removal is refused (`sweeper.refuse.reverify-ignored-only`) fail-closed
 *     with the facts just measured. Genuine tracked or untracked dirt stays a refusal that mutates
 *     nothing — including dirt that arrived after the first classification.
 *
 * Exit codes: 0 valid dry run / passing check / every requested completed track fully reclaimed;
 * 1 unreadable facts (snapshot, sibling declarations, git, engine, path), a failing requested
 * check, a failed reclamation, or a requested completed track still owning an artifact;
 * 2 invalid invocation or inventory.
 */
import { execFile } from 'node:child_process';
import { lstat, readFile, readdir, realpath, statfs } from 'node:fs/promises';
import { availableParallelism, cpus, homedir, tmpdir } from 'node:os';
import { basename, dirname, isAbsolute, join, normalize, resolve, sep } from 'node:path';
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
const ENGINE_SUBMODULE_REFUSAL = 'working trees containing submodules cannot be moved or removed';
const ENGINE_DIRTY_REFUSAL = 'cleanup.refuse.dirty-worktree';
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
 * A path already spelled through the canonical root is a plain prefix slice; any other spelling is
 * canonicalized one prefix at a time from the filesystem root, and the FIRST prefix that lands on
 * or inside `root` decides the outcome: landing exactly on `root` yields the remaining lexical
 * tail, while landing strictly inside it means the spelling entered the root's interior before it
 * named the root — it crossed a link below the root — and is refused here without walking further.
 * Deciding on the first entry is what makes the rule unshort-circuitable: a later prefix that
 * happens to resolve back to `root` cannot resurrect a receipt whose earlier prefix already
 * resolved inside it, and no component below the root can be followed for an accepted receipt
 * because reaching such a component first requires a prefix that resolves inside the root.
 * `tail` is null when the path never lands on the root, which includes the root itself, any path
 * whose alias prefix does not exist, and every path that enters the root's interior first;
 * `unreadable` carries the failure code of a canonical walk that failed for a reason other than
 * `ENOENT`.
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
    if (isWithin(root, canonical)) return { tail: null, unreadable: null };
  }
  return { tail: null, unreadable: null };
}

async function canonicalWithin(parent, child) {
  const root = await pathKey(parent);
  if (child.split(sep).includes('..')) return { within: false, unreadable: null };
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

async function canonicalWorktreeShape(path, repoRoot) {
  const parent = dirname(path);
  if (basename(parent) !== '.worktrees') {
    return { ok: false, detail: `${path} is not a <repo>/.worktrees/<name> path (its parent is ${parent})` };
  }
  const parentKey = await pathKey(parent);
  const expectedKey = await pathKey(join(repoRoot, '.worktrees'));
  if (parentKey !== expectedKey) {
    return { ok: false, detail: `${path} is not inside this repository's own .worktrees (${parentKey} is not ${expectedKey})` };
  }
  return { ok: true, detail: null };
}

async function ownershipSpellings(path) {
  return unique([await pathKey(path), join(await pathKey(dirname(path)), basename(path))]);
}

async function retainedOwnership(track, declaration) {
  const plan = declaration.plans.find(candidate => candidate.id === track.plan_id) ?? null;
  if (plan === null) {
    return { proven: false, detail: `the snapshot declares no plan row ${track.plan_id}` };
  }
  if (plan.worktreePaths.length === 0) {
    return { proven: false, detail: `the snapshot plan row ${plan.id} retains no worktree path, so the receipt's path is not owned by it` };
  }
  const receiptSpellings = await ownershipSpellings(track.worktree_path);
  const retainedSpellings = (await Promise.all(plan.worktreePaths.map(path => ownershipSpellings(path)))).flat();
  if (!receiptSpellings.some(spelling => retainedSpellings.includes(spelling))) {
    return { proven: false, detail: `the receipt's worktree ${track.worktree_path} is not one of the path(s) the snapshot row ${plan.id} retains (${plan.worktreePaths.join(', ')})` };
  }
  return { proven: true, detail: null };
}

async function receiptOf(path, trackId) {
  const read = await readInventory(path);
  if (read.error !== undefined) return { error: `the ownership receipt can no longer be read (${read.error})` };
  const raw = read.raw;
  if (!plainRow(raw) || !Array.isArray(raw.tracks)) return { error: 'the ownership receipt no longer carries a tracks array' };
  const entry = raw.tracks.find(candidate => plainRow(candidate) && candidate.track_id === trackId) ?? null;
  if (entry === null) return { error: `the ownership receipt no longer declares track ${trackId}` };
  return { entry };
}

function cap(text) {
  const value = String(text ?? '');
  if (value.length <= CAPTURE_LIMIT) return { text: value, truncated: false };
  return { text: value.slice(0, CAPTURE_LIMIT), truncated: true };
}


function parseArgs(argv) {
  if (argv.length === 1 && (argv[0] === '-h' || argv[0] === '--help')) {
    return { apply: false, checkExit: null, checkConvergence: false, help: true };
  }
  const options = { apply: false, checkExit: null, checkConvergence: false };
  const named = { '--repo': 'repo', '--harness': 'harness', '--workflow': 'workflow', '--inventory': 'inventory' };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '-h' || arg === '--help') {
      throw new UsageError(`${arg} is the standalone non-JSON mode and must be the only argument`);
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
  return options;
}

function usage() {
  return [
    'Usage: node scripts/worktree-sweep.mjs --repo <absolute-main-root> --harness <absolute-control-harness>',
    '       --workflow <id> --inventory <absolute-json> [--apply | --check-exit <track-id> | --check-convergence]',
    '',
    'Worktree/cache sweep: parses the version-1 ownership inventory, reconciles it with the',
    'workflow snapshot and real Git facts, measures capacity, and proposes ordered reclamation',
    'actions. Without --apply nothing is deleted by this script.',
    '',
    'Options:',
    '  --repo <path>        Absolute main checkout root (its Git worktree list is the main root).',
    '  --harness <path>     Absolute control harness directory holding workflows/<id>/snapshot.json.',
    '  --workflow <id>      Workflow id whose snapshot drives claims and protection.',
    '  --inventory <path>   Absolute version-1 ownership receipt document. Each track\'s `worktree`',
    '                       must be the canonical `<repo>/.worktrees/<name>` linked checkout of this',
    '                       repository on its declared branch; any other shape is not a reclaimable',
    '                       feature path: the dry run refuses it (sweeper.refuse.non-canonical-worktree)',
    '                       and `--apply` refuses it at action time (sweeper.refuse.reverify-worktree).',
    '                       The receipt never authorizes a deletion: an absent worktree (the idempotent',
    '                       retry) is owned only when the snapshot row retains that exact path',
    '                       (sweeper.refuse.unproven-ownership), and a branch claim alone never does.',
    '  --check-exit <id>    Read-only: assert this completed track keeps no target/temp/worktree/branch',
    '                       footprint (its declared branch still existing is unreclaimed state).',
    '  --check-convergence  Read-only: assert main + integration are the only worktrees listed and no unclaimed footprint remains.',
    '  --apply              Reclaim the completed, merged, released, producer-stopped tracks: re-verify',
    '                       every fact immediately before acting, remove the exact target/temporary',
    '                       footprint, then hand each worktree/branch to the installed engine (never',
    '                       force). Only on the two measured refusals — submodule gitlinks, or dirt that',
    '                       is purely an enumerated ignored-only build-output footprint — does it take',
    '                       the documented non-force route `rm -rf <exact worktree path>` + prune.',
    '  -h, --help           Print this usage. Help is standalone: mixed with any',
    '                       other argument it is an invalid invocation (exit 2).',
    '',
    'Exit codes: 0 valid dry run, passing check or a fully reclaimed apply; 1 unreadable facts, a',
    'failing check, a failed reclamation or a requested completed track still owning an artifact;',
    '2 invalid invocation/inventory.',
    '',
    'Measurement convention: every reported size is a sum of APPARENT byte sizes read with `lstat`',
    'per path — symlinks are never followed and never counted, and each hardlink is counted once per',
    'link. Hardlink-rich build trees therefore read higher than a block-based `du -sh`, which is the',
    'conservative direction for the 120 GiB feature-target watermark (reclaim earlier, never later);',
    '`capacity.root_free_bytes` is the real `statfs` availability, not an apparent-size sum.',
    '',
    'A child that produced no exit code is classified in `spawn_error`: `timeout` when the bounded',
    '30 s timeout killed it or it died to a signal, a named failure such as `ENOENT` or',
    '`ERR_CHILD_PROCESS_STDIO_MAXBUFFER` otherwise, and `spawn-failed` only when nothing else is',
    'known. Every one of those classifications is fail-closed.',
    '',
    'A worktree whose index holds submodule gitlinks is reported as `blocked` rather than proposed:',
    'a non-forced removal is inadmissible here, and this tool never forces and never runs',
    '`git submodule deinit` (that mutates shared configuration). Recovery uses the exact-path',
    'non-force route above and re-observes the result.',
    '',
  ].join('\n');
}


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


/**
 * A non-`ENOENT` failure — at the path itself, at a directory read, or at a descendant stat —
 * is reported as `unreadable` rather than as absence or as a silently partial total, because an
 * unmeasured footprint must never be certified gone or read as enough free capacity. A descendant
 * that vanishes mid-walk is `ENOENT` and simply contributes no bytes.
 *
 * The total is a sum of APPARENT byte sizes (`lstat().size`), and a hardlinked inode is counted
 * once per link because the links are counted as paths. Build trees are hardlink-heavy, so this
 * reads at or above a block-based `du -sh` — the conservative direction for the feature-target
 * watermark (it can demand reclamation earlier, never later). This convention is documented in
 * `usage()` so an operator comparing the number against `du` knows why the two disagree.
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

function expectedTargetFor(cacheRoot, worktreePath) {
  return join(cacheRoot, `${FEATURE_TARGET_PREFIX}${basename(worktreePath)}`);
}


function spawnFailureOf(error) {
  if (typeof error.code === 'number') return null;
  if (nonEmptyString(error.code)) return String(error.code);
  if (error.killed === true || nonEmptyString(error.signal)) return 'timeout';
  return 'spawn-failed';
}

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
      spawn_error: spawnFailureOf(error),
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
    base_branch: nonEmptyString(branch.base) ? branch.base : undefined,
    integration_branch: nonEmptyString(branch.integration) ? branch.integration : undefined,
    integration_worktree_path: nonEmptyString(doc.integration_worktree_path)
      ? doc.integration_worktree_path
      : nonEmptyString(doc.control_worktree_path) ? doc.control_worktree_path : undefined,
    plans,
  };
}

/**
 * Sibling snapshots are bounded to `<harness>/workflows/<id>/snapshot.json` — never a home scan —
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

async function foreignClaimsOf(siblings, cacheRoot) {
  const branches = new Map();
  const paths = new Map();
  for (const sibling of siblings) {
    for (const plan of sibling.plans) {
      for (const branch of plan.branches) if (!branches.has(branch)) branches.set(branch, sibling.id);
    }
    if (sibling.integration_branch !== undefined && !branches.has(sibling.integration_branch)) {
      branches.set(sibling.integration_branch, sibling.id);
    }
    if (sibling.integration_worktree_path !== undefined) {
      paths.set(await pathKey(sibling.integration_worktree_path), sibling.id);
    }
    for (const plan of sibling.plans) {
      for (const worktreePath of plan.worktreePaths) {
        for (const claimed of [worktreePath, expectedTargetFor(cacheRoot, worktreePath)]) {
          const key = await pathKey(claimed);
          if (!paths.has(key)) paths.set(key, sibling.id);
        }
      }
    }
  }
  return { branches, paths };
}

async function foreignClaimOn(claims, track, targetKey) {
  const branchOwner = claims.branches.get(track.branch);
  if (branchOwner !== undefined) return `branch ${track.branch} is claimed by foreign workflow ${branchOwner}`;
  const pathOwner = claims.paths.get(track.worktree_key) ?? claims.paths.get(targetKey);
  if (pathOwner !== undefined) return `track ${track.track_id} names a path claimed by foreign workflow ${pathOwner}`;
  return null;
}


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
      } else {
        const shape = await canonicalWorktreeShape(entry.worktree, mainRoot);
        if (!shape.ok) {
          refuse('sweeper.refuse.non-canonical-worktree', `track ${entry.track_id} worktree ${entry.worktree} is not a reclaimable feature path: ${shape.detail}`);
        }
        if (onDisk.exists || entry.state === 'active') {
          refuse('sweeper.refuse.stale-worktree', `track ${entry.track_id} declares worktree ${entry.worktree} that Git does not list`);
        }
      }
    } else {
      const shape = await canonicalWorktreeShape(entry.worktree, mainRoot);
      if (!shape.ok) {
        refuse('sweeper.refuse.non-canonical-worktree', `track ${entry.track_id} worktree ${entry.worktree} is not a reclaimable feature path: ${shape.detail}`);
      }
      if (worktree.branch !== entry.branch) {
        refuse('sweeper.refuse.stale-worktree-pair', `worktree ${entry.worktree} has ${worktree.branch} checked out, not the declared ${entry.branch}`);
      }
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


async function runRecorded(file, args, cwd, environment) {
  const record = { argv: [file, ...args], exit_code: null, spawn_error: null, stdout: '', stderr: '', truncated: false };
  try {
    const { stdout, stderr } = await exec(file, args, {
      cwd,
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
    record.spawn_error = spawnFailureOf(error);
    record.stdout = out.text;
    record.stderr = err.text;
    record.truncated = out.truncated || err.truncated;
  }
  return record;
}

/** The exact argv contract: `--workflow --harness --worktree [--apply]`, never a widening flag. */
async function invokeEngine({ mainRoot, workflowId, harnessDir, worktreePath, environment, apply = false }) {
  const args = ['worktree', 'cleanup', '--workflow', workflowId, '--harness', harnessDir, '--worktree', worktreePath];
  if (apply) args.push('--apply');
  return await runRecorded(ENGINE_BINARY, args, mainRoot, environment);
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


async function reverifyTemporary(path, { tempRootKey, cacheRoot, mainRoot }) {
  const currentRoot = await pathKey(tmpdir());
  if (currentRoot !== tempRootKey) {
    return { ok: false, absent: false, reason: 'sweeper.refuse.reverify-temp-root', detail: `the system temp root now canonicalises to ${currentRoot}, not the planned ${tempRootKey}` };
  }
  const containment = await canonicalWithin(currentRoot, path);
  if (containment.unreadable !== null) {
    return { ok: false, absent: false, reason: 'sweeper.refuse.path-unreadable', detail: `temporary ${path} cannot be resolved (${containment.unreadable})` };
  }
  if (!containment.within) {
    return { ok: false, absent: false, reason: 'sweeper.refuse.reverify-temporary', detail: `temporary ${path} is no longer an unlinked receipt path inside the system temp root` };
  }
  if (isWithin(cacheRoot, path) || isWithin(mainRoot, path)) {
    return { ok: false, absent: false, reason: 'sweeper.refuse.reverify-temporary', detail: `temporary ${path} now sits inside a protected shared location` };
  }
  const probe = await describePath(path);
  if (probe.unreadable !== null) {
    return { ok: false, absent: false, reason: 'sweeper.refuse.path-unreadable', detail: `temporary ${path} cannot be read (${probe.unreadable})` };
  }
  if (probe.is_symlink) {
    return { ok: false, absent: false, reason: 'sweeper.refuse.symlink-path', detail: `temporary ${path} became a symlink` };
  }
  return { ok: true, absent: !probe.exists, reason: null, detail: null };
}

async function reverifyOwnedTarget(path, { expectedPath, cacheRootKey, environment }) {
  const resolution = cacheRootFrom(environment);
  if (resolution.error !== undefined) {
    return { ok: false, absent: false, reason: 'sweeper.refuse.cache-root', detail: resolution.error };
  }
  const currentRoot = await pathKey(resolution.path);
  if (currentRoot !== cacheRootKey) {
    return { ok: false, absent: false, reason: 'sweeper.refuse.reverify-cache-root', detail: `the cache root now canonicalises to ${currentRoot}, not the planned ${cacheRootKey}` };
  }
  if (resolve(path) === resolve(join(currentRoot, CANONICAL_TARGET_NAME))) {
    return { ok: false, absent: false, reason: 'sweeper.refuse.protected-target', detail: `target ${path} is the shared canonical cache` };
  }
  if (resolve(path) !== resolve(expectedPath)) {
    return { ok: false, absent: false, reason: 'sweeper.refuse.reverify-target', detail: `target ${path} is no longer the .envrc-derived ${expectedPath}` };
  }
  const containment = await canonicalWithin(currentRoot, path);
  if (containment.unreadable !== null) {
    return { ok: false, absent: false, reason: 'sweeper.refuse.path-unreadable', detail: `target ${path} cannot be resolved (${containment.unreadable})` };
  }
  if (!containment.within) {
    return { ok: false, absent: false, reason: 'sweeper.refuse.reverify-target', detail: `target ${path} is no longer an unlinked path inside the cache root` };
  }
  const probe = await describePath(path);
  if (probe.unreadable !== null) {
    return { ok: false, absent: false, reason: 'sweeper.refuse.path-unreadable', detail: `target ${path} cannot be read (${probe.unreadable})` };
  }
  if (probe.is_symlink) {
    return { ok: false, absent: false, reason: 'sweeper.refuse.symlink-path', detail: `target ${path} became a symlink` };
  }
  return { ok: true, absent: !probe.exists, reason: null, detail: null };
}

async function reverifyWorktree(path, { branch, worktreeKey, repoRoot, mainRoot, protectedKeys, environment }) {
  const probe = await describePath(path);
  if (probe.unreadable !== null) {
    return { ok: false, absent: false, reason: 'sweeper.refuse.path-unreadable', detail: `worktree ${path} cannot be read (${probe.unreadable})` };
  }
  if (probe.is_symlink) {
    return { ok: false, absent: false, reason: 'sweeper.refuse.symlink-path', detail: `worktree ${path} is a symlink` };
  }
  const listing = await runGit(['worktree', 'list', '--porcelain'], repoRoot, environment);
  if (listing.exit_code !== 0) {
    return { ok: false, absent: false, reason: 'sweeper.refuse.git-unreadable', detail: `git worktree list failed in ${repoRoot} (${listing.spawn_error ?? listing.stderr.trim()})` };
  }
  const records = await Promise.all(parseWorktreeList(listing.stdout).map(async record => ({ ...record, key: await pathKey(record.path) })));
  const listed = records.find(record => record.key === worktreeKey) ?? null;
  if (listed === null && !probe.exists) return { ok: true, absent: true, worktree_listed: false, reason: null, detail: null };
  if (listed === null) {
    return { ok: false, absent: false, reason: 'sweeper.refuse.reverify-worktree', detail: `${path} exists on disk but is no longer a linked worktree of this repository` };
  }
  const shape = await canonicalWorktreeShape(path, repoRoot);
  const parentProbe = shape.ok ? await describePath(dirname(path)) : null;
  if (!shape.ok || !parentProbe.exists || parentProbe.is_symlink || parentProbe.unreadable !== null) {
    const detail = shape.ok ? `${path} is not a readable <dir>/.worktrees/<name> linked checkout` : shape.detail;
    return { ok: false, absent: false, reason: 'sweeper.refuse.reverify-worktree', detail };
  }
  if (listed.branch !== branch) {
    return { ok: false, absent: false, reason: 'sweeper.refuse.reverify-worktree', detail: `${path} now has ${listed.branch} checked out, not the declared ${branch}` };
  }
  if (mainRoot !== null && worktreeKey === mainRoot) {
    return { ok: false, absent: false, reason: 'sweeper.refuse.reverify-worktree', detail: `${path} is the main worktree` };
  }
  if (protectedKeys.has(worktreeKey)) {
    return { ok: false, absent: false, reason: 'sweeper.refuse.reverify-worktree', detail: `${path} is a protected checkout` };
  }
  return { ok: true, absent: false, worktree_listed: true, reason: null, detail: null };
}

/** Snapshot release proof: the claiming plan row is terminal and holds no execution lease. */
function releaseProof(track, declaration) {
  const plan = declaration.plans.find(candidate => candidate.id === track.plan_id) ?? null;
  if (plan === null) {
    return { released: false, code: 'sweeper.refuse.not-released', detail: `the snapshot declares no plan row ${track.plan_id}, so the track is not released` };
  }
  if (plan.status !== TERMINAL_PLAN_STATUS) {
    return { released: false, code: 'sweeper.refuse.not-released', detail: `plan ${plan.id} is ${plan.status}, not ${TERMINAL_PLAN_STATUS}, so the track is not released` };
  }
  if (plan.leased) {
    return { released: false, code: 'sweeper.refuse.not-released', detail: `plan ${plan.id} still holds an execution lease` };
  }
  return { released: true, code: null, detail: null };
}

async function mergeProof(track, declaration, repoRoot, environment) {
  const evidenceBase = declaration.integration_branch ?? declaration.base_branch;
  if (!nonEmptyString(evidenceBase)) {
    return { merged: false, code: 'sweeper.refuse.merge-unresolvable', detail: 'the workflow declaration records neither an integration nor a base ref' };
  }
  const branchRef = await runGit(['rev-parse', '--verify', '--quiet', `refs/heads/${track.branch}`], repoRoot, environment);
  if (branchRef.exit_code !== 0) {
    if (branchRef.exit_code === 1 && branchRef.stdout.trim() === '' && branchRef.stderr.trim() === '') {
      return { merged: true, code: null, detail: null };
    }
    return { merged: false, code: 'sweeper.refuse.git-unreadable', detail: `git rev-parse could not read refs/heads/${track.branch} (${branchRef.spawn_error ?? branchRef.stderr.trim()})` };
  }
  const ancestry = await runGit(['merge-base', '--is-ancestor', track.branch, evidenceBase], repoRoot, environment);
  if (ancestry.exit_code === 0) return { merged: true, code: null, detail: null };
  if (ancestry.exit_code === 1) {
    return { merged: false, code: 'sweeper.refuse.unmerged-track', detail: `${track.branch} is not an ancestor of ${evidenceBase}` };
  }
  return { merged: false, code: 'sweeper.refuse.merge-unresolvable', detail: `git merge-base could not decide ${track.branch} against ${evidenceBase} (${ancestry.spawn_error ?? ancestry.stderr.trim()})` };
}

/**
 * `apply: failed worktree <exact path>: fatal: <ENGINE_SUBMODULE_REFUSAL>`. The named path must
 * canonicalise to this track's worktree, so a refusal about any other path cannot authorize a
 * removal here.
 */
async function engineSubmoduleRefusal(record, worktreePath, worktreeKey) {
  const prefix = 'apply: failed worktree ';
  const suffix = `: fatal: ${ENGINE_SUBMODULE_REFUSAL}`;
  for (const line of `${record.stdout}\n${record.stderr}`.split('\n')) {
    const trimmed = line.trim();
    if (!trimmed.startsWith(prefix) || !trimmed.endsWith(suffix)) continue;
    const named = trimmed.slice(prefix.length, trimmed.length - suffix.length);
    if ((await pathKey(named)) === worktreeKey) return { named };
  }
  return null;
}

async function measureIgnoredOnlyFootprint(track, context) {
  const dirty = await runGit(['status', '--porcelain', '--untracked-files=no'], track.worktree_path, context.environment);
  if (dirty.exit_code !== 0) {
    return { ok: false, detail: `git status --untracked-files=no failed in ${track.worktree_path} (${dirty.spawn_error ?? dirty.stderr.trim()})` };
  }
  const trackedChange = dirty.stdout.split('\n').find(line => line !== '') ?? null;
  if (trackedChange !== null) {
    return { ok: false, detail: `the tracked tree of ${track.worktree_path} is not clean (${JSON.stringify(trackedChange)})` };
  }
  const status = await runGit(['status', '--porcelain', '-z', '--untracked-files=normal', '--ignored=traditional'], track.worktree_path, context.environment);
  if (status.exit_code !== 0) {
    return { ok: false, detail: `git status --ignored failed in ${track.worktree_path} (${status.spawn_error ?? status.stderr.trim()})` };
  }
  const ignored = [];
  for (const record of status.stdout.split('\0')) {
    if (record === '') continue;
    if (!record.startsWith('!! ')) {
      return { ok: false, detail: `${track.worktree_path} carries a non-ignored porcelain entry ${JSON.stringify(record)}, which is real dirt` };
    }
    const relative = record.slice(3);
    if (relative === '' || (relative.startsWith('"') && relative.endsWith('"'))) {
      return { ok: false, detail: `the ignored enumeration of ${track.worktree_path} carries an entry this tool cannot decode (${JSON.stringify(record)})` };
    }
    ignored.push(relative);
  }
  if (ignored.length === 0) {
    return { ok: false, detail: `${track.worktree_path} reports no ignored path, so there is no ignored-only footprint to enumerate` };
  }
  const paths = [];
  let total = 0;
  for (const relative of sorted(ignored)) {
    const path = join(track.worktree_path, relative.endsWith(sep) ? relative.slice(0, -1) : relative);
    const measured = await pathBytes(path);
    if (measured.unreadable !== null || measured.is_symlink || !measured.exists) {
      return { ok: false, detail: `the enumerated ignored path ${path} cannot be measured truthfully (exists ${measured.exists}, symlink ${measured.is_symlink}, unreadable ${measured.unreadable})` };
    }
    paths.push({ path, bytes: measured.bytes });
    total += measured.bytes;
  }
  return { ok: true, footprint: { paths, total_bytes: total } };
}

async function measureWorktreeChanges(track, context) {
  const status = await runGit(['status', '--porcelain', '-z', '--untracked-files=normal'], track.worktree_path, context.environment);
  if (status.exit_code !== 0) {
    return { ok: false, detail: `git status failed in ${track.worktree_path} (${status.spawn_error ?? status.stderr.trim()})` };
  }
  const change = status.stdout.split('\0').find(record => record !== '') ?? null;
  if (change === null) return { ok: true, detail: null };
  return { ok: false, detail: `${track.worktree_path} carries a working-tree change that arrived after the engine's refusal (${JSON.stringify(change)})` };
}

/** `git worktree prune --dry-run --verbose` names each stale record it would drop. */
function prunableWorktreeNames(text) {
  const names = [];
  for (const line of String(text).split('\n')) {
    const match = /^Removing worktrees\/(.+?): /.exec(line.trim());
    if (match !== null) names.push(match[1]);
  }
  return names;
}

async function pruneScopedWorktree({ name, context, actions }) {
  const dry = await runRecorded('git', ['worktree', 'prune', '--dry-run', '--verbose'], context.repoRoot, context.environment);
  context.commands.push(dry);
  if (dry.spawn_error !== null || dry.exit_code !== 0) {
    const detail = `git worktree prune --dry-run failed (${dry.spawn_error ?? dry.stderr.trim()})`;
    addAction(actions, 'prune', context.repoRoot, 'refuse', 'sweeper.refuse.git-unreadable', detail);
    return false;
  }
  const names = prunableWorktreeNames(`${dry.stdout}\n${dry.stderr}`);
  const foreign = unique(names.filter(candidate => candidate !== name));
  if (foreign.length > 0) {
    const detail = `git worktree prune would also drop foreign stale record(s): ${sorted(foreign).join(', ')}`;
    addAction(actions, 'prune', context.repoRoot, 'refuse', 'sweeper.refuse.prune-foreign', detail);
    return false;
  }
  if (!names.includes(name)) {
    addAction(actions, 'prune', context.repoRoot, 'absent', 'sweeper.absent.idempotent');
    return true;
  }
  const actual = await runRecorded('git', ['worktree', 'prune'], context.repoRoot, context.environment);
  context.commands.push(actual);
  if (actual.spawn_error !== null || actual.exit_code !== 0) {
    const detail = `git worktree prune failed (${actual.spawn_error ?? actual.stderr.trim()})`;
    addAction(actions, 'prune', context.repoRoot, 'refuse', 'sweeper.refuse.git-unreadable', detail);
    return false;
  }
  addAction(actions, 'prune', context.repoRoot, 'executed', 'sweeper.executed.prune');
  return true;
}

async function reclaimOwnedPath({ path, kind, context, actions, reverify }) {
  const check = await reverify();
  if (!check.ok) {
    addAction(actions, kind, path, 'refuse', check.reason, check.detail);
    return { completed: false, mutated: false };
  }
  if (check.absent) {
    addAction(actions, kind, path, 'absent', 'sweeper.absent.idempotent');
    return { completed: true, mutated: false };
  }
  const removal = await runRecorded('rm', ['-rf', path], context.repoRoot, context.environment);
  context.commands.push(removal);
  const after = await describePath(path);
  if (removal.spawn_error !== null || removal.exit_code !== 0 || after.exists || after.unreadable !== null) {
    const state = after.unreadable !== null ? `unreadable (${after.unreadable})` : after.exists ? 'still present' : 'gone';
    const detail = `rm -rf ${path} exited ${removal.exit_code ?? removal.spawn_error} and the path is ${state}`;
    addAction(actions, kind, path, 'refuse', 'sweeper.refuse.remove-failed', detail);
    return { completed: false, mutated: true };
  }
  addAction(actions, kind, path, 'executed', `sweeper.executed.${kind}`);
  return { completed: true, mutated: true };
}

async function nonForceWorktreeRemoval({ track, context, actions, gate, refusal }) {
  const path = track.worktree_path;
  const blocked = addAction(actions, 'engine-worktree-removal', path, 'blocked', refusal.reason, refusal.detail);
  if (refusal.footprint !== undefined) blocked.ignored_footprint = refusal.footprint;
  const check = await gate();
  if (!check.ok) {
    addAction(actions, 'fallback-worktree-removal', path, 'refuse', check.reason, check.detail);
    return;
  }
  if (check.worktree_absent) {
    addAction(actions, 'fallback-worktree-removal', path, 'absent', 'sweeper.absent.idempotent');
    return;
  }
  const removal = await runRecorded('rm', ['-rf', path], context.repoRoot, context.environment);
  context.commands.push(removal);
  const after = await describePath(path);
  if (removal.spawn_error !== null || removal.exit_code !== 0 || after.exists || after.unreadable !== null) {
    const state = after.unreadable !== null ? `unreadable (${after.unreadable})` : after.exists ? 'still present' : 'gone';
    const detail = `rm -rf ${path} exited ${removal.exit_code ?? removal.spawn_error} and the path is ${state}`;
    addAction(actions, 'fallback-worktree-removal', path, 'refuse', 'sweeper.refuse.remove-failed', detail);
    return;
  }
  addAction(actions, 'fallback-worktree-removal', path, 'executed', 'sweeper.executed.non-force-remove');
  if (!(await pruneScopedWorktree({ name: basename(path), context, actions }))) return;

  const release = await invokeEngine({
    mainRoot: context.repoRoot,
    workflowId: context.options.workflow,
    harnessDir: context.options.harness,
    worktreePath: path,
    environment: context.environment,
    apply: true,
  });
  context.commands.push(release);
  const branchRef = await runGit(['rev-parse', '--verify', '--quiet', `refs/heads/${track.branch}`], context.repoRoot, context.environment);
  if (branchRef.exit_code === 0) {
    addAction(
      actions,
      'engine-branch-removal',
      track.branch,
      'retained',
      'sweeper.retained.branch-unreachable',
      `the engine declines branch candidates for an exact path whose worktree record is pruned (its raw output is recorded in commands[]), and never force-deletes a branch; release ${track.branch} from the workflow-level cleanup checkpoint`,
    );
    return;
  }
  if (branchRef.exit_code === 1 && branchRef.stdout.trim() === '' && branchRef.stderr.trim() === '') {
    addAction(actions, 'engine-branch-removal', track.branch, 'absent', 'sweeper.absent.idempotent');
    return;
  }
  const detail = `git rev-parse could not read refs/heads/${track.branch} (${branchRef.spawn_error ?? branchRef.stderr.trim()})`;
  addAction(actions, 'engine-branch-removal', track.branch, 'refuse', 'sweeper.refuse.git-unreadable', detail);
}

const ACTION_KIND_ORDER = ['reclaim-target', 'reclaim-temporary', 'engine-worktree-removal', 'fallback-worktree-removal', 'prune', 'engine-branch-removal', 'reclaim-footprint', 'protected'];

function orderActions(actions) {
  const rank = action => {
    const index = ACTION_KIND_ORDER.indexOf(action.kind);
    return index === -1 ? ACTION_KIND_ORDER.length : index;
  };
  return [...actions].sort((a, b) => rank(a) - rank(b)).map((action, index) => ({ ...action, order: index + 1 }));
}

async function applyTrack(fact, context) {
  const { track } = fact;
  const actions = [];
  const engineRecords = [];
  const refuseAll = (code, detail) => {
    addAction(actions, 'reclaim-footprint', track.worktree_path, 'refuse', code, detail);
    return { actions, engineRecords, applied: false };
  };

  if (track.state !== 'completed') {
    addAction(actions, 'protected', track.worktree_path, 'protected', 'sweeper.protected.track-active');
    return { actions, engineRecords, applied: false };
  }
  if (track.producer_stopped !== true) {
    return refuseAll('sweeper.refuse.producer-running', `track ${track.track_id} holds no producer-stopped receipt, so its footprint is not a reclaimable slice`);
  }
  const reclaimable = fact.target.exists === true || fact.temporary_paths.some(temporary => temporary.exists === true);
  if (reclaimable && fact.worktree.listed === false && fact.ownership.proven !== true) {
    return refuseAll('sweeper.refuse.unproven-ownership', `track ${track.track_id} is not reclaimable: no live Git record proves its worktree and ${fact.ownership.detail}`);
  }
  const release = releaseProof(track, context.declaration);
  if (!release.released) return refuseAll(release.code, release.detail);
  const merge = await mergeProof(track, context.declaration, context.repoRoot, context.environment);
  if (!merge.merged) return refuseAll(merge.code, merge.detail);

  const worktreeGate = () => reverifyWorktree(track.worktree_path, {
    branch: track.branch,
    worktreeKey: track.worktree_key,
    repoRoot: context.repoRoot,
    mainRoot: context.mainRoot,
    protectedKeys: context.protectedWorktreeKeys,
    environment: context.environment,
  });
  const targetGate = () => reverifyOwnedTarget(fact.target.path, {
    expectedPath: fact.target.expected_path,
    cacheRootKey: context.cacheRootKey,
    environment: context.environment,
  });
  const footprintGate = async () => {
    const worktreeCheck = await worktreeGate();
    if (!worktreeCheck.ok) return worktreeCheck;
    const targetCheck = await targetGate();
    if (!targetCheck.ok) return targetCheck;
    return {
      ok: true,
      worktree_absent: worktreeCheck.absent,
      target_absent: targetCheck.absent,
      worktree_listed: worktreeCheck.worktree_listed === true,
      reason: null,
      detail: null,
    };
  };
  const gated = ownCheck => async () => {
    const footprint = await footprintGate();
    if (!footprint.ok) return footprint;
    return { ...(await ownCheck()), worktree_listed: footprint.worktree_listed };
  };
  const targetRemovalGate = async () => {
    const footprint = await footprintGate();
    if (!footprint.ok) return footprint;
    return { ok: true, absent: footprint.target_absent, worktree_listed: footprint.worktree_listed, reason: null, detail: null };
  };
  /**
   * The sibling declarations are re-read in the same breath, because the plan's foreign-claim
   * projection is a fact of the planning pass too: a peer that claims this track's branch, worktree
   * or target only AFTER the inventory was reconciled is not in that projection, and the step about
   * to delete such a path is exactly the deletion the plan never authorized.
   */
  const authorizationGate = async worktreeListed => {
    const declarations = await readDeclarations(context.options.harness, context.options.workflow);
    if (declarations.error !== undefined) {
      return { ok: false, reason: 'sweeper.refuse.snapshot-unreadable', detail: `the workflow declaration can no longer be read (${declarations.error})` };
    }
    const unreadableSibling = (declarations.unreadable ?? [])[0] ?? null;
    if (unreadableSibling !== null) {
      return { ok: false, reason: 'sweeper.refuse.sibling-snapshot-unreadable', detail: `sibling workflow ${unreadableSibling.workflow_id}: ${unreadableSibling.detail} — its claims are unknown, so this step is withheld` };
    }
    const fresh = declarations.own;
    if (fresh.id !== context.declaration.id) {
      return { ok: false, reason: 'sweeper.refuse.snapshot-identity', detail: `the workflow declaration now reports id ${fresh.id}, not the planned ${context.declaration.id}` };
    }
    const claims = await foreignClaimsOf(declarations.siblings ?? [], context.cacheRoot);
    const foreign = await foreignClaimOn(claims, track, await pathKey(track.target_path));
    if (foreign !== null) {
      return { ok: false, reason: 'sweeper.refuse.foreign-claim', detail: `re-read immediately before this step: ${foreign}` };
    }
    const claimers = branchClaimers(fresh, track.branch);
    if (claimers.length !== 1 || claimers[0].plan_id !== track.plan_id) {
      return { ok: false, reason: 'sweeper.refuse.stale-branch-claim', detail: `the re-read snapshot no longer claims branch ${track.branch} for plan ${track.plan_id} alone` };
    }
    const release = releaseProof(track, fresh);
    if (!release.released) {
      return { ok: false, reason: release.code, detail: `re-read immediately before this step: ${release.detail}` };
    }
    if (worktreeListed !== true) {
      const ownership = await retainedOwnership(track, fresh);
      if (!ownership.proven) {
        return { ok: false, reason: 'sweeper.refuse.unproven-ownership', detail: `re-read immediately before this step: ${ownership.detail}` };
      }
    }
    const merge = await mergeProof(track, fresh, context.repoRoot, context.environment);
    if (!merge.merged) {
      return { ok: false, reason: merge.code, detail: `re-read immediately before this step: ${merge.detail}` };
    }
    const receipt = await receiptOf(context.options.inventory, track.track_id);
    if (receipt.error !== undefined) {
      return { ok: false, reason: 'sweeper.refuse.inventory-unreadable', detail: receipt.error };
    }
    if (receipt.entry.state !== 'completed') {
      return { ok: false, reason: 'sweeper.refuse.track-not-completed', detail: `re-read immediately before this step: the receipt now declares track ${track.track_id} as ${JSON.stringify(receipt.entry.state)}` };
    }
    if (receipt.entry.producer_stopped !== true) {
      return { ok: false, reason: 'sweeper.refuse.producer-running', detail: `re-read immediately before this step: the receipt no longer reports a stopped producer for track ${track.track_id}` };
    }
    return { ok: true, reason: null, detail: null };
  };
  const authorized = gate => async () => {
    const check = await gate();
    if (!check.ok) return check;
    if (check.absent === true || check.worktree_absent === true) return check;
    const listed = check.worktree_listed ?? fact.worktree.listed;
    const facts = await authorizationGate(listed);
    if (!facts.ok) return { ok: false, absent: false, reason: facts.reason, detail: facts.detail };
    return check;
  };
  const ignoredOnlyGate = async () => {
    const footprint = await footprintGate();
    if (!footprint.ok || footprint.worktree_absent) return footprint;
    const probe = await invokeEngine({
      mainRoot: context.repoRoot,
      workflowId: context.options.workflow,
      harnessDir: context.options.harness,
      worktreePath: track.worktree_path,
      environment: context.environment,
    });
    context.commands.push(probe);
    if (probe.spawn_error !== null) {
      return { ok: false, absent: false, reason: 'sweeper.refuse.engine-unavailable', detail: `engine cleanup could not be executed to re-derive the ignored-only classification (${probe.spawn_error})` };
    }
    if (probe.exit_code === EXIT_INVALID) {
      return { ok: false, absent: false, reason: 'sweeper.refuse.engine-usage', detail: `engine cleanup rejected the invocation for ${track.worktree_path}` };
    }
    if (probe.exit_code !== 0) {
      return { ok: false, absent: false, reason: 'sweeper.refuse.engine-probe', detail: `engine cleanup could not probe ${track.worktree_path} (exit ${probe.exit_code})` };
    }
    const decision = await engineDecision(probe, track.worktree_key);
    if (decision.reason !== ENGINE_DIRTY_REFUSAL) {
      return { ok: false, absent: false, reason: 'sweeper.refuse.reverify-ignored-only', detail: `the installed engine no longer refuses ${track.worktree_path} with ${ENGINE_DIRTY_REFUSAL} (raw row reason ${decision.reason}), so the ignored-only classification is stale` };
    }
    const measured = await measureIgnoredOnlyFootprint(track, context);
    if (!measured.ok) {
      return { ok: false, absent: false, reason: 'sweeper.refuse.reverify-ignored-only', detail: `the ignored-only classification of ${track.worktree_path} no longer holds: ${measured.detail}` };
    }
    return { ok: true, worktree_absent: false, target_absent: footprint.target_absent, worktree_listed: footprint.worktree_listed, reason: null, detail: null };
  };
  const submoduleFallbackGate = async () => {
    const footprint = await footprintGate();
    if (!footprint.ok || footprint.worktree_absent) return footprint;
    const changes = await measureWorktreeChanges(track, context);
    if (!changes.ok) {
      return { ok: false, absent: false, reason: 'sweeper.refuse.reverify-submodule-fallback', detail: changes.detail };
    }
    return { ok: true, worktree_absent: false, target_absent: footprint.target_absent, worktree_listed: footprint.worktree_listed, reason: null, detail: null };
  };

  let ignoredOnly = null;
  if (fact.worktree.listed) {
    const probe = await invokeEngine({
      mainRoot: context.repoRoot,
      workflowId: context.options.workflow,
      harnessDir: context.options.harness,
      worktreePath: track.worktree_path,
      environment: context.environment,
    });
    context.commands.push(probe);
    engineRecords.push(probe);
    if (probe.spawn_error !== null) {
      return refuseAll('sweeper.refuse.engine-unavailable', `engine cleanup could not be executed (${probe.spawn_error})`);
    }
    if (probe.exit_code === EXIT_INVALID) {
      return refuseAll('sweeper.refuse.engine-usage', `engine cleanup rejected the invocation for ${track.worktree_path}`);
    }
    if (probe.exit_code !== 0) {
      return refuseAll('sweeper.refuse.engine-probe', `engine cleanup could not probe ${track.worktree_path} (exit ${probe.exit_code})`);
    }
    const decision = await engineDecision(probe, track.worktree_key);
    if (decision.verdict !== 'propose') {
      if (decision.reason === ENGINE_DIRTY_REFUSAL) {
        const measured = await measureIgnoredOnlyFootprint(track, context);
        if (measured.ok) {
          ignoredOnly = measured.footprint;
        } else {
          return refuseAll(
            decision.reason,
            `the installed engine does not permit removing ${track.worktree_path} and the dirt it reports is not an ignored-only footprint (raw row reason ${decision.reason}; ${measured.detail}); nothing is reclaimed`,
          );
        }
      } else {
        return refuseAll(decision.reason, `the installed engine does not permit removing ${track.worktree_path} (raw row reason ${decision.reason})`);
      }
    }
  }

  let worktreeRemains = false;
  if (!fact.worktree.listed) {
    addAction(actions, 'engine-worktree-removal', track.worktree_path, 'absent', 'sweeper.absent.idempotent');
  } else {
    const worktreeCheck = await worktreeGate();
    if (!worktreeCheck.ok) {
      addAction(actions, 'engine-worktree-removal', track.worktree_path, 'refuse', worktreeCheck.reason, worktreeCheck.detail);
      return { actions, engineRecords, applied: false };
    }
    worktreeRemains = !worktreeCheck.absent;
    if (!worktreeRemains) addAction(actions, 'engine-worktree-removal', track.worktree_path, 'absent', 'sweeper.absent.idempotent');
  }

  // The target's own gate carries the whole footprint, so a worktree or cache-root fact that moved
  // since planning stops the removal instead of acting on the stale plan.
  //
  // The steps run ONLY while the previous one completed: `reclaimOwnedPath` reports whether its step
  // finished (removed, or idempotently absent), and the first step that did not returns from this
  // track there — the later receipts are not touched, the engine is never handed the worktree, and
  // the row that step appended names it (`kind` + `ref`) and what it refused. One continuation rule
  // for every step, no per-caller special case.
  let mutated = false;
  const steps = [
    { kind: 'reclaim-target', path: fact.target.path, reverify: authorized(targetRemovalGate) },
    ...fact.temporary_paths.map(temporary => ({
      kind: 'reclaim-temporary',
      path: temporary.path,
      reverify: authorized(gated(() => reverifyTemporary(temporary.path, { tempRootKey: context.tempRootKey, cacheRoot: context.cacheRoot, mainRoot: context.mainRoot }))),
    })),
  ];
  for (const step of steps) {
    const result = await reclaimOwnedPath({ ...step, context, actions });
    mutated = mutated || result.mutated;
    if (!result.completed) return { actions, engineRecords, applied: mutated };
  }
  if (!worktreeRemains) return { actions, engineRecords, applied: true };

  const beforeEngine = await authorized(footprintGate)();
  if (!beforeEngine.ok) {
    addAction(actions, 'engine-worktree-removal', track.worktree_path, 'refuse', beforeEngine.reason, beforeEngine.detail);
    return { actions, engineRecords, applied: true };
  }
  if (beforeEngine.worktree_absent) {
    addAction(actions, 'engine-worktree-removal', track.worktree_path, 'absent', 'sweeper.absent.idempotent');
    return { actions, engineRecords, applied: true };
  }

  const applied = await invokeEngine({
    mainRoot: context.repoRoot,
    workflowId: context.options.workflow,
    harnessDir: context.options.harness,
    worktreePath: track.worktree_path,
    environment: context.environment,
    apply: true,
  });
  context.commands.push(applied);
  engineRecords.push(applied);
  if (applied.spawn_error !== null) {
    addAction(actions, 'engine-worktree-removal', track.worktree_path, 'refuse', 'sweeper.refuse.engine-unavailable', `engine cleanup --apply could not be executed (${applied.spawn_error})`);
    return { actions, engineRecords, applied: true };
  }
  const afterEngine = await worktreeGate();
  if (!afterEngine.ok) {
    addAction(actions, 'engine-worktree-removal', track.worktree_path, 'refuse', afterEngine.reason, afterEngine.detail);
    return { actions, engineRecords, applied: true };
  }
  if (!afterEngine.absent) {
    const refusal = await engineSubmoduleRefusal(applied, track.worktree_path, track.worktree_key);
    if (refusal !== null && fact.worktree.submodule_gitlinks > 0) {
      await nonForceWorktreeRemoval({
        track,
        context,
        actions,
        gate: authorized(submoduleFallbackGate),
        refusal: {
          reason: 'sweeper.blocked.submodule-gitlinks',
          detail: `the installed engine refused the non-force removal with "${ENGINE_SUBMODULE_REFUSAL}"; taking the documented exact-path route`,
        },
      });
      return { actions, engineRecords, applied: true };
    }
    if (ignoredOnly !== null) {
      await nonForceWorktreeRemoval({
        track,
        context,
        actions,
        gate: authorized(ignoredOnlyGate),
        refusal: {
          reason: 'sweeper.blocked.ignored-outputs',
          detail: `the installed engine refused ${track.worktree_path} with ${ENGINE_DIRTY_REFUSAL} while its tracked tree is clean: the ignored-only footprint is ${ignoredOnly.paths.length} enumerated path(s) totalling ${ignoredOnly.total_bytes} bytes; taking the documented exact-path route`,
          footprint: ignoredOnly,
        },
      });
      return { actions, engineRecords, applied: true };
    }
    const detail = `engine cleanup --apply left ${track.worktree_path} in place (exit ${applied.exit_code}) without the measured submodule refusal or an enumerated ignored-only footprint; no fallback is taken`;
    addAction(actions, 'engine-worktree-removal', track.worktree_path, 'refuse', 'sweeper.refuse.engine-apply', detail);
    return { actions, engineRecords, applied: true };
  }
  addAction(actions, 'engine-worktree-removal', track.worktree_path, 'executed', 'sweeper.executed.engine-remove');
  await pruneScopedWorktree({ name: basename(track.worktree_path), context, actions });
  return { actions, engineRecords, applied: true };
}


function addAction(actions, kind, ref, verdict, reason, detail = null) {
  const action = { order: actions.length + 1, kind, ref, verdict, reason };
  if (detail !== null) action.detail = detail;
  actions.push(action);
  return action;
}

/** G3 own-exit rule: producer stopped, own target/temporaries absent, worktree gone and unlisted, and the declared branch released. */
function exitReasons(track) {
  const reasons = [];
  const unreadable = [
    track.target.unreadable === null ? null : `target ${track.target.path}`,
    ...track.temporary_paths.filter(temporary => temporary.unreadable !== null).map(temporary => `temporary ${temporary.path}`),
    track.worktree.unreadable === null ? null : `worktree ${track.worktree.path}`,
  ].filter(entry => entry !== null);
  if (unreadable.length > 0) {
    reasons.push({ code: 'sweeper.check.path-unreadable', detail: `track ${track.track_id} facts cannot be read: ${unreadable.join(', ')}` });
  }
  if (track.state !== 'completed') reasons.push({ code: 'sweeper.check.track-not-completed', detail: `track ${track.track_id} is ${track.state}` });
  if (track.producer_stopped !== true) reasons.push({ code: 'sweeper.check.producer-running', detail: `track ${track.track_id} holds no producer-stopped receipt` });
  if (track.state === 'completed') {
    if (track.worktree.branch_unreadable !== null) {
      reasons.push({ code: 'sweeper.check.branch-unreadable', detail: `track ${track.track_id} branch ${track.branch} cannot be read (${track.worktree.branch_unreadable})` });
    } else if (track.worktree.branch_present) {
      reasons.push({ code: 'sweeper.check.branch-present', detail: `track ${track.track_id} branch ${track.branch} still exists` });
    }
  }
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

/** The projection `exitReasons` consumes, so `--apply` can re-use it on freshly observed facts. */
function projectTrackFact(fact) {
  return {
    track_id: fact.track.track_id,
    state: fact.track.state,
    producer_stopped: fact.track.producer_stopped,
    branch: fact.track.branch,
    target: fact.target,
    temporary_paths: fact.temporary_paths,
    worktree: fact.worktree,
  };
}

function buildTrackActions(fact, decision) {
  const actions = [];
  const track = fact.track;
  if (track.state !== 'completed') {
    addAction(actions, 'protected', track.worktree_path, 'protected', 'sweeper.protected.track-active');
    return actions;
  }
  const reclaimable = fact.target.exists === true || fact.temporary_paths.some(temporary => temporary.exists === true);
  if (reclaimable && fact.worktree.listed === false && fact.ownership.proven !== true) {
    addAction(
      actions,
      'reclaim-footprint',
      track.worktree_path,
      'refuse',
      'sweeper.refuse.unproven-ownership',
      `track ${track.track_id}: no live Git record proves its worktree and ${fact.ownership.detail}`,
    );
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

/**
 * Path ownership is decided here, on the facts this pass measured: a listed linked worktree of this
 * repository on the claimed branch is owned by that record, and an absent one is owned only when the
 * snapshot row this receipt pairs with retains that exact path.
 */
async function observeTrack(track, { repoRoot, environment, declaration }) {
  const refusals = [];
  const refuse = (code, detail, exitCode = EXIT_FACTS) => refusals.push(refusal(code, detail, exitCode));
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
  const branchRef = await runGit(['rev-parse', '--verify', '--quiet', `refs/heads/${track.branch}`], repoRoot, environment);
  let branchPresent;
  let branchUnreadable = null;
  if (branchRef.exit_code === 0) {
    branchPresent = true;
  } else if (branchRef.exit_code === 1 && branchRef.stdout.trim() === '') {
    branchPresent = false;
  } else {
    branchPresent = true;
    branchUnreadable = branchRef.spawn_error ?? branchRef.stderr.trim();
  }
  if (listed !== null) {
    const status = await runGit(['status', '--porcelain', '--untracked-files=no'], listed.path, environment);
    if (status.exit_code === 0) dirtyTracked = status.stdout.trim() !== '';
    else refuse('sweeper.refuse.git-unreadable', `git status failed in ${listed.path} (${status.spawn_error ?? status.stderr.trim()})`);
    submodules = await submoduleState(listed.path, environment);
    if (submodules.error !== undefined) refuse('sweeper.refuse.git-unreadable', `submodule probe failed in ${listed.path} (${submodules.error})`);
  }
  return {
    fact: {
      track,
      main_root: repoRoot,
      target: { path: track.target_path, expected_path: track.expected_target, exists: target.exists, is_symlink: target.is_symlink, bytes: target.bytes, unreadable: target.unreadable },
      temporary_paths: temporaries,
      ownership: listed !== null
        ? { proven: true, source: 'git-linked-worktree', detail: null }
        : await retainedOwnership(track, declaration),
      worktree: {
        path: track.worktree_path,
        listed: listed !== null,
        exists: worktreeProbe.exists,
        unreadable: worktreeProbe.unreadable,
        checked_out_branch: listed?.branch ?? null,
        head: listed?.head ?? null,
        branch_present: branchPresent,
        branch_unreadable: branchUnreadable,
        locked: listed?.locked ?? false,
        dirty_tracked: dirtyTracked,
        submodule_gitlinks: submodules.gitlinks ?? 0,
        submodule_unresolved_pointers: submodules.unresolved ?? 0,
        removal_blocked_by_submodules: (submodules.gitlinks ?? 0) > 0,
      },
    },
    refusals,
  };
}

async function refreshListedWorktree(track, repoRoot, environment) {
  const listing = await runGit(['worktree', 'list', '--porcelain'], repoRoot, environment);
  if (listing.exit_code !== 0) return track.listed_worktree;
  for (const record of parseWorktreeList(listing.stdout)) {
    if ((await pathKey(record.path)) === track.worktree_key) return record;
  }
  return null;
}

export async function sweepWorktreeInventory(options, environment = process.env) {
  const refusals = [];
  const refuse = (code, detail, exitCode = EXIT_FACTS) => refusals.push(refusal(code, detail, exitCode));

  const mode = options.apply ? 'apply' : options.checkExit !== null ? 'check-exit' : options.checkConvergence ? 'check-convergence' : 'dry-run';
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

  const foreignClaims = await foreignClaimsOf(declarations.siblings ?? [], cacheRoot);

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

  const canonicalTarget = join(cacheRoot, CANONICAL_TARGET_NAME);
  const protectedCheckouts = [];
  if (mainRoot !== null) {
    protectedCheckouts.push({ role: 'main', path: worktrees[0].path, branch: worktrees[0].branch, target: canonicalTarget });
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
        target: expectedTargetFor(cacheRoot, declaration.integration_worktree_path),
      });
    }
  }
  for (const checkout of protectedCheckouts) {
    const measured = await pathBytes(checkout.target);
    if (measured.unreadable !== null) refuse('sweeper.refuse.path-unreadable', `protected target ${checkout.target} of the ${checkout.role} checkout cannot be read (${measured.unreadable})`);
    if (measured.is_symlink) {
      refuse(
        checkout.role === 'main' ? 'sweeper.refuse.canonical-symlink' : 'sweeper.refuse.symlink-path',
        `the protected target ${checkout.target} of the ${checkout.role} checkout is a symlink`,
      );
    }
    checkout.target_present = measured.exists;
    checkout.target_is_symlink = measured.is_symlink;
    checkout.target_bytes = measured.bytes;
  }

  const trackFacts = [];
  for (const track of reconciliation.tracks) {
    const observed = await observeTrack(track, { repoRoot, environment, declaration });
    refusals.push(...observed.refusals);
    trackFacts.push(observed.fact);
  }

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
  const ownedTargetKeys = new Set(await Promise.all([
    ...trackFacts.map(fact => pathKey(fact.target.path)),
    ...protectedCheckouts.map(checkout => pathKey(checkout.target)),
  ]));
  for (const name of cacheEntries.names) {
    if (!name.startsWith(FEATURE_TARGET_PREFIX)) continue;
    const path = join(cacheRoot, name);
    const measured = await pathBytes(path);
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
      leftover: true,
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
      leftover: !integrationNamed,
      reason: integrationNamed ? 'sweeper.unknown.integration-named' : 'sweeper.unknown.unclaimed-worktree',
    });
  }

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

  const commands = [];
  const decisions = new Map();
  const engineRecords = new Map();
  const applyByTrack = new Map();
  let applyIncomplete = false;
  if (refusals.length > 0) {
  } else if (mode === 'apply') {
    const context = {
      options,
      environment,
      repoRoot,
      mainRoot,
      cacheRoot,
      cacheRootKey: await pathKey(cacheRoot),
      tempRootKey: tempRoot,
      declaration,
      protectedWorktreeKeys,
      commands,
    };
    for (const fact of sorted(trackFacts, candidate => candidate.track.track_id)) {
      const outcome = await applyTrack(fact, context);
      outcome.actions = orderActions(outcome.actions);
      applyByTrack.set(fact.track.track_id, outcome);
      if (outcome.engineRecords.length > 0) engineRecords.set(fact.track.track_id, outcome.engineRecords[outcome.engineRecords.length - 1]);
      if (outcome.applied) {
        const relisted = { ...fact.track, listed_worktree: await refreshListedWorktree(fact.track, repoRoot, environment) };
        const observed = await observeTrack(relisted, { repoRoot, environment, declaration });
        fact.target = observed.fact.target;
        fact.temporary_paths = observed.fact.temporary_paths;
        fact.worktree = observed.fact.worktree;
        fact.ownership = observed.fact.ownership;
      }
      const refused = outcome.actions.some(action => action.verdict === 'refuse');
      if (refused || (fact.track.state === 'completed' && exitReasons(projectTrackFact(fact)).length > 0)) applyIncomplete = true;
    }
  } else {
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
    const outcome = applyByTrack.get(track.track_id) ?? null;
    const actions = blocked
      ? [{ order: 1, kind: 'refused', ref: track.worktree_path, verdict: 'refuse', reason: 'sweeper.refuse.inventory-blocked' }]
      : outcome !== null ? outcome.actions : buildTrackActions(fact, decisions.get(track.track_id) ?? null);
    const reasons = exitReasons(projectTrackFact(fact));
    tracks.push({
      track_id: track.track_id,
      plan_id: track.plan_id,
      state: track.state,
      producer_stopped: track.producer_stopped,
      branch: track.branch,
      worktree: fact.worktree,
      target: fact.target,
      temporary_paths: fact.temporary_paths,
      ownership: track.claim === null
        ? null
        : {
          workflow_id: options.workflow,
          plan_id: track.claim.plan_id,
          source: track.claim.source,
          path_proof: fact.ownership.proven === true ? fact.ownership.source : 'unproven',
        },
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
  if ((mode === 'check-exit' || mode === 'check-convergence') && refusals.length === 0) {
    const reasons = mode === 'check-exit'
      ? exitReasons(checkTrack)
      : [
        ...sorted(tracks, track => track.track_id).flatMap(track => exitReasons(track)),
        ...extraWorktrees.map(worktree => ({ code: 'sweeper.check.worktree-extra', detail: `Git still lists non-protected worktree ${worktree.path}` })),
        ...unknownPaths.filter(entry => entry.leftover === true).map(entry => ({ code: 'sweeper.check.unknown-path', detail: `unclaimed footprint ${entry.path} remains` })),
      ];
    checks = { mode, track_id: mode === 'check-exit' ? options.checkExit : null, passed: reasons.length === 0, reasons };
  }

  const exitCode = refusals.length > 0
    ? Math.max(...refusals.map(entry => entry.exit_code))
    : mode === 'apply'
      ? applyIncomplete ? EXIT_FACTS : EXIT_OK
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
      ok: refusals.length === 0 && (mode === 'apply' ? !applyIncomplete : checks === null || checks.passed),
      refusals,
      checks,
    },
    exitCode,
  };
}


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

if (process.argv[1] && import.meta.url === pathToFileURL(await realpath(resolve(process.argv[1]))).href) {
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
