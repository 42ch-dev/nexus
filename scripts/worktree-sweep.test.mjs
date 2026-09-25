#!/usr/bin/env node
/**
 * Contract tests for the read-only half of the G2 sweeper contract (P1-T2).
 *
 * Scope: the sweeper's own observable contracts, exercised through its real CLI against
 * disposable fixtures this file creates and owns. Every fixture is an isolated temporary Git
 * repository with its own linked worktrees, an isolated `HOME`/`XDG_CACHE_HOME`, a fixture
 * snapshot document (never the live workflow snapshot) and a fixture cache root. No product
 * state, no network, no live harness write and no credential is involved.
 *
 *   node --test scripts/worktree-sweep.test.mjs
 *
 * Every case fails on a plausible regression of the contract it names:
 *   * the canonical shared target (and the active peer's footprint) must never be proposed for
 *     reclamation, while a completed track's own target still is;
 *   * the engine's raw dry-run rows decide the proposal: a recorded `remove` row proposes, and a
 *     recorded `refuse` row is reported as a refusal, never as permission;
 *   * an own exit check must pass for a clean completed track while a peer is still active, and
 *     the same run must fail convergence because the global footprint is incomplete;
 *   * the watermark gate must admit zero new tracks whenever reclamation is required;
 *   * an unreadable snapshot/sibling and every stale or unknown ownership shape must refuse with
 *     a non-zero exit and must propose nothing;
 *   * a path or receipt that cannot be read is a fact gap, never proof of absence: an unreadable
 *     worktree, receipt or cache descendant must refuse (and must never let `--check-exit` pass),
 *     while a receipt whose components are real directories inside the temp root stays owned;
 *   * a receipt under a linked parent, a receipt claimed by two tracks, and a sibling plan's
 *     worktree path (or the feature target it implies) are refused before anything is proposed;
 *   * a receipt that names an in-root path through an alias of the temp root stays owned, while a
 *     receipt traversing a link inside that root is still refused — including an alias chain that
 *     leaves the root and enters it again, and a receipt whose `..` would be resolved against such
 *     a link;
 *   * `--apply` reclaims exactly the exact target/temporary footprint and the engine-released
 *     worktree/branch of a completed, merged, released, producer-stopped track, re-observing every
 *     fact afterwards, and leaves an active peer and the shared canonical cache untouched;
 *   * `--apply` refuses — with explicit reasons and zero mutation — a dirty worktree, a live lease,
 *     and an unmerged branch, preserving their bytes;
 *   * an idempotent retry re-reads the partial state (it never replays the old action list) and
 *     finishes only the owned remainder, never touching a peer;
 *   * against the T1 native topology the ACTUAL installed CLI enumerates the freshly initialized
 *     linked worktrees without the historic gitdir traversal fatal, and the measured submodule
 *     removal refusal is reported truthfully and routed around with the exact-path non-force
 *     `rm -rf` + `git worktree prune` route — never `--force`, never `git submodule deinit`;
 *   * a completed track whose declared branch the engine could not release keeps the apply exit at 1
 *     with `exit_clean: false` while still reporting the `retained` row truthfully — the sweeper
 *     never deletes a branch itself, so the residual is reported, not silently certified reclaimed;
 *   * the mandatory re-verification gate is re-run immediately before every mutating step, so a fact
 *     that moves between two mutations aborts the later one — and every one after it — with zero
 *     further mutation, whether the change lands before the engine handover or during `--apply`
 *     before the non-force fallback;
 *   * an engine `cleanup.refuse.dirty-worktree` caused purely by an ignored-only build-output
 *     footprint is reported as `blocked` with each exact ignored path and its measured size and then
 *     reclaimed through the documented exact-path route, while tracked dirt and untracked non-ignored
 *     paths still refuse with zero mutation;
 *   * the canonical `<repo>/.worktrees/<name>` feature-path requirement is part of the contract text
 *     this tool documents, and `--apply` enforces it at action time.
 */
import { strict as assert } from 'node:assert';
import { execFile } from 'node:child_process';
import { createHash } from 'node:crypto';
import { chmodSync, existsSync, mkdirSync, readFileSync, readlinkSync, readdirSync, realpathSync, statSync, symlinkSync, writeFileSync } from 'node:fs';
import { mkdir, mkdtemp, realpath, rm, symlink, writeFile } from 'node:fs/promises';
import { cpus, tmpdir } from 'node:os';
import { basename, join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';

import { computeAvailableK, evaluateWatermarks } from './worktree-sweep.mjs';

const exec = promisify(execFile);
const SCRIPT = fileURLToPath(new URL('./worktree-sweep.mjs', import.meta.url));
const INITIALIZER = fileURLToPath(new URL('./init-worktree-submodules.mjs', import.meta.url));
const WORKFLOW = 'fixture-wf';
const GIB = 1024 ** 3;
const BUDGET = 120 * GIB;
const ESTIMATE = 20 * GIB;
const SIBLING_ID = 'fixture-sibling';

/**
 * Fixture-local ssh transport (T1's convention): the fixture's submodule origins are reached as
 * `ssh://localhost/<path>` and this shim runs the remote git command on this machine, so the
 * fixture needs no protocol override and touches no global Git configuration.
 */
const SSH_SHIM = `#!/bin/sh
while [ $# -gt 0 ]; do
  case "$1" in
    -o|-p|-i|-F|-l|-c|-m|-e|-b|-E|-I|-L|-R|-Q|-S|-W|-w) shift 2 ;;
    -*) shift ;;
    *) shift; break ;;
  esac
done
exec sh -c "$*"
`;

const GIT_ENV = {
  ...process.env,
  GIT_CONFIG_GLOBAL: '/dev/null',
  GIT_CONFIG_SYSTEM: '/dev/null',
  GIT_AUTHOR_DATE: '2026-01-01T00:00:00Z',
  GIT_COMMITTER_DATE: '2026-01-01T00:00:00Z',
};

async function git(args, cwd) {
  const { stdout } = await exec('git', ['-c', 'user.name=fixture', '-c', 'user.email=fixture@example.com', ...args], { cwd, env: GIT_ENV });
  return stdout;
}

async function gitTolerant(args, cwd) {
  try {
    return await git(args, cwd);
  } catch (error) {
    return String(error.stderr ?? error.message ?? '');
  }
}

// --- fixture -----------------------------------------------------------------------------

async function makeFixture(options = {}) {
  const shape = options.shape ?? 'leased';
  const planStatus = options.planStatus ?? (shape === 'released' || shape === 'partial' ? 'Done' : 'InProgress');
  const leased = options.leased ?? shape === 'leased';
  const orphans = options.orphans !== false;
  const root = await mkdtemp(join(tmpdir(), 'v1197-p1-t2-'));
  const main = join(root, 'main');
  const cache = join(root, 'cache');
  const harness = join(root, 'harness');
  const home = join(root, 'home');
  const ownerWorktree = join(root, '.worktrees', 'fixture-owner');
  const peerWorktree = join(root, '.worktrees', 'fixture-peer');
  const integrationWorktree = join(root, '.worktrees', 'iteration-fixture');
  const detachedWorktree = join(root, 'elsewhere', 'fixture-detached');
  const ownerTarget = join(cache, 'nexus-target-fixture-owner');
  const peerTarget = join(cache, 'nexus-target-fixture-peer');
  const canonicalTarget = join(cache, 'nexus-target');
  const detachedTarget = join(cache, 'nexus-target-fixture-detached');
  const paths = { root, main, cache, ownerWorktree, peerWorktree, integrationWorktree, ownerTarget, peerTarget, canonicalTarget };

  await mkdir(main, { recursive: true });
  await mkdir(cache, { recursive: true });
  await mkdir(join(harness, 'workflows', WORKFLOW), { recursive: true });
  await mkdir(home, { recursive: true });
  await git(['init', '--initial-branch=main'], main);
  await writeFile(join(main, 'README.md'), 'fixture\n');
  await git(['add', 'README.md'], main);
  await git(['commit', '-m', 'init'], main);
  if (options.submodule === true) {
    // A real gitlink, committed before any worktree exists, so every fixture worktree index
    // carries the submodule entry that makes Git refuse `git worktree remove`.
    const origin = join(root, 'submodule-origin');
    await mkdir(origin, { recursive: true });
    await git(['init', '--initial-branch=main'], origin);
    await writeFile(join(origin, 'sub.txt'), 'submodule\n');
    await git(['add', 'sub.txt'], origin);
    await git(['commit', '-m', 'sub'], origin);
    await git(['-c', 'protocol.file.allow=always', 'submodule', 'add', origin, 'vendor/fixture-sub'], main);
    await git(['commit', '-m', 'add submodule'], main);
  }
  if (options.ignoredOutputs === true) {
    // Committed before any worktree exists so every linked checkout sees the same ignore rule.
    await writeFile(join(main, '.gitignore'), 'node_modules/\ndist/\n');
    await git(['add', '.gitignore'], main);
    await git(['commit', '-m', 'ignore build outputs'], main);
  }
  await git(['worktree', 'add', '-b', 'iteration/fixture', integrationWorktree], main);
  await git(['worktree', 'add', '-b', 'feat/fixture-owner', ownerWorktree], main);
  await git(['worktree', 'add', '-b', 'feat/fixture-peer', peerWorktree], main);
  if (orphans) await git(['worktree', 'add', '--detach', join(root, '.worktrees', 'fixture-orphan')], main);
  if (options.nonCanonicalTrack === true) {
    // A linked checkout outside the mandated `<dir>/.worktrees/<name>` shape: the dry run may still
    // propose it, but `--apply` must refuse to act on a path it cannot re-prove.
    await mkdir(join(root, 'elsewhere'), { recursive: true });
    await git(['worktree', 'add', '-b', 'feat/fixture-detached', detachedWorktree], main);
  }
  if (options.ignoredOutputs === true) {
    // Ignored build outputs inside the feature worktree: the second measured cleanup obstacle, where
    // the engine reports the worktree dirty although nothing tracked changed. Directory-level sizes
    // are what the fixture can assert against the enumerated footprint.
    await mkdir(join(ownerWorktree, 'node_modules', 'pkg'), { recursive: true });
    await writeFile(join(ownerWorktree, 'node_modules', 'pkg', 'index.js'), 'x'.repeat(4096));
    await mkdir(join(ownerWorktree, 'dist'), { recursive: true });
    await writeFile(join(ownerWorktree, 'dist', 'out.js'), 'y'.repeat(1024));
  }
  if (options.dirty === true) await writeFile(join(ownerWorktree, 'README.md'), 'dirty tracked change\n');
  if (options.unmerged === true) {
    await writeFile(join(ownerWorktree, 'unmerged.txt'), 'unmerged work\n');
    await git(['add', 'unmerged.txt'], ownerWorktree);
    await git(['commit', '-m', 'unmerged work'], ownerWorktree);
  }

  const trackBranches = ['feat/fixture-owner', 'feat/fixture-peer'];
  if (options.nonCanonicalTrack === true) trackBranches.push('feat/fixture-detached');
  const snapshot = {
    schema_version: 1,
    id: options.snapshotId ?? WORKFLOW,
    type: 'iteration',
    status: 'running',
    started_at: '2026-01-01T00:00:00.000Z',
    updated_at: '2026-01-01T00:00:00.000Z',
    branch: { base: 'main', integration: 'iteration/fixture', target: 'main' },
    integration_worktree_path: integrationWorktree,
    plans: [
      {
        id: 'fixture-plan',
        title: 'Fixture plan',
        file: 'plans/fixture.md',
        status: planStatus,
        metadata: { track_branches: trackBranches },
        ...(leased
          ? {
            execution_lease: {
              holder: '00000000-0000-0000-0000-000000000000',
              claimed_at: '2026-01-01T00:00:00.000Z',
              worktree_path: ownerWorktree,
              working_branch: 'feat/fixture-owner',
              session_label: 'fixture coordinator label',
            },
          }
          : {}),
      },
    ],
  };
  const snapshotPath = join(harness, 'workflows', WORKFLOW, 'snapshot.json');
  if (options.snapshotText !== null) {
    await writeFile(snapshotPath, options.snapshotText ?? `${JSON.stringify(snapshot, null, 2)}\n`);
  }
  if (options.siblingText !== undefined) {
    const text = typeof options.siblingText === 'function' ? options.siblingText(paths) : options.siblingText;
    await mkdir(join(harness, 'workflows', SIBLING_ID), { recursive: true });
    await writeFile(join(harness, 'workflows', SIBLING_ID, 'snapshot.json'), text);
  }

  await mkdir(canonicalTarget, { recursive: true });
  await writeFile(join(canonicalTarget, 'canonical.bin'), 'canonical\n');
  await mkdir(ownerTarget, { recursive: true });
  await writeFile(join(ownerTarget, 'target.bin'), 'owner target\n');
  await mkdir(peerTarget, { recursive: true });
  await writeFile(join(peerTarget, 'target.bin'), 'peer target\n');
  if (options.nonCanonicalTrack === true) {
    await mkdir(detachedTarget, { recursive: true });
    await writeFile(join(detachedTarget, 'target.bin'), 'detached target\n');
  }
  if (orphans) {
    await mkdir(join(cache, 'nexus-target-orphan-leftover'), { recursive: true });
    await writeFile(join(cache, 'nexus-target-orphan-leftover', 'junk.bin'), 'orphan\n');
  }

  let inventory = {
    version: 1,
    workflow_id: WORKFLOW,
    active_plan_ids: ['fixture-plan'],
    scheduling: { ready_independent_tasks: 3, disk_budget_bytes: BUDGET, per_track_target_estimate_bytes: ESTIMATE },
    tracks: [
      {
        track_id: 'fixture-owner',
        plan_id: 'fixture-plan',
        worktree: ownerWorktree,
        branch: 'feat/fixture-owner',
        target: ownerTarget,
        temporary_paths: [],
        producer_stopped: true,
        state: 'completed',
      },
      {
        track_id: 'fixture-peer',
        plan_id: 'fixture-plan',
        worktree: peerWorktree,
        branch: 'feat/fixture-peer',
        target: peerTarget,
        temporary_paths: [],
        producer_stopped: false,
        state: 'active',
      },
    ],
  };
  if (options.nonCanonicalTrack === true) {
    inventory.tracks.push({
      track_id: 'fixture-detached',
      plan_id: 'fixture-plan',
      worktree: detachedWorktree,
      branch: 'feat/fixture-detached',
      target: detachedTarget,
      temporary_paths: [],
      producer_stopped: true,
      state: 'completed',
    });
  }
  if (options.mutateInventory !== undefined) inventory = options.mutateInventory(inventory, paths);
  const inventoryPath = join(root, 'inventory.json');
  await writeFile(inventoryPath, `${JSON.stringify(inventory, null, 2)}\n`);

  if (shape === 'reclaimed' || shape === 'partial') {
    // A merged slice the engine already released, with no worktree left. `reclaimed` also drops the
    // owned target (the fully clean shape); `partial` keeps it, plus whatever temporary receipts the
    // caller lists, so an idempotent retry has real leftover to reclaim. `keepBranch` models the
    // measured case where the engine released the worktree but not the track's branch.
    await git(['worktree', 'remove', ownerWorktree], main);
    if (options.keepBranch !== true) await git(['branch', '-d', 'feat/fixture-owner'], main);
  }
  if (shape === 'reclaimed') await rm(ownerTarget, { recursive: true, force: true });

  const fixture = {
    root,
    main,
    cache,
    harness,
    home,
    inventory: inventoryPath,
    ownerWorktree,
    peerWorktree,
    integrationWorktree,
    ownerTarget,
    peerTarget,
    canonicalTarget,
    detachedWorktree,
    detachedTarget,
    env: { ...process.env, HOME: home, XDG_CACHE_HOME: cache },
    git(args, cwd = main) {
      return git(args, cwd);
    },
    async run(args = []) {
      const argv = ['--repo', main, '--harness', harness, '--workflow', WORKFLOW, '--inventory', inventoryPath, ...args];
      try {
        const { stdout, stderr } = await exec(process.execPath, [SCRIPT, ...argv], { cwd: root, env: fixture.env, maxBuffer: 8 * 1024 * 1024 });
        return { code: 0, stdout, stderr, document: JSON.parse(stdout) };
      } catch (error) {
        const stdout = String(error.stdout ?? '');
        let document = null;
        try {
          document = JSON.parse(stdout);
        } catch {
          document = null;
        }
        return { code: typeof error.code === 'number' ? error.code : null, stdout, stderr: String(error.stderr ?? ''), document };
      }
    },
    async fingerprint() {
      return fingerprint(fixture);
    },
    async teardown() {
      // T1's merge-first fixture policy: the fixture's own branches are merged into its integration
      // checkout before anything is deleted, so `git branch -d` is the lawful, non-force teardown.
      for (const branch of ['feat/fixture-owner', 'feat/fixture-peer', 'feat/fixture-detached']) {
        await gitTolerant(['merge', '--no-edit', '--ff-only', branch], integrationWorktree);
      }
      for (const path of [ownerWorktree, peerWorktree, detachedWorktree, integrationWorktree, join(root, '.worktrees', 'fixture-orphan')]) {
        if (!existsSync(path)) continue;
        await gitTolerant(['worktree', 'remove', path], main);
        // Test-owned disposable trees only: Git refuses to remove a worktree holding submodule
        // gitlinks, so drop the fixture's own bytes and let the prune below forget the entry.
        if (existsSync(path)) await rm(path, { recursive: true, force: true });
      }
      await gitTolerant(['worktree', 'prune'], main);
      for (const branch of ['feat/fixture-owner', 'feat/fixture-peer', 'feat/fixture-detached', 'iteration/fixture']) {
        await gitTolerant(['branch', '-d', branch], main);
      }
      await rm(root, { recursive: true, force: true });
      assert.equal(existsSync(root), false, `fixture root ${root} was not removed`);
    },
  };
  return fixture;
}

/** Working-tree bytes plus refs and the worktree list; `.git` internals stay out of scope. */
function listFiles(root, base = '', out = []) {
  for (const entry of readdirSync(root, { withFileTypes: true })) {
    if (entry.name === '.git') continue;
    const path = join(root, entry.name);
    const relative = base === '' ? entry.name : `${base}/${entry.name}`;
    if (entry.isSymbolicLink()) out.push([relative, 'symlink', readlinkSync(path)]);
    else if (entry.isDirectory()) listFiles(path, relative, out);
    else out.push([relative, statSync(path).size, createHash('sha256').update(readFileSync(path)).digest('hex')]);
  }
  return out;
}

function directoryBytes(root) {
  let total = 0;
  for (const entry of readdirSync(root, { withFileTypes: true })) {
    const path = join(root, entry.name);
    if (entry.isSymbolicLink()) continue;
    if (entry.isDirectory()) total += directoryBytes(path);
    else total += statSync(path).size;
  }
  return total;
}

async function fingerprint(fixture) {
  return JSON.stringify({
    files: listFiles(fixture.root).sort(),
    refs: await gitTolerant(['for-each-ref'], fixture.main),
    worktrees: await gitTolerant(['worktree', 'list', '--porcelain'], fixture.main),
    // Shared config and submodule registration are covered so a `git submodule deinit`
    // (which rewrites them) would be caught rather than tolerated.
    config: await gitTolerant(['config', '--list', '--local'], fixture.main),
    submodules: await gitTolerant(['submodule', 'status'], fixture.main),
  });
}

function refusalCodes(run) {
  return (run.document?.refusals ?? []).map(entry => entry.code);
}

function proposedRefs(document) {
  return document.tracks.flatMap(track => track.actions.filter(action => action.verdict === 'propose').map(action => action.ref));
}

function trackOf(document, trackId) {
  const track = document.tracks.find(candidate => candidate.track_id === trackId);
  assert.ok(track, `track ${trackId} must be reported`);
  return track;
}

const ENGINE_FLAGS = ['--apply', '--all-workflows', '--remote', '--ignore-unreadable-snapshots', '--force', '-f'];

/**
 * Ownership receipts live under the canonical temp root, and `tmpdir()` is not canonical on every
 * platform: fixtures name them through the fixture root's real path, so a receipt is a plain
 * descendant of the canonical root instead of an accident of `/var` versus `/private/var`.
 */
function receiptBase(paths) {
  return join(realpathSync(paths.root), 'receipts');
}

/**
 * An alias of the fixture's own temp root: `<root>/temp-alias -> <root>/temp-real`. Setting
 * `TMPDIR` to the alias makes the child sweep a canonical root reached only through a link, which
 * is the shape `/var` versus `/private/var` takes on Darwin — constructed here so the same shape
 * is exercised on a host whose `tmpdir()` is already canonical.
 */
function aliasTempRoot(paths) {
  const root = realpathSync(paths.root);
  const realTemp = join(root, 'temp-real');
  const aliasTemp = join(root, 'temp-alias');
  mkdirSync(realTemp, { recursive: true });
  symlinkSync(realTemp, aliasTemp, 'dir');
  return { realTemp, aliasTemp };
}

/**
 * A fixture-local `rm` shim: it runs the real `/bin/rm` and then, once, when it removed the given
 * path, injects one fact change. The change therefore lands strictly BETWEEN two of the sweep's
 * mutating steps, which is exactly what the mandatory re-verification gate has to notice.
 */
async function rmShim(root, { after, mutate }) {
  const directory = join(root, 'rm-shim');
  const marker = join(root, 'rm-shim-fired');
  await mkdir(directory, { recursive: true });
  await writeFile(join(directory, 'rm'), `#!/bin/sh
/bin/rm "$@"
status=$?
if [ $status -eq 0 ] && [ ! -f ${JSON.stringify(marker)} ]; then
  for arg in "$@"; do
    if [ "$arg" = ${JSON.stringify(after)} ]; then
      touch ${JSON.stringify(marker)}
      ${mutate} >/dev/null 2>&1
    fi
  done
fi
exit $status
`, { mode: 0o755 });
  return { directory, marker };
}

/**
 * A PATH wrapper for the ACTUAL installed CLI: every invocation is delegated to the real binary
 * verbatim, and one fact change is injected immediately before the first `--apply` handover — that
 * is, while the engine is running: after the gate that allowed the handover and before the non-force
 * fallback that follows it. The engine's own output is never rewritten or faked.
 */
async function engineApplyWrapper(root, { mutate }) {
  const directory = join(root, 'engine-wrapper');
  const marker = join(root, 'engine-wrapper-fired');
  const engine = (await exec('sh', ['-c', 'command -v mstar-harness'], { env: process.env })).stdout.trim();
  await mkdir(directory, { recursive: true });
  await writeFile(join(directory, 'mstar-harness'), `#!/bin/sh
for arg in "$@"; do
  if [ "$arg" = "--apply" ] && [ ! -f ${JSON.stringify(marker)} ]; then
    touch ${JSON.stringify(marker)}
    ${mutate} >/dev/null 2>&1
  fi
done
exec ${JSON.stringify(engine)} "$@"
`, { mode: 0o755 });
  return { directory, marker };
}

/** Run the sweeper itself, outside any fixture, so a usage/contract probe keeps its exit code. */
async function runScript(args) {
  try {
    const { stdout, stderr } = await exec(process.execPath, [SCRIPT, ...args], { cwd: process.cwd(), env: process.env, maxBuffer: 8 * 1024 * 1024 });
    return { code: 0, stdout, stderr };
  } catch (error) {
    return { code: error.code, stdout: String(error.stdout ?? ''), stderr: String(error.stderr ?? '') };
  }
}

/** A sibling plan declaration claiming one worktree path — and therefore one feature target. */
function siblingSnapshot({ worktreePath, branches = [] }) {
  return `${JSON.stringify({
    schema_version: 1,
    id: SIBLING_ID,
    type: 'plan',
    status: 'running',
    branch: { base: 'main', target: 'main' },
    plans: [{
      id: 'sibling-plan',
      title: 'Sibling plan',
      file: 'plans/sibling.md',
      status: 'InProgress',
      metadata: branches.length > 0 ? { worktree_path: worktreePath, track_branches: branches } : { worktree_path: worktreePath },
    }],
  }, null, 2)}\n`;
}

// --- tests -------------------------------------------------------------------------------

test('inventory protects active and shared paths', async t => {
  const leased = await makeFixture({ shape: 'leased' });
  t.after(() => leased.teardown());

  const before = await leased.fingerprint();
  const run = await leased.run();
  assert.equal(run.code, 0);
  assert.equal(run.document.ok, true);
  assert.deepEqual(run.document.refusals, []);

  // The main checkout and the snapshot-recorded integration checkout stay protected, and the
  // shared canonical target occupancy is reported once.
  assert.deepEqual(run.document.protected_checkouts.map(checkout => checkout.role), ['main', 'integration']);
  for (const checkout of run.document.protected_checkouts) {
    assert.equal(checkout.target, leased.canonicalTarget);
    assert.equal(checkout.target_present, true);
    assert.equal(checkout.target_is_symlink, false);
  }
  assert.ok(run.document.protected_checkouts[0].target_bytes > 0);
  assert.equal(run.document.protected_checkouts[1].target_bytes, null);
  assert.equal(run.document.protected_checkouts[1].target_bytes_counted_by, 'main');

  // Nothing protected is ever proposed: not the canonical cache, not the active peer, not main.
  const proposed = proposedRefs(run.document);
  assert.equal(proposed.includes(leased.canonicalTarget), false);
  assert.equal(proposed.includes(leased.peerWorktree), false);
  assert.equal(proposed.includes(leased.peerTarget), false);

  const peer = trackOf(run.document, 'fixture-peer');
  assert.deepEqual(peer.actions.map(action => action.verdict), ['protected']);
  assert.equal(peer.actions[0].reason, 'sweeper.protected.track-active');
  assert.equal(peer.ownership.source, 'plan-branch-claim');

  // A completed track still proposes exactly its own target, and the engine's recorded refusal
  // is reported as a refusal rather than as permission.
  const owner = trackOf(run.document, 'fixture-owner');
  assert.deepEqual(owner.actions.filter(action => action.verdict === 'propose').map(action => action.kind), ['reclaim-target']);
  assert.equal(proposed.includes(leased.ownerTarget), true);
  const removal = owner.actions.find(action => action.kind === 'engine-worktree-removal');
  assert.equal(removal.verdict, 'refuse');
  assert.equal(removal.reason, 'cleanup.refuse.active-lease');
  assert.equal(owner.exit_clean, false);

  // The dry run records the real installed engine command, verbatim and without widening flags.
  assert.equal(run.document.commands.length, 1);
  assert.deepEqual(run.document.commands[0].argv, [
    'mstar-harness', 'worktree', 'cleanup',
    '--workflow', WORKFLOW,
    '--harness', leased.harness,
    '--worktree', owner.worktree.path,
  ]);
  for (const flag of ENGINE_FLAGS) assert.equal(run.document.commands[0].argv.includes(flag), false);
  assert.equal(run.document.commands[0].exit_code, 0);
  assert.equal(run.document.commands[0].spawn_error, null);
  assert.match(run.document.commands[0].stdout, /\nrefuse \| worktree \| /);

  // Unclaimed footprints are reported, owned ones are not, and the canonical cache is never one.
  const unknown = run.document.unknown_paths.map(entry => entry.path);
  assert.equal(unknown.includes(leased.canonicalTarget), false);
  assert.equal(unknown.includes(leased.integrationWorktree), false);
  assert.equal(unknown.includes(leased.ownerWorktree), false);
  assert.equal(unknown.includes(leased.peerWorktree), false);
  assert.equal(unknown.some(path => path.endsWith('nexus-target-orphan-leftover')), true);
  assert.equal(unknown.some(path => path.endsWith('fixture-orphan')), true);
  assert.equal(run.document.unknown_paths.find(entry => entry.kind === 'feature-target').verdict, 'unknown');

  // Opaque coordination data never reaches stdout.
  assert.equal(run.stdout.includes('00000000-0000-0000-0000-000000000000'), false);
  assert.equal(run.stdout.includes('fixture coordinator label'), false);
  assert.equal(run.stdout.includes('session_file'), false);

  assert.equal(await leased.fingerprint(), before);

  // A released, merged slice is the one shape the engine itself permits: the recorded `remove`
  // row becomes an ordered proposal, followed by the scoped prune step.
  const released = await makeFixture({ shape: 'released' });
  t.after(() => released.teardown());
  const releasedBefore = await released.fingerprint();
  const releasedRun = await released.run();
  assert.equal(releasedRun.code, 0);
  const releasedOwner = trackOf(releasedRun.document, 'fixture-owner');
  assert.match(releasedRun.document.commands[0].stdout, /\nremove \| worktree \| /);
  assert.deepEqual(releasedOwner.actions.map(action => action.kind), ['reclaim-target', 'engine-worktree-removal', 'prune-dry-run']);
  assert.deepEqual(releasedOwner.actions.map(action => action.verdict), ['propose', 'propose', 'propose']);
  assert.equal(releasedOwner.actions[1].reason, 'sweeper.propose.engine-remove');
  assert.equal(await released.fingerprint(), releasedBefore);

  // A worktree holding submodule gitlinks is reported as blocked, never as permission: measured
  // on this repository's Git, `git worktree remove` refuses such a worktree, so nothing is
  // proposed and neither a forced removal nor `git submodule deinit` is ever used.
  const blocked = await makeFixture({ shape: 'released', submodule: true });
  t.after(() => blocked.teardown());
  const blockedBefore = await blocked.fingerprint();
  const blockedRun = await blocked.run();
  assert.equal(blockedRun.code, 0);
  const blockedOwner = trackOf(blockedRun.document, 'fixture-owner');
  assert.equal(blockedOwner.worktree.removal_blocked_by_submodules, true);
  const blockedRemoval = blockedOwner.actions.find(action => action.kind === 'engine-worktree-removal');
  assert.equal(blockedRemoval.verdict, 'blocked');
  assert.equal(blockedRemoval.reason, 'sweeper.blocked.submodule-gitlinks');
  assert.equal(blockedOwner.actions.some(action => action.kind === 'prune-dry-run'), false);
  assert.equal(proposedRefs(blockedRun.document).includes(blockedOwner.worktree.path), false);
  assert.equal(blockedRun.document.commands.length, 1);
  for (const command of blockedRun.document.commands) {
    assert.equal(command.argv.includes('deinit'), false);
    assert.equal(command.argv.includes('--force'), false);
  }
  // The blocked verdict rests on measured facts, and reports the diagnosis with them: this
  // fixture's submodule pointer resolves, while this repository's linked checkouts report the
  // unresolvable gitdir that makes the non-forced removal inadmissible.
  assert.ok(blockedOwner.worktree.submodule_gitlinks >= 1);
  assert.equal(blockedOwner.worktree.submodule_unresolved_pointers, 0);

  // The own exit gate fails and explains why, instead of pretending the footprint is gone.
  const blockedCheck = await blocked.run(['--check-exit', 'fixture-owner']);
  assert.equal(blockedCheck.code, 1);
  assert.equal(blockedCheck.document.checks.reasons.map(reason => reason.code).includes('sweeper.check.worktree-blocked-submodules'), true);
  assert.equal(await blocked.fingerprint(), blockedBefore);
});

test('check-exit distinguishes own completion from peer activity', async t => {
  const reclaimed = await makeFixture({ shape: 'reclaimed' });
  t.after(() => reclaimed.teardown());

  const before = await reclaimed.fingerprint();
  const checkExit = await reclaimed.run(['--check-exit', 'fixture-owner']);
  assert.equal(checkExit.code, 0);
  assert.equal(checkExit.document.mode, 'check-exit');
  assert.equal(checkExit.document.ok, true);
  assert.equal(checkExit.document.checks.track_id, 'fixture-owner');
  assert.equal(checkExit.document.checks.passed, true);
  assert.deepEqual(checkExit.document.checks.reasons, []);
  assert.deepEqual(checkExit.document.commands, []);

  // The global footprint is still incomplete (the peer is live), so convergence must fail even
  // though the own exit passed.
  const convergence = await reclaimed.run(['--check-convergence']);
  assert.equal(convergence.code, 1);
  assert.equal(convergence.document.ok, false);
  assert.equal(convergence.document.checks.passed, false);
  const convergenceCodes = convergence.document.checks.reasons.map(reason => reason.code);
  assert.equal(convergenceCodes.includes('sweeper.check.track-not-completed'), true);
  assert.equal(convergence.document.checks.reasons.every(reason => !reason.detail.includes('track fixture-owner ')), true);
  assert.deepEqual(convergence.document.commands, []);
  assert.equal(await reclaimed.fingerprint(), before);

  const residual = await makeFixture({ shape: 'leased' });
  t.after(() => residual.teardown());
  const residualBefore = await residual.fingerprint();
  const residualRun = await residual.run(['--check-exit', 'fixture-owner']);
  assert.equal(residualRun.code, 1);
  assert.equal(residualRun.document.checks.passed, false);
  const residualCodes = residualRun.document.checks.reasons.map(reason => reason.code);
  assert.equal(residualCodes.includes('sweeper.check.target-present'), true);
  assert.equal(residualCodes.includes('sweeper.check.worktree-listed'), true);
  assert.equal(await residual.fingerprint(), residualBefore);

  // Mode and check contracts.
  const unknownTrack = await reclaimed.run(['--check-exit', 'nosuch-track']);
  assert.equal(unknownTrack.code, 2);
  assert.equal(refusalCodes(unknownTrack).includes('sweeper.refuse.unknown-track'), true);

  const conflicting = await reclaimed.run(['--check-exit', 'fixture-owner', '--check-convergence']);
  assert.equal(conflicting.code, 2);
  assert.equal(conflicting.document, null);
  assert.match(conflicting.stderr, /mutually exclusive/);

  // `--apply` is a real mode now (covered by the apply tests); the argument-level refusal that
  // survives is its mutual exclusion with the read-only check modes.
  const applyWithCheck = await reclaimed.run(['--apply', '--check-convergence']);
  assert.equal(applyWithCheck.code, 2);
  assert.equal(applyWithCheck.document, null);
  assert.match(applyWithCheck.stderr, /mutually exclusive/);
});

test('capacity watermarks require reclamation before new tracks', async t => {
  const fixture = await makeFixture({ shape: 'leased' });
  t.after(() => fixture.teardown());

  const run = await fixture.run();
  const capacity = run.document.capacity;
  assert.equal(capacity.units, 'bytes');
  assert.ok(capacity.root_free_bytes > 0);
  assert.equal(capacity.cores, cpus().length);
  // Measured aggregate covers every feature target and excludes the protected canonical cache.
  const expected = ['nexus-target-fixture-owner', 'nexus-target-fixture-peer', 'nexus-target-orphan-leftover']
    .reduce((total, name) => total + directoryBytes(join(fixture.cache, name)), 0);
  assert.equal(capacity.aggregate_feature_target_bytes, expected);
  assert.ok(capacity.aggregate_feature_target_bytes < directoryBytes(fixture.cache));
  assert.equal(capacity.feature_target_count, 3);
  assert.equal(capacity.configured_disk_budget_bytes, BUDGET);
  assert.equal(capacity.per_track_target_estimate_bytes, ESTIMATE);
  assert.equal(capacity.watermarks.root_free_min_bytes, 90 * GIB);
  assert.equal(capacity.watermarks.feature_targets_max_bytes, 120 * GIB);

  // K follows G3 exactly, and a required reclamation admits zero new tracks.
  assert.equal(capacity.computed_k, Math.min(3, Math.floor(BUDGET / ESTIMATE), Math.max(1, Math.floor(cpus().length / 2))));
  assert.equal(capacity.admissible_new_tracks, capacity.watermarks.reclamation_required ? 0 : capacity.computed_k);
  assert.equal(capacity.watermarks.reclamation_required, !(capacity.watermarks.root_free_ok && capacity.watermarks.feature_targets_ok));

  // Boundary behaviour, not rows of equivalents: zero ready work, a disk-bound budget, a
  // CPU-bound host, and a failing watermark admitting nothing.
  const idle = await makeFixture({ shape: 'leased', mutateInventory: document => ({ ...document, scheduling: { ...document.scheduling, ready_independent_tasks: 0 } }) });
  t.after(() => idle.teardown());
  const idleRun = await idle.run();
  assert.equal(idleRun.document.capacity.computed_k, 0);
  assert.equal(idleRun.document.capacity.admissible_new_tracks, 0);

  const diskBound = await makeFixture({ shape: 'leased', mutateInventory: document => ({ ...document, scheduling: { ...document.scheduling, disk_budget_bytes: GIB } }) });
  t.after(() => diskBound.teardown());
  const diskRun = await diskBound.run();
  assert.equal(diskRun.document.capacity.computed_k, 0);
  assert.equal(diskRun.document.capacity.admissible_new_tracks, 0);

  assert.equal(computeAvailableK({ readyIndependentTasks: 3, diskBudgetBytes: BUDGET, perTrackTargetEstimateBytes: ESTIMATE, cores: 10 }), 3);
  assert.equal(computeAvailableK({ readyIndependentTasks: 3, diskBudgetBytes: BUDGET, perTrackTargetEstimateBytes: ESTIMATE, cores: 2 }), 1);
  assert.equal(computeAvailableK({ readyIndependentTasks: 3, diskBudgetBytes: BUDGET, perTrackTargetEstimateBytes: ESTIMATE, cores: 1 }), 1);
  assert.equal(computeAvailableK({ readyIndependentTasks: 0, diskBudgetBytes: BUDGET, perTrackTargetEstimateBytes: ESTIMATE, cores: 10 }), 0);
  assert.equal(computeAvailableK({ readyIndependentTasks: 3, diskBudgetBytes: 4 * GIB, perTrackTargetEstimateBytes: ESTIMATE, cores: 10 }), 0);

  assert.equal(evaluateWatermarks({ rootFreeBytes: 90 * GIB, featureTargetBytes: 120 * GIB }).reclamation_required, false);
  assert.equal(evaluateWatermarks({ rootFreeBytes: 90 * GIB - 1, featureTargetBytes: 0 }).reclamation_required, true);
  assert.equal(evaluateWatermarks({ rootFreeBytes: 90 * GIB, featureTargetBytes: 120 * GIB + 1 }).reclamation_required, true);
});

test('unreadable or stale ownership fails closed', async t => {
  const scenarios = [
    ['corrupt snapshot', { snapshotText: '{ not json' }, 1, 'sweeper.refuse.snapshot-unreadable'],
    ['missing snapshot', { snapshotText: null }, 1, 'sweeper.refuse.snapshot-unreadable'],
    ['unreadable sibling declaration', { siblingText: '{ not json' }, 1, 'sweeper.refuse.sibling-snapshot-unreadable'],
    ['foreign claim', {
      siblingText: `${JSON.stringify({
        schema_version: 1,
        id: 'fixture-sibling',
        type: 'plan',
        status: 'running',
        branch: { base: 'main', target: 'main' },
        plans: [{ id: 'sibling-plan', title: 'Sibling', file: 'plans/sibling.md', status: 'InProgress', metadata: { track_branches: ['feat/fixture-owner'] } }],
      })}\n`,
    }, 2, 'sweeper.refuse.foreign-claim'],
    ['snapshot identity mismatch', { snapshotId: 'other-workflow' }, 2, 'sweeper.refuse.snapshot-identity'],
    ['stale branch claim', { mutateInventory: document => { document.tracks[0].branch = 'feat/fixture-unclaimed'; return document; } }, 2, 'sweeper.refuse.stale-branch-claim'],
    ['stale target mapping', { mutateInventory: (document, paths) => { document.tracks[0].target = join(paths.cache, 'nexus-target-wrong-name'); return document; } }, 2, 'sweeper.refuse.stale-target'],
    ['unknown track state', { mutateInventory: document => { document.tracks[0].state = 'paused'; return document; } }, 2, 'sweeper.refuse.unknown-state'],
    ['duplicate track ownership', { mutateInventory: document => { document.tracks[1] = { ...document.tracks[0], track_id: 'fixture-twin' }; return document; } }, 2, 'sweeper.refuse.duplicate-track'],
    ['omitted claimed plan', { mutateInventory: document => { document.active_plan_ids = []; return document; } }, 2, 'sweeper.refuse.inventory-omission'],
    ['unknown inventory version', { mutateInventory: document => ({ ...document, version: 2 }) }, 2, 'sweeper.refuse.inventory-version'],
    ['stale worktree pair', { mutateInventory: (document, paths) => { document.tracks[0].worktree = paths.peerWorktree; document.tracks[0].target = join(paths.cache, 'nexus-target-fixture-peer'); document.tracks[0].branch = 'feat/fixture-unclaimed'; return document; } }, 2, 'sweeper.refuse.stale-worktree-pair'],
    // A receipt whose parent is a regular file cannot be read at all: that failure is a fact gap
    // (exit 1), never proof that the receipt is gone.
    ['temporary receipt under a non-directory parent', {
      mutateInventory: (document, paths) => {
        const notADirectory = join(realpathSync(paths.root), 'not-a-directory');
        writeFileSync(notADirectory, 'not a directory\n');
        document.tracks[0].temporary_paths = [join(notADirectory, 'receipt')];
        return document;
      },
    }, 1, 'sweeper.refuse.path-unreadable'],
    // The declared parent is a link: following it would describe and propose a location the
    // receipt never named, so the containment check walks the canonical components instead.
    ['temporary receipt through a symlinked parent', {
      mutateInventory: (document, paths) => {
        const receipts = receiptBase(paths);
        mkdirSync(receipts, { recursive: true });
        mkdirSync(join(paths.cache, 'detached-receipt'), { recursive: true });
        writeFileSync(join(paths.cache, 'detached-receipt', 'payload.bin'), 'detached\n');
        symlinkSync(paths.cache, join(receipts, 'link'), 'dir');
        document.tracks[0].temporary_paths = [join(receipts, 'link', 'detached-receipt')];
        return document;
      },
    }, 2, 'sweeper.refuse.stale-temporary'],
    // One receipt cannot be owned twice: the completed track's proposal would otherwise reclaim a
    // path the still-active peer track has also listed.
    ['overlapping temporary receipts across tracks', {
      mutateInventory: (document, paths) => {
        const shared = join(receiptBase(paths), 'shared');
        mkdirSync(shared, { recursive: true });
        writeFileSync(join(shared, 'payload.bin'), 'shared\n');
        document.tracks[0].temporary_paths = [shared];
        document.tracks[1].temporary_paths = [shared];
        return document;
      },
    }, 2, 'sweeper.refuse.duplicate-temporary'],
    // A sibling plan's worktree path is a foreign claim even while that worktree is absent and the
    // branch is separately claimed locally.
    ['foreign sibling worktree path claim', {
      siblingText: paths => siblingSnapshot({ worktreePath: join(paths.root, '.worktrees', 'fixture-shared') }),
      mutateInventory: (document, paths) => {
        document.tracks[0].worktree = join(paths.root, '.worktrees', 'fixture-shared');
        document.tracks[0].target = join(paths.cache, 'nexus-target-fixture-shared');
        return document;
      },
    }, 2, 'sweeper.refuse.foreign-claim'],
    // A different sibling worktree path can still imply the same `.envrc` feature target: the
    // derived target is claimed too, so the collision cannot slip through the path map.
    ['foreign sibling target collision', {
      siblingText: paths => siblingSnapshot({ worktreePath: join(paths.root, 'elsewhere', 'fixture-shared') }),
      mutateInventory: (document, paths) => {
        document.tracks[0].worktree = join(paths.root, '.worktrees', 'fixture-shared');
        document.tracks[0].target = join(paths.cache, 'nexus-target-fixture-shared');
        return document;
      },
    }, 2, 'sweeper.refuse.foreign-claim'],
  ];

  for (const [name, options, expectedCode, expectedRefusal] of scenarios) {
    const fixture = await makeFixture({ shape: 'leased', ...options });
    t.after(() => fixture.teardown());
    const before = await fixture.fingerprint();
    const run = await fixture.run();
    assert.equal(run.code, expectedCode, `${name}: exit code`);
    assert.equal(run.document.ok, false, `${name}: ok`);
    assert.equal(refusalCodes(run).includes(expectedRefusal), true, `${name}: refusal ${expectedRefusal} in ${JSON.stringify(refusalCodes(run))}`);
    assert.deepEqual(proposedRefs(run.document), [], `${name}: nothing may be proposed`);
    assert.equal(run.document.tracks.every(track => track.actions.every(action => action.verdict !== 'propose')), true, `${name}: no proposing action`);
    assert.deepEqual(run.document.commands, [], `${name}: no engine invocation on refused input`);
    assert.equal(await fixture.fingerprint(), before, `${name}: read-only`);
  }

  // Facts that cannot be read must never be certified as an absent footprint: pre-fix both
  // finished-track shapes below reported the footprint gone (target absent, worktree unlisted and
  // "not present") and let `--check-exit` exit 0 without having read anything.
  const unreadableWorktree = await makeFixture({
    shape: 'reclaimed',
    mutateInventory: (document, paths) => {
      const notADirectory = join(realpathSync(paths.root), 'not-a-directory');
      writeFileSync(notADirectory, 'not a directory\n');
      document.tracks[0].worktree = join(notADirectory, 'fixture-shared');
      document.tracks[0].target = join(paths.cache, 'nexus-target-fixture-shared');
      return document;
    },
  });
  t.after(() => unreadableWorktree.teardown());
  const unreadableWorktreeBefore = await unreadableWorktree.fingerprint();
  const unreadableWorktreeRun = await unreadableWorktree.run();
  assert.equal(unreadableWorktreeRun.code, 1);
  assert.equal(refusalCodes(unreadableWorktreeRun).includes('sweeper.refuse.path-unreadable'), true);
  const unreadableWorktreeTrack = trackOf(unreadableWorktreeRun.document, 'fixture-owner');
  assert.equal(unreadableWorktreeTrack.exit_clean, false);
  const unreadableWorktreeCheck = await unreadableWorktree.run(['--check-exit', 'fixture-owner']);
  assert.equal(unreadableWorktreeCheck.code, 1);
  assert.equal(unreadableWorktreeCheck.document.ok, false);
  assert.equal(unreadableWorktreeCheck.document.checks, null);
  assert.equal(await unreadableWorktree.fingerprint(), unreadableWorktreeBefore);

  const unreadableReceipt = await makeFixture({
    shape: 'reclaimed',
    mutateInventory: (document, paths) => {
      const notADirectory = join(realpathSync(paths.root), 'not-a-directory');
      writeFileSync(notADirectory, 'not a directory\n');
      document.tracks[0].temporary_paths = [join(notADirectory, 'receipt')];
      return document;
    },
  });
  t.after(() => unreadableReceipt.teardown());
  const unreadableReceiptBefore = await unreadableReceipt.fingerprint();
  const unreadableReceiptRun = await unreadableReceipt.run();
  assert.equal(unreadableReceiptRun.code, 1);
  assert.equal(refusalCodes(unreadableReceiptRun).includes('sweeper.refuse.path-unreadable'), true);
  const unreadableReceiptTrack = trackOf(unreadableReceiptRun.document, 'fixture-owner');
  assert.equal(unreadableReceiptTrack.exit_clean, false);
  assert.deepEqual(unreadableReceiptTrack.temporary_paths.map(entry => entry.unreadable), ['ENOTDIR']);
  const unreadableReceiptCheck = await unreadableReceipt.run(['--check-exit', 'fixture-owner']);
  assert.equal(unreadableReceiptCheck.code, 1);
  assert.equal(unreadableReceiptCheck.document.checks, null);
  assert.equal(await unreadableReceipt.fingerprint(), unreadableReceiptBefore);

  // Control: canonical containment refuses linked parents, not receipts. A receipt whose
  // components are all real directories inside the canonical temp root stays owned and proposable.
  const ownedReceipt = await makeFixture({
    shape: 'leased',
    mutateInventory: (document, paths) => {
      const receipt = join(receiptBase(paths), 'owned');
      mkdirSync(receipt, { recursive: true });
      writeFileSync(join(receipt, 'payload.bin'), 'owned receipt\n');
      document.tracks[0].temporary_paths = [receipt];
      return document;
    },
  });
  t.after(() => ownedReceipt.teardown());
  const ownedReceiptRun = await ownedReceipt.run();
  assert.equal(ownedReceiptRun.code, 0);
  assert.deepEqual(ownedReceiptRun.document.refusals, []);
  const ownedReceiptTrack = trackOf(ownedReceiptRun.document, 'fixture-owner');
  assert.equal(ownedReceiptTrack.temporary_paths[0].exists, true);
  assert.ok(ownedReceiptTrack.temporary_paths[0].bytes > 0);
  const ownedTemporaryAction = ownedReceiptTrack.actions.find(action => action.kind === 'reclaim-temporary');
  assert.equal(ownedTemporaryAction.verdict, 'propose');
  assert.equal(ownedTemporaryAction.reason, 'sweeper.propose.reclaim-owned-temporary');

  // An unreadable cache descendant must not silently shrink the measured aggregate. Permission
  // bits are the only non-ENOENT directory-read failure a fixture can create deterministically,
  // and root bypasses them, so the check is skipped there instead of asserted vacuously.
  if (process.getuid?.() === 0) {
    t.diagnostic('skipped the unreadable cache-descendant check: root bypasses permission bits');
  } else {
    const locked = await makeFixture({ shape: 'leased' });
    const lockedTarget = join(locked.cache, 'nexus-target-locked');
    await mkdir(lockedTarget, { recursive: true });
    await writeFile(join(lockedTarget, 'locked.bin'), 'locked\n');
    const lockedBefore = await locked.fingerprint();
    chmodSync(lockedTarget, 0o000);
    t.after(async () => {
      chmodSync(lockedTarget, 0o700);
      await locked.teardown();
    });
    const lockedRun = await locked.run();
    assert.equal(lockedRun.code, 1);
    assert.equal(refusalCodes(lockedRun).includes('sweeper.refuse.path-unreadable'), true);
    assert.equal(lockedRun.document.ok, false);
    assert.deepEqual(proposedRefs(lockedRun.document), []);
    chmodSync(lockedTarget, 0o700);
    assert.equal(await locked.fingerprint(), lockedBefore);
  }

  // A symlinked cache path is refused rather than followed.
  const symlinked = await makeFixture({ shape: 'leased' });
  t.after(() => symlinked.teardown());
  await rm(symlinked.ownerTarget, { recursive: true, force: true });
  const detachedTarget = join(symlinked.root, 'detached-target');
  await mkdir(detachedTarget, { recursive: true });
  await writeFile(join(detachedTarget, 'elsewhere.bin'), 'elsewhere\n');
  await symlink(detachedTarget, symlinked.ownerTarget);
  const symlinkBefore = await symlinked.fingerprint();
  const symlinkRun = await symlinked.run();
  assert.equal(symlinkRun.code, 1);
  assert.equal(refusalCodes(symlinkRun).includes('sweeper.refuse.symlink-path'), true);
  assert.deepEqual(proposedRefs(symlinkRun.document), []);
  assert.equal(await symlinked.fingerprint(), symlinkBefore);

  // Invalid invocation is a usage failure, not a document.
  const relative = await symlinked.run(['--check-exit', 'fixture-owner', '--repo', 'relative/path']);
  assert.equal(relative.code, 2);
  assert.equal(relative.document, null);
});

// The temp root is compared canonically, so a receipt naming an in-root path through a system alias
// of that root must stay owned instead of being refused as a stale claim — while a path that
// traverses a link *inside* the root still describes a location the receipt never named.
test('unreadable or stale receipt aliases stay owned', async t => {
  const reclaimTemporary = run => trackOf(run.document, 'fixture-owner').actions.find(action => action.kind === 'reclaim-temporary');
  const ownedReceipt = (run, receipt) => {
    const fact = trackOf(run.document, 'fixture-owner').temporary_paths.find(entry => entry.path === receipt);
    assert.ok(fact, `receipt ${receipt} must be reported`);
    assert.equal(fact.exists, true);
    assert.ok(fact.bytes > 0);
    const action = reclaimTemporary(run);
    assert.equal(action.verdict, 'propose');
    assert.equal(action.reason, 'sweeper.propose.reclaim-owned-temporary');
  };

  // The reported shape: the fixture root is created under `tmpdir()`, so this receipt is spelled
  // through that alias (`/var/...` on Darwin, canonical `/private/var/...`) exactly as an inventory
  // producer would write it.
  const hostAlias = await makeFixture({
    shape: 'leased',
    mutateInventory: (document, paths) => {
      const receipt = join(paths.root, 'receipts', 'owned');
      mkdirSync(receipt, { recursive: true });
      writeFileSync(join(receipt, 'payload.bin'), 'host-aliased receipt\n');
      document.tracks[0].temporary_paths = [receipt];
      return document;
    },
  });
  t.after(() => hostAlias.teardown());
  if (realpathSync(tmpdir()) === tmpdir()) {
    t.diagnostic('this host spells tmpdir() canonically; the system-alias shape is covered by the constructed temp-root alias below');
  }
  const hostAliasBefore = await hostAlias.fingerprint();
  const hostAliasRun = await hostAlias.run();
  assert.equal(hostAliasRun.code, 0, `host-aliased receipt: ${hostAliasRun.stdout}`);
  assert.deepEqual(hostAliasRun.document.refusals, [], `host-aliased receipt: ${hostAliasRun.stdout}`);
  ownedReceipt(hostAliasRun, join(hostAlias.root, 'receipts', 'owned'));
  assert.equal(await hostAlias.fingerprint(), hostAliasBefore);

  // The same shape, constructed from the fixture's own temp root so it holds on every host.
  const aliased = await makeFixture({
    shape: 'leased',
    mutateInventory: (document, paths) => {
      const { aliasTemp } = aliasTempRoot(paths);
      const receipt = join(aliasTemp, 'receipts', 'owned');
      mkdirSync(receipt, { recursive: true });
      writeFileSync(join(receipt, 'payload.bin'), 'aliased receipt\n');
      document.tracks[0].temporary_paths = [receipt];
      return document;
    },
  });
  t.after(() => aliased.teardown());
  aliased.env.TMPDIR = join(realpathSync(aliased.root), 'temp-alias');
  const aliasedReceipt = join(realpathSync(aliased.root), 'temp-alias', 'receipts', 'owned');
  const aliasedBefore = await aliased.fingerprint();
  const aliasedRun = await aliased.run();
  assert.equal(aliasedRun.code, 0, `constructed alias: ${aliasedRun.stdout}`);
  assert.deepEqual(aliasedRun.document.refusals, [], `constructed alias: ${aliasedRun.stdout}`);
  ownedReceipt(aliasedRun, aliasedReceipt);
  assert.equal(await aliased.fingerprint(), aliasedBefore);

  // Alias tolerance covers the root spelling only: a link below the aliased root is still refused,
  // never followed out of the temp root.
  const escaped = await makeFixture({
    shape: 'leased',
    mutateInventory: (document, paths) => {
      const { realTemp, aliasTemp } = aliasTempRoot(paths);
      mkdirSync(join(paths.cache, 'detached-receipt'), { recursive: true });
      writeFileSync(join(paths.cache, 'detached-receipt', 'payload.bin'), 'detached\n');
      symlinkSync(paths.cache, join(realTemp, 'link'), 'dir');
      document.tracks[0].temporary_paths = [join(aliasTemp, 'link', 'detached-receipt')];
      return document;
    },
  });
  t.after(() => escaped.teardown());
  escaped.env.TMPDIR = join(realpathSync(escaped.root), 'temp-alias');
  const escapedBefore = await escaped.fingerprint();
  const escapedRun = await escaped.run();
  assert.equal(escapedRun.code, 2, `escaping link under aliased root: ${escapedRun.stdout}`);
  assert.equal(escapedRun.document.ok, false);
  assert.equal(refusalCodes(escapedRun).includes('sweeper.refuse.stale-temporary'), true, `refusals: ${JSON.stringify(refusalCodes(escapedRun))}`);
  assert.deepEqual(proposedRefs(escapedRun.document), []);
  assert.deepEqual(escapedRun.document.commands, []);
  assert.equal(await escaped.fingerprint(), escapedBefore);

  // An alias chain that leaves the root and re-enters it: `nested-alias` points *below* the root
  // (`root/sub`), `root/sub/escape` points out of the root, and `outside/back` points back at the
  // root, so the receipt's resolved end state is an ordinary in-root path. That end state — and any
  // single canonical `realpath` of the receipt — is identical to the plainly spelled in-root
  // receipt's, so only the order in which the receipt's prefixes entered the root distinguishes
  // them. The search must therefore end on the *first* prefix that reaches the root's interior,
  // never on a later prefix that happens to resolve back to the root.
  const chainedEscape = await makeFixture({
    shape: 'leased',
    mutateInventory: (document, paths) => {
      const { realTemp } = aliasTempRoot(paths);
      const root = realpathSync(paths.root);
      mkdirSync(join(root, 'outside'), { recursive: true });
      mkdirSync(join(realTemp, 'sub'), { recursive: true });
      symlinkSync(join(root, 'outside'), join(realTemp, 'sub', 'escape'), 'dir');
      symlinkSync(realTemp, join(root, 'outside', 'back'), 'dir');
      symlinkSync(join(realTemp, 'sub'), join(root, 'nested-alias'), 'dir');
      const receipt = join(realTemp, 'receipts', 'owned');
      mkdirSync(receipt, { recursive: true });
      writeFileSync(join(receipt, 'payload.bin'), 'chained escape receipt\n');
      document.tracks[0].temporary_paths = [join(root, 'nested-alias', 'escape', 'back', 'receipts', 'owned')];
      return document;
    },
  });
  t.after(() => chainedEscape.teardown());
  chainedEscape.env.TMPDIR = join(realpathSync(chainedEscape.root), 'temp-alias');
  const chainedBefore = await chainedEscape.fingerprint();
  const chainedRun = await chainedEscape.run();
  assert.equal(chainedRun.code, 2, `chained alias escape: ${chainedRun.stdout}`);
  assert.equal(chainedRun.document.ok, false);
  assert.equal(refusalCodes(chainedRun).includes('sweeper.refuse.stale-temporary'), true, `refusals: ${JSON.stringify(refusalCodes(chainedRun))}`);
  assert.deepEqual(proposedRefs(chainedRun.document), []);
  assert.deepEqual(chainedRun.document.commands, []);
  assert.equal(await chainedEscape.fingerprint(), chainedBefore);

  // The receipt is decided on `resolve()`d text but measured and reported by its own spelling, and
  // the kernel resolves a `..` following a link against that link's target. `escape-dir` leaves the
  // root, so `escape-dir/../receipts/owned` denotes an out-of-root location while lexical
  // normalization rewrites it into an ordinary in-root path: a receipt carrying `..` cannot be both
  // decided and used as one location, so it is refused rather than normalized.
  const dotDot = await makeFixture({
    shape: 'leased',
    mutateInventory: (document, paths) => {
      const { realTemp } = aliasTempRoot(paths);
      const root = realpathSync(paths.root);
      mkdirSync(join(root, 'outside'), { recursive: true });
      mkdirSync(join(root, 'receipts', 'owned'), { recursive: true });
      writeFileSync(join(root, 'receipts', 'owned', 'payload.bin'), 'out-of-root receipt\n');
      symlinkSync(join(root, 'outside'), join(realTemp, 'escape-dir'), 'dir');
      document.tracks[0].temporary_paths = [`${join(realTemp, 'escape-dir')}/../receipts/owned`];
      return document;
    },
  });
  t.after(() => dotDot.teardown());
  dotDot.env.TMPDIR = join(realpathSync(dotDot.root), 'temp-alias');
  const dotDotBefore = await dotDot.fingerprint();
  const dotDotRun = await dotDot.run();
  assert.equal(dotDotRun.code, 2, `dot-dot receipt through a link: ${dotDotRun.stdout}`);
  assert.equal(dotDotRun.document.ok, false);
  assert.equal(refusalCodes(dotDotRun).includes('sweeper.refuse.stale-temporary'), true, `refusals: ${JSON.stringify(refusalCodes(dotDotRun))}`);
  assert.deepEqual(proposedRefs(dotDotRun.document), []);
  assert.deepEqual(dotDotRun.document.commands, []);
  assert.equal(await dotDot.fingerprint(), dotDotBefore);
});

// --- P1-T3: guarded apply + the real installed cleanup ---------------------------------------

function actionPairs(track) {
  return track.actions.map(action => [action.kind, action.verdict]);
}

/**
 * T1's native topology in its own disposable repository: a real superproject with a real local
 * submodule (reached through the fixture's own ssh shim, so no global protocol or config is
 * touched) and feature worktrees under `<main>/.worktrees/<name>` initialized by T1's script. This
 * is the shape whose historic main-only pointer made a linked checkout fatal to traverse; the test
 * proves the ACTUAL installed CLI enumerates these trees instead.
 */
async function makeNativeFixture() {
  const root = await realpath(await mkdtemp(join(tmpdir(), 'v1197-p1-t3-native-')));
  const main = join(root, 'main');
  const cache = join(root, 'cache');
  const harness = join(root, 'harness');
  const home = join(root, 'home');
  const shim = join(root, 'fixture-ssh.sh');
  const origin = join(root, 'sub-origin');
  const worktrees = { 'fixture-a': join(main, '.worktrees', 'feature-a'), 'fixture-b': join(main, '.worktrees', 'feature-b') };
  const branches = { 'fixture-a': 'feat/fixture-a', 'fixture-b': 'feat/fixture-b' };
  const targets = { 'fixture-a': join(cache, 'nexus-target-feature-a'), 'fixture-b': join(cache, 'nexus-target-feature-b') };
  const integration = join(main, '.worktrees', 'iteration-fixture');
  const canonicalTarget = join(cache, 'nexus-target');
  const env = { ...GIT_ENV, HOME: home, XDG_CACHE_HOME: cache, GIT_SSH_COMMAND: shim };
  // Hermetic and clean: the fixture's own ssh transport, no user git config, no protocol override.
  const gitEnv = (args, cwd = root) => exec('git', ['-c', 'user.name=fixture', '-c', 'user.email=fixture@example.com', ...args], { cwd, env, maxBuffer: 8 * 1024 * 1024 });
  const tolerant = async (args, cwd) => {
    try {
      return await gitEnv(args, cwd);
    } catch (error) {
      return { stdout: String(error.stdout ?? ''), stderr: String(error.stderr ?? error.message ?? '') };
    }
  };

  await mkdir(join(main, '.worktrees'), { recursive: true });
  await mkdir(join(harness, 'workflows', WORKFLOW), { recursive: true });
  await mkdir(cache, { recursive: true });
  await mkdir(home, { recursive: true });
  await writeFile(shim, SSH_SHIM, { mode: 0o755 });

  await gitEnv(['init', '-q', '--initial-branch=main', origin]);
  await writeFile(join(origin, 'sub.txt'), 'submodule\n');
  await gitEnv(['add', 'sub.txt'], origin);
  await gitEnv(['commit', '-q', '-m', 'sub'], origin);

  await gitEnv(['init', '-q', '--initial-branch=main', main]);
  await writeFile(join(main, 'README.md'), 'fixture\n');
  await gitEnv(['add', 'README.md'], main);
  await gitEnv(['commit', '-q', '-m', 'init'], main);
  await writeFile(join(main, '.gitmodules'), `[submodule "sub"]\n\tpath = sub\n\turl = ssh://localhost${origin}\n`);
  const subHead = (await gitEnv(['-C', origin, 'rev-parse', 'HEAD'])).stdout.trim();
  await gitEnv(['update-index', '--add', '--cacheinfo', `160000,${subHead},sub`], main);
  await gitEnv(['add', '.gitmodules'], main);
  await gitEnv(['commit', '-q', '-m', 'register submodule'], main);
  await gitEnv(['submodule', 'update', '--init', '--recursive'], main);
  await gitEnv(['worktree', 'add', '-q', '-b', branches['fixture-a'], worktrees['fixture-a']], main);
  await gitEnv(['worktree', 'add', '-q', '-b', branches['fixture-b'], worktrees['fixture-b']], main);
  await gitEnv(['worktree', 'add', '-q', '-b', 'iteration/fixture', integration], main);
  // T1's own tool establishes the per-worktree metadata; a failure here fails the test loudly.
  for (const path of Object.values(worktrees)) {
    await exec(process.execPath, [INITIALIZER, '--worktree', path], { cwd: root, env });
  }

  await writeFile(join(harness, 'workflows', WORKFLOW, 'snapshot.json'), `${JSON.stringify({
    schema_version: 1,
    id: WORKFLOW,
    type: 'iteration',
    status: 'running',
    started_at: '2026-01-01T00:00:00.000Z',
    updated_at: '2026-01-01T00:00:00.000Z',
    branch: { base: 'main', integration: 'iteration/fixture', target: 'main' },
    integration_worktree_path: integration,
    plans: [{
      id: 'fixture-plan',
      title: 'Fixture plan',
      file: 'plans/fixture.md',
      status: 'Done',
      metadata: { track_branches: [branches['fixture-a'], branches['fixture-b']] },
    }],
  }, null, 2)}\n`);
  await mkdir(canonicalTarget, { recursive: true });
  await writeFile(join(canonicalTarget, 'canonical.bin'), 'canonical\n');
  for (const trackId of ['fixture-a', 'fixture-b']) {
    await mkdir(targets[trackId], { recursive: true });
    await writeFile(join(targets[trackId], 'target.bin'), `${trackId} target\n`);
  }
  const inventoryPath = join(root, 'inventory.json');
  await writeFile(inventoryPath, `${JSON.stringify({
    version: 1,
    workflow_id: WORKFLOW,
    active_plan_ids: ['fixture-plan'],
    scheduling: { ready_independent_tasks: 2, disk_budget_bytes: BUDGET, per_track_target_estimate_bytes: ESTIMATE },
    tracks: ['fixture-a', 'fixture-b'].map(trackId => ({
      track_id: trackId,
      plan_id: 'fixture-plan',
      worktree: worktrees[trackId],
      branch: branches[trackId],
      target: targets[trackId],
      temporary_paths: [],
      producer_stopped: true,
      state: 'completed',
    })),
  }, null, 2)}\n`);

  const fixture = {
    root,
    main,
    cache,
    harness,
    integration,
    inventory: inventoryPath,
    worktrees,
    branches,
    targets,
    canonicalTarget,
    env,
    git: async (args, cwd) => (await gitEnv(args, cwd)).stdout,
    async run(args = []) {
      const argv = ['--repo', main, '--harness', harness, '--workflow', WORKFLOW, '--inventory', inventoryPath, ...args];
      try {
        const { stdout, stderr } = await exec(process.execPath, [SCRIPT, ...argv], { cwd: root, env, maxBuffer: 16 * 1024 * 1024 });
        return { code: 0, stdout, stderr, document: JSON.parse(stdout) };
      } catch (error) {
        const stdout = String(error.stdout ?? '');
        let document = null;
        try {
          document = JSON.parse(stdout);
        } catch {
          document = null;
        }
        return { code: typeof error.code === 'number' ? error.code : null, stdout, stderr: String(error.stderr ?? ''), document };
      }
    },
    async teardown() {
      // Merge-first fixture policy: both feature branches already sit at the integration head; the
      // explicit merge keeps the non-force `branch -d` below lawful, and any teardown failure is
      // reported instead of masked.
      const failures = [];
      for (const branch of Object.values(branches)) {
        if (!existsSync(integration)) continue;
        const merged = await tolerant(['merge', '--no-edit', '--ff-only', branch], integration);
        if (merged.stderr !== undefined && /fatal|error/i.test(merged.stderr)) failures.push(`merge ${branch}: ${merged.stderr.trim()}`);
      }
      for (const path of [...Object.values(worktrees), integration]) {
        if (!existsSync(path)) continue;
        await tolerant(['worktree', 'remove', path], main);
        // Fixture-owned disposable tree only: Git refuses to remove a submodule-bearing worktree,
        // so drop the fixture's own bytes and let the prune below forget the record.
        if (existsSync(path)) await rm(path, { recursive: true, force: true });
        if (existsSync(path)) failures.push(`worktree path ${path} survived teardown`);
      }
      await tolerant(['worktree', 'prune'], main);
      for (const branch of [...Object.values(branches), 'iteration/fixture']) {
        await tolerant(['branch', '-d', branch], main);
      }
      await rm(root, { recursive: true, force: true });
      if (existsSync(root)) failures.push(`fixture root ${root} survives`);
      assert.deepEqual(failures, [], `native fixture teardown: ${failures.join(' | ')}`);
    },
  };
  return fixture;
}

test('apply reclaims only released merged slice', async t => {
  const fixture = await makeFixture({ shape: 'released', orphans: false });
  t.after(() => fixture.teardown());

  const peerBytesBefore = directoryBytes(fixture.peerTarget);
  const canonicalBytesBefore = directoryBytes(fixture.canonicalTarget);
  const before = await fixture.fingerprint();

  // The dry run is a proposal only: it deletes nothing and still defers to the engine's own permit.
  const dry = await fixture.run();
  assert.equal(dry.code, 0);
  assert.equal(dry.document.mode, 'dry-run');
  assert.equal(trackOf(dry.document, 'fixture-owner').actions.find(action => action.kind === 'engine-worktree-removal').verdict, 'propose');
  assert.equal(await fixture.fingerprint(), before, 'the planning pass must not mutate anything');

  const apply = await fixture.run(['--apply']);
  assert.equal(apply.code, 0, apply.stdout);
  assert.equal(apply.document.mode, 'apply');
  assert.equal(apply.document.ok, true);
  assert.deepEqual(apply.document.refusals, []);

  const owner = trackOf(apply.document, 'fixture-owner');
  assert.deepEqual(actionPairs(owner), [
    ['reclaim-target', 'executed'],
    ['engine-worktree-removal', 'executed'],
    ['prune', 'absent'],
  ], JSON.stringify(owner.actions));
  assert.equal(owner.exit_clean, true);
  assert.equal(owner.target.exists, false);
  assert.equal(owner.worktree.listed, false);
  assert.equal(owner.worktree.exists, false);

  // Engine-first: exactly one `--apply` call for the exact path, and the engine is the thing that
  // removed the worktree and deleted the merged branch.
  const applied = apply.document.commands.filter(record => record.argv.includes('--apply'));
  assert.equal(applied.length, 1);
  assert.deepEqual(applied[0].argv, [
    'mstar-harness', 'worktree', 'cleanup',
    '--workflow', WORKFLOW,
    '--harness', fixture.harness,
    '--worktree', fixture.ownerWorktree,
    '--apply',
  ]);
  assert.equal(applied[0].exit_code, 0);
  assert.match(applied[0].stdout, /apply: removed worktree /);
  assert.match(applied[0].stdout, /apply: deleted branch feat\/fixture-owner/);

  // Re-observed, not assumed: the exact paths are gone, Git no longer lists the worktree, and the
  // branch the engine released is gone too.
  assert.equal(existsSync(fixture.ownerWorktree), false);
  assert.equal(existsSync(fixture.ownerTarget), false);
  assert.equal((await fixture.git(['worktree', 'list', '--porcelain'])).includes(fixture.ownerWorktree), false);
  assert.equal((await fixture.git(['for-each-ref', '--format=%(refname)'])).includes('feat/fixture-owner'), false);

  // The active peer and the shared canonical cache are untouched.
  const peer = trackOf(apply.document, 'fixture-peer');
  assert.deepEqual(peer.actions.map(action => action.verdict), ['protected']);
  assert.equal(peer.target.exists, true);
  assert.equal(existsSync(fixture.peerWorktree), true);
  assert.equal(directoryBytes(fixture.peerTarget), peerBytesBefore);
  assert.equal(directoryBytes(fixture.canonicalTarget), canonicalBytesBefore);

  // The own exit gate passes; global convergence still fails while the peer is live.
  const checkExit = await fixture.run(['--check-exit', 'fixture-owner']);
  assert.equal(checkExit.code, 0);
  assert.equal(checkExit.document.checks.passed, true);
  const convergence = await fixture.run(['--check-convergence']);
  assert.equal(convergence.code, 1);
  assert.equal(convergence.document.checks.passed, false);

  // Idempotent retry: nothing remains, nothing runs, nothing is touched.
  const retry = await fixture.run(['--apply']);
  assert.equal(retry.code, 0, retry.stdout);
  assert.deepEqual(retry.document.commands, []);
  assert.deepEqual(actionPairs(trackOf(retry.document, 'fixture-owner')), [
    ['reclaim-target', 'absent'],
    ['engine-worktree-removal', 'absent'],
  ]);
  assert.equal(directoryBytes(fixture.canonicalTarget), canonicalBytesBefore);
});

test('apply refusal preserves dirty active unmerged and canonical data', async t => {
  const cases = [
    ['dirty worktree', { dirty: true }, 'cleanup.refuse.dirty-worktree'],
    ['live lease', { shape: 'leased' }, 'sweeper.refuse.not-released'],
    ['unmerged branch', { unmerged: true }, 'sweeper.refuse.unmerged-track'],
  ];
  for (const [name, options, expectedReason] of cases) {
    const fixture = await makeFixture({ shape: 'released', orphans: false, ...options });
    t.after(() => fixture.teardown());

    const canonicalBytesBefore = directoryBytes(fixture.canonicalTarget);
    const canonicalHashBefore = createHash('sha256').update(readFileSync(join(fixture.canonicalTarget, 'canonical.bin'))).digest('hex');
    const before = await fixture.fingerprint();

    const apply = await fixture.run(['--apply']);
    assert.equal(apply.code, 1, `${name}: ${apply.stdout}`);
    assert.equal(apply.document.ok, false, name);
    const owner = trackOf(apply.document, 'fixture-owner');
    assert.deepEqual(actionPairs(owner), [['reclaim-footprint', 'refuse']], `${name}: ${JSON.stringify(owner.actions)}`);
    assert.equal(owner.actions[0].reason, expectedReason, name);
    assert.equal(owner.actions[0].detail.length > 0, true, `${name}: the refusal carries its reason`);
    assert.equal(owner.exit_clean, false, name);

    // A refused track loses nothing: its worktree, its dirt, its owned target, the shared canonical
    // cache and the peer's live footprint are all byte-identical.
    assert.equal(existsSync(fixture.ownerWorktree), true, name);
    assert.equal(existsSync(fixture.ownerTarget), true, name);
    assert.equal(existsSync(fixture.peerWorktree), true, name);
    assert.equal(directoryBytes(fixture.canonicalTarget), canonicalBytesBefore, name);
    assert.equal(createHash('sha256').update(readFileSync(join(fixture.canonicalTarget, 'canonical.bin'))).digest('hex'), canonicalHashBefore, name);
    assert.equal(await fixture.fingerprint(), before, `${name}: nothing may change`);
    // No `--apply` was ever handed to the engine for a refused track.
    assert.equal(apply.document.commands.some(record => record.argv.includes('--apply')), false, name);
  }

  // The unmerged slice keeps its unmerged commit, not only its directory.
  const unmerged = await makeFixture({ shape: 'released', orphans: false, unmerged: true });
  t.after(() => unmerged.teardown());
  const unmergedApply = await unmerged.run(['--apply']);
  assert.equal(unmergedApply.code, 1);
  assert.equal(trackOf(unmergedApply.document, 'fixture-owner').actions[0].reason, 'sweeper.refuse.unmerged-track');
  const ownerCommits = (await unmerged.git(['log', '--oneline', 'feat/fixture-owner'])).trim().split('\n');
  const mainCommits = (await unmerged.git(['log', '--oneline', 'main'])).trim().split('\n');
  assert.equal(ownerCommits.length, mainCommits.length + 1, 'the unmerged commit must be preserved');
  assert.equal(existsSync(join(unmerged.ownerWorktree, 'unmerged.txt')), true);

  // A dry-run proposal is never authorization: the exact worktree has to still be a
  // `<dir>/.worktrees/<name>` linked checkout when the action runs, so a checkout recorded outside
  // that shape is refused at action time with zero mutation — even though the dry run proposed it.
  const detached = await makeFixture({ shape: 'reclaimed', planStatus: 'Done', orphans: false, nonCanonicalTrack: true });
  t.after(() => detached.teardown());
  const detachedBefore = await detached.fingerprint();
  const detachedDry = await detached.run();
  assert.equal(detachedDry.code, 0);
  const proposedDetached = trackOf(detachedDry.document, 'fixture-detached');
  assert.deepEqual(actionPairs(proposedDetached), [
    ['reclaim-target', 'propose'],
    ['engine-worktree-removal', 'propose'],
    ['prune-dry-run', 'propose'],
  ], JSON.stringify(proposedDetached.actions));
  const detachedApply = await detached.run(['--apply']);
  assert.equal(detachedApply.code, 1, detachedApply.stdout);
  const detachedTrack = trackOf(detachedApply.document, 'fixture-detached');
  assert.deepEqual(actionPairs(detachedTrack), [['engine-worktree-removal', 'refuse']], JSON.stringify(detachedTrack.actions));
  assert.equal(detachedTrack.actions[0].reason, 'sweeper.refuse.reverify-worktree');
  assert.equal(detachedTrack.exit_clean, false);
  assert.equal(existsSync(detached.detachedWorktree), true);
  assert.equal(existsSync(detached.detachedTarget), true);
  assert.equal(await detached.fingerprint(), detachedBefore, 'the re-verification gate must not mutate');

  // A refused inventory never mutates under `--apply` either: the same fail-closed input that
  // withholds every proposal withholds every action, and the whole fixture stays byte-identical.
  const refusedInventory = await makeFixture({
    shape: 'released',
    orphans: false,
    mutateInventory: document => {
      document.tracks[0].branch = 'feat/fixture-unclaimed';
      return document;
    },
  });
  t.after(() => refusedInventory.teardown());
  const refusedBefore = await refusedInventory.fingerprint();
  const refusedApply = await refusedInventory.run(['--apply']);
  assert.equal(refusedApply.code, 2);
  assert.equal(refusalCodes(refusedApply).includes('sweeper.refuse.stale-branch-claim'), true);
  assert.deepEqual(refusedApply.document.commands, []);
  assert.equal(refusedApply.document.tracks.every(track => track.actions.every(action => action.verdict !== 'executed')), true);
  assert.equal(await refusedInventory.fingerprint(), refusedBefore);
});

test('retry observes partial cleanup and does not touch peers', async t => {
  let receipt = null;
  const fixture = await makeFixture({
    shape: 'partial',
    orphans: false,
    mutateInventory: (document, paths) => {
      receipt = join(receiptBase(paths), 'partial');
      mkdirSync(receipt, { recursive: true });
      writeFileSync(join(receipt, 'payload.bin'), 'partial receipt\n');
      document.tracks[0].temporary_paths = [receipt];
      return document;
    },
  });
  t.after(() => fixture.teardown());

  const peerBytesBefore = directoryBytes(fixture.peerTarget);
  const canonicalBytesBefore = directoryBytes(fixture.canonicalTarget);
  // The engine already released this track's worktree and branch; only the owned cache target and
  // the recorded temporary receipt remain — exactly the partial state a retry has to finish.
  assert.equal(existsSync(fixture.ownerWorktree), false);
  assert.equal(existsSync(receipt), true);

  const apply = await fixture.run(['--apply']);
  assert.equal(apply.code, 0, apply.stdout);
  assert.deepEqual(apply.document.refusals, []);
  const owner = trackOf(apply.document, 'fixture-owner');
  assert.deepEqual(actionPairs(owner), [
    ['reclaim-target', 'executed'],
    ['reclaim-temporary', 'executed'],
    ['engine-worktree-removal', 'absent'],
  ], JSON.stringify(owner.actions));
  assert.equal(owner.exit_clean, true);
  assert.equal(existsSync(fixture.ownerTarget), false);
  assert.equal(existsSync(receipt), false);
  // A retry re-observes instead of replaying: with no listed worktree it never invokes the engine.
  assert.equal(apply.document.commands.some(record => record.argv.includes('cleanup')), false);
  assert.deepEqual(apply.document.commands.map(record => record.argv), [
    ['rm', '-rf', fixture.ownerTarget],
    ['rm', '-rf', receipt],
  ]);

  // The peer keeps its worktree, its target and its bytes.
  assert.equal(trackOf(apply.document, 'fixture-peer').actions.every(action => action.verdict === 'protected'), true);
  assert.equal(existsSync(fixture.peerWorktree), true);
  assert.equal(directoryBytes(fixture.peerTarget), peerBytesBefore);

  // Second retry: every fact reads absent, so nothing runs and nothing changes.
  const retry = await fixture.run(['--apply']);
  assert.equal(retry.code, 0, retry.stdout);
  assert.deepEqual(actionPairs(trackOf(retry.document, 'fixture-owner')), [
    ['reclaim-target', 'absent'],
    ['reclaim-temporary', 'absent'],
    ['engine-worktree-removal', 'absent'],
  ]);
  assert.deepEqual(retry.document.commands, []);
  assert.equal(directoryBytes(fixture.peerTarget), peerBytesBefore);
  assert.equal(directoryBytes(fixture.canonicalTarget), canonicalBytesBefore);
  const checkExit = await fixture.run(['--check-exit', 'fixture-owner']);
  assert.equal(checkExit.code, 0);
});

test('actual cleanup dry-run enumerates initialized worktrees', async t => {
  const fixture = await makeNativeFixture();
  t.after(() => fixture.teardown());
  const paths = [['fixture-a', fixture.worktrees['fixture-a']], ['fixture-b', fixture.worktrees['fixture-b']]];

  // Phase 1 — the ACTUAL installed engine, driven by the sweeper's dry run, enumerates both freshly
  // initialized linked worktrees as removable without the historic gitdir traversal fatal.
  const dry = await fixture.run();
  assert.equal(dry.code, 0, dry.stdout);
  assert.deepEqual(dry.document.refusals, []);
  assert.equal(dry.document.commands.length, 2);
  for (const [trackId, path] of paths) {
    const record = dry.document.commands.find(candidate => candidate.argv.includes(path));
    assert.ok(record, `${trackId}: the engine was probed for the exact path`);
    assert.deepEqual(record.argv.slice(-2), ['--worktree', path]);
    assert.equal(record.exit_code, 0, `${trackId}: ${record.stderr}`);
    assert.equal(record.spawn_error, null);
    assert.equal(
      record.stdout.split('\n').some(line => line === `remove | worktree | ${path} | cleanup.remove.merged`),
      true,
      `${trackId}: ${record.stdout}`,
    );
    assert.equal(/fatal:|not a git repository|submodule/.test(`${record.stdout}${record.stderr}`), false, `${trackId}: ${record.stderr}`);
    // The measured reason the engine's own removal is inadmissible here is already reported.
    const probed = trackOf(dry.document, trackId);
    assert.equal(probed.worktree.submodule_gitlinks >= 1, true);
    assert.equal(probed.worktree.removal_blocked_by_submodules, true);
    // T1's native metadata is real and resolves per checkout: the historic shape was a pointer into
    // the main-only module directory, which made traversal from a linked checkout fatal.
    assert.equal(
      (await fixture.git(['-C', join(path, 'sub'), 'rev-parse', '--absolute-git-dir'])).trim(),
      join(fixture.main, '.git', 'worktrees', basename(path), 'modules', 'sub'),
    );
    assert.equal((await fixture.git(['-C', path, 'status', '--porcelain'], fixture.root)).trim(), '');
  }

  // Phase 2 — guarded apply on the same topology: the measured submodule refusal is reported
  // verbatim and routed around with the documented exact-path non-force route. The retained branch
  // keeps this apply INCOMPLETE (exit 1): exit 0 means every requested completed track was fully
  // reclaimed, and a branch the engine could not release is unreclaimed state — reported truthfully,
  // never deleted by this tool.
  const apply = await fixture.run(['--apply']);
  assert.equal(apply.code, 1, apply.stdout);
  assert.equal(apply.document.ok, false);
  assert.deepEqual(apply.document.refusals, []);
  for (const [trackId, path] of paths) {
    const track = trackOf(apply.document, trackId);
    assert.deepEqual(actionPairs(track), [
      ['reclaim-target', 'executed'],
      ['engine-worktree-removal', 'blocked'],
      ['fallback-worktree-removal', 'executed'],
      ['prune', 'executed'],
      ['engine-branch-removal', 'retained'],
    ], `${trackId}: ${JSON.stringify(track.actions)}`);
    assert.equal(track.exit_clean, false);
    assert.equal(track.worktree.branch_present, true);
    const blocked = track.actions.find(action => action.verdict === 'blocked');
    assert.equal(blocked.reason, 'sweeper.blocked.submodule-gitlinks');
    assert.match(blocked.detail, /working trees containing submodules cannot be moved or removed/);
    assert.equal(track.worktree.listed, false);
    assert.equal(track.worktree.exists, false);
    assert.equal(track.target.exists, false);
  }
  // The branch rows are truthful for the moment each track's own re-observation measured them, and
  // this tool deletes no branch itself (no branch command is ever run). Measured engine side effect
  // worth naming: the engine's later workflow-scoped `--apply` for the sibling deletes the merged
  // branch it can now reach once this track's worktree record is pruned, so only the last track's
  // branch survives to the end of the run.
  assert.deepEqual(
    (await fixture.git(['for-each-ref', '--format=%(refname)'], fixture.main)).trim().split('\n').sort(),
    ['refs/heads/feat/fixture-b', 'refs/heads/iteration/fixture', 'refs/heads/main'],
  );
  assert.equal(apply.document.commands.some(record => record.argv.includes('branch')), false);

  // The refusal is never suppressed: both `--apply` invocations failed with the measured message,
  // and the fallback ran only after that refusal.
  const refused = apply.document.commands.filter(record => record.argv.includes('--apply') && record.exit_code === 1);
  assert.equal(refused.length, 2);
  for (const record of refused) {
    assert.match(record.stderr, /apply: failed worktree .*: fatal: working trees containing submodules cannot be moved or removed/);
  }
  // Never a force flag, never `git submodule deinit` (which unregisters the SHARED superproject
  // configuration), never a wildcard.
  for (const record of apply.document.commands) {
    assert.equal(record.argv.includes('--force'), false, record.argv.join(' '));
    assert.equal(record.argv.includes('-f'), false, record.argv.join(' '));
    assert.equal(record.argv.some(argument => argument.includes('deinit')), false, record.argv.join(' '));
    assert.equal(record.argv.some(argument => argument.includes('*')), false, record.argv.join(' '));
  }
  // Exactly the scoped, exact-path removals: each owned target and each of the two worktree paths.
  assert.deepEqual(
    apply.document.commands.filter(record => record.argv[0] === 'rm').map(record => record.argv),
    [
      ['rm', '-rf', fixture.targets['fixture-a']],
      ['rm', '-rf', fixture.worktrees['fixture-a']],
      ['rm', '-rf', fixture.targets['fixture-b']],
      ['rm', '-rf', fixture.worktrees['fixture-b']],
    ],
  );
  // The scoped prune dry run named only this track's record before the actual prune ran.
  const pruneDrys = apply.document.commands.filter(record => record.argv.join(' ') === 'git worktree prune --dry-run --verbose');
  assert.equal(pruneDrys.length, 2);
  for (const record of pruneDrys) {
    assert.equal(/Removing worktrees\/feature-[ab]: /.test(`${record.stdout}${record.stderr}`), true, `${record.stdout}${record.stderr}`);
  }

  // Raw re-observation: exact paths absent, no record left, main submodule registration intact.
  for (const [, path] of paths) assert.equal(existsSync(path), false);
  const listed = await fixture.git(['worktree', 'list', '--porcelain'], fixture.main);
  assert.equal(listed.includes('feature-a'), false);
  assert.equal(listed.includes('feature-b'), false);
  assert.equal(listed.includes('iteration-fixture'), true);
  assert.equal(existsSync(join(fixture.main, '.git', 'worktrees', 'feature-a')), false);
  assert.equal(existsSync(join(fixture.main, '.git', 'worktrees', 'feature-b')), false);
  assert.equal((await fixture.git(['config', '--get', 'submodule.sub.active'], fixture.main)).trim(), 'true');
  const submoduleStatus = (await fixture.git(['submodule', 'status'], fixture.main)).trim();
  assert.equal(submoduleStatus.startsWith('-'), false, `submodule unregistered: ${submoduleStatus}`);
  assert.match(submoduleStatus, / sub /);
  assert.equal(existsSync(join(fixture.main, 'sub', 'sub.txt')), true);
  assert.equal(directoryBytes(fixture.canonicalTarget) > 0, true);

  // Own exit and the convergence checkpoint both report the branch the engine could not release as
  // the residual, with it named, instead of certifying the slice reclaimed.
  const checkExit = await fixture.run(['--check-exit', 'fixture-b']);
  assert.equal(checkExit.code, 1, checkExit.stdout);
  assert.equal(checkExit.document.checks.passed, false);
  assert.deepEqual(checkExit.document.checks.reasons.map(reason => reason.code), ['sweeper.check.branch-present']);
  assert.match(checkExit.document.checks.reasons[0].detail, /feat\/fixture-b still exists/);
  const convergence = await fixture.run(['--check-convergence']);
  assert.equal(convergence.code, 1, JSON.stringify(convergence.document.checks));
  assert.deepEqual(convergence.document.checks.reasons.map(reason => reason.code), ['sweeper.check.branch-present']);

  // No fixture orphan remains, and teardown reports rather than masks a leftover.
  await fixture.teardown();
});

// --- P1-T3 fix round 1: retained branch, per-mutation gate, ignored-only footprint, contract text --

test('a retained branch keeps the apply exit incomplete', async t => {
  // The measured shape: the engine released this slice's worktree but the track's branch survives.
  // Before this fix the aggregate ignored the branch, so this fixture exited 0 with
  // `exit_clean: true` — a successful apply for state that was never reclaimed.
  const fixture = await makeFixture({ shape: 'partial', keepBranch: true, orphans: false });
  t.after(() => fixture.teardown());

  const apply = await fixture.run(['--apply']);
  assert.equal(apply.code, 1, apply.stdout);
  assert.equal(apply.document.ok, false);
  // The residual is reported, not a refusal: nothing about this run was inadmissible.
  assert.deepEqual(apply.document.refusals, []);
  const owner = trackOf(apply.document, 'fixture-owner');
  assert.deepEqual(actionPairs(owner), [
    ['reclaim-target', 'executed'],
    ['engine-worktree-removal', 'absent'],
  ], JSON.stringify(owner.actions));
  assert.equal(owner.exit_clean, false);
  assert.equal(owner.worktree.branch_present, true);
  assert.equal(owner.worktree.listed, false);
  assert.equal(owner.target.exists, false);

  // The branch is untouched and no branch deletion was ever attempted — the boundary stays with the
  // PM's workflow-level checkpoint.
  assert.equal(
    (await fixture.git(['for-each-ref', '--format=%(refname)', 'refs/heads/feat/fixture-owner'])).trim(),
    'refs/heads/feat/fixture-owner',
  );
  assert.equal(apply.document.commands.some(record => record.argv.includes('branch')), false);

  // The read-only exit gate reports the very same residual, and the reason names the branch.
  const checkExit = await fixture.run(['--check-exit', 'fixture-owner']);
  assert.equal(checkExit.code, 1);
  assert.equal(checkExit.document.checks.passed, false);
  assert.deepEqual(checkExit.document.checks.reasons.map(reason => reason.code), ['sweeper.check.branch-present']);
  assert.match(checkExit.document.checks.reasons[0].detail, /feat\/fixture-owner still exists/);
});

test('the re-verification gate stops the receipt removal on a changed worktree fact', async t => {
  // A fact moves between the target removal and the receipt removal. The receipt's own check is not
  // enough: the whole footprint is re-proved immediately before that mutation, so the receipt, the
  // engine handover and the fallback all stay unmade.
  let receipt = null;
  const fixture = await makeFixture({
    shape: 'released',
    orphans: false,
    mutateInventory: (document, paths) => {
      receipt = join(receiptBase(paths), 'gate-round');
      mkdirSync(receipt, { recursive: true });
      writeFileSync(join(receipt, 'payload.bin'), 'gate receipt\n');
      document.tracks[0].temporary_paths = [receipt];
      return document;
    },
  });
  t.after(() => fixture.teardown());
  const shim = await rmShim(fixture.root, {
    after: fixture.ownerTarget,
    mutate: `git -C ${JSON.stringify(fixture.ownerWorktree)} checkout --quiet --detach`,
  });
  fixture.env.PATH = `${shim.directory}:${process.env.PATH}`;

  const apply = await fixture.run(['--apply']);
  assert.equal(apply.code, 1, apply.stdout);
  assert.equal(existsSync(shim.marker), true, 'the injected fact change must have fired');
  const owner = trackOf(apply.document, 'fixture-owner');
  assert.deepEqual(actionPairs(owner), [
    ['reclaim-target', 'executed'],
    ['reclaim-temporary', 'refuse'],
    ['engine-worktree-removal', 'refuse'],
  ], JSON.stringify(owner.actions));
  for (const refusal of owner.actions.filter(action => action.verdict === 'refuse')) {
    assert.equal(refusal.reason, 'sweeper.refuse.reverify-worktree', JSON.stringify(refusal));
  }
  // Zero further mutation: the receipt keeps its bytes, the worktree keeps its branch, and only the
  // already-finished target removal sits in the command log — no `--apply` was ever handed over.
  assert.equal(existsSync(receipt), true);
  assert.equal(existsSync(fixture.ownerWorktree), true);
  assert.equal((await fixture.git(['rev-parse', '--abbrev-ref', 'HEAD'], fixture.ownerWorktree)).trim(), 'HEAD');
  assert.equal(
    (await fixture.git(['for-each-ref', '--format=%(refname)', 'refs/heads/feat/fixture-owner'], fixture.main)).trim(),
    'refs/heads/feat/fixture-owner',
  );
  assert.deepEqual(
    apply.document.commands.filter(record => record.argv[0] === 'rm').map(record => record.argv),
    [['rm', '-rf', fixture.ownerTarget]],
  );
  assert.equal(apply.document.commands.some(record => record.argv.includes('--apply')), false);
});

test('the re-verification gate stops the non-force fallback on a changed fact', async t => {
  // A fact moves while the engine is running: the change lands after the gate that allowed the
  // handover and reaches the gate inside the non-force fallback, so the fallback's `rm -rf` never
  // runs. The wrapper delegates every invocation to the actual installed CLI, whose measured
  // submodule refusal is what routes the run into that fallback.
  const native = await makeNativeFixture();
  t.after(() => native.teardown());
  const decoy = join(native.root, 'decoy-target');
  await mkdir(join(decoy, 'sub'), { recursive: true });
  const wrapper = await engineApplyWrapper(native.root, {
    mutate: `/bin/rm -rf ${JSON.stringify(native.targets['fixture-a'])} && ln -s ${JSON.stringify(decoy)} ${JSON.stringify(native.targets['fixture-a'])}`,
  });
  native.env.PATH = `${wrapper.directory}:${process.env.PATH}`;

  const injected = await native.run(['--apply']);
  assert.equal(existsSync(wrapper.marker), true, 'the injected fact change must have fired');
  assert.equal(injected.code, 1, injected.stdout);
  assert.equal(injected.document.ok, false);
  const trackA = trackOf(injected.document, 'fixture-a');
  assert.deepEqual(actionPairs(trackA), [
    ['reclaim-target', 'executed'],
    ['engine-worktree-removal', 'blocked'],
    ['fallback-worktree-removal', 'refuse'],
  ], JSON.stringify(trackA.actions));
  assert.equal(trackA.actions[2].reason, 'sweeper.refuse.reverify-target');
  // The engine WAS handed this exact path — the change happened after that gate — and it refused it
  // with the measured submodule message.
  const handovers = injected.document.commands.filter(record => record.argv.includes('--apply'));
  assert.equal(handovers.some(record => record.argv.includes(native.worktrees['fixture-a']) && record.exit_code === 1), true);
  // Zero further mutation for the affected track: worktree, checked-out submodule content and the
  // Git record all survive, with no fallback removal and no prune.
  assert.equal(existsSync(join(native.worktrees['fixture-a'], 'sub', 'sub.txt')), true);
  assert.equal((await native.git(['worktree', 'list', '--porcelain'], native.main)).includes('feature-a'), true);
  assert.deepEqual(
    injected.document.commands.filter(record => record.argv[0] === 'rm' && record.argv[2] === native.worktrees['fixture-a']),
    [],
  );
  // No prune ever touched the affected track's record (the sibling's own scoped prune still runs).
  assert.equal(
    injected.document.commands.some(record => record.argv[0] === 'git' && record.argv[1] === 'worktree' && record.argv[2] === 'prune' && `${record.stdout}${record.stderr}`.includes('feature-a')),
    false,
  );
  // The unaffected sibling track still completed its own reclamation in the same run.
  assert.deepEqual(actionPairs(trackOf(injected.document, 'fixture-b')), [
    ['reclaim-target', 'executed'],
    ['engine-worktree-removal', 'blocked'],
    ['fallback-worktree-removal', 'executed'],
    ['prune', 'executed'],
    ['engine-branch-removal', 'retained'],
  ], JSON.stringify(trackOf(injected.document, 'fixture-b').actions));
  await native.teardown();
});

test('apply routes around an ignored-only footprint and refuses real dirt', async t => {
  // The second measured cleanup obstacle: the engine reports the worktree dirty although the tracked
  // tree is clean, because build preparation left ignored outputs inside it. Exception before the
  // fix: a permanent refusal. Now the exact ignored paths and sizes are enumerated and reported, and
  // the documented exact-path non-force route reclaims the slice.
  const ignored = await makeFixture({ shape: 'released', orphans: false, ignoredOutputs: true });
  t.after(() => ignored.teardown());
  const nodeModulesBytes = directoryBytes(join(ignored.ownerWorktree, 'node_modules'));
  const distBytes = directoryBytes(join(ignored.ownerWorktree, 'dist'));
  assert.equal((await ignored.git(['status', '--porcelain', '--untracked-files=no'], ignored.ownerWorktree)).trim(), '', 'the tracked tree must be clean');
  const canonicalWorktree = realpathSync(ignored.ownerWorktree);

  const apply = await ignored.run(['--apply']);
  // The reclamation happens, so the only residual is the branch the engine could not release (the
  // engine declines branch candidates for a pruned exact path) — which is exactly the exit-1 rule.
  assert.equal(apply.code, 1, apply.stdout);
  assert.equal(apply.document.ok, false);
  assert.deepEqual(apply.document.refusals, []);
  const owner = trackOf(apply.document, 'fixture-owner');
  // Reclaimed through the documented route: the exact target and the exact worktree path are gone,
  // the record is pruned, and the engine's dirt refusal is reported as `blocked` rather than as the
  // permanent refusal it used to be.
  assert.deepEqual(actionPairs(owner), [
    ['reclaim-target', 'executed'],
    ['engine-worktree-removal', 'blocked'],
    ['fallback-worktree-removal', 'executed'],
    ['prune', 'executed'],
    ['engine-branch-removal', 'retained'],
  ], JSON.stringify(owner.actions));
  const blocked = owner.actions.find(action => action.verdict === 'blocked');
  assert.equal(blocked.reason, 'sweeper.blocked.ignored-outputs');
  assert.equal(blocked.kind, 'engine-worktree-removal');
  assert.match(blocked.detail, /cleanup\.refuse\.dirty-worktree/);
  assert.deepEqual(blocked.ignored_footprint, {
    paths: [
      { path: join(ignored.ownerWorktree, 'dist'), bytes: distBytes },
      { path: join(ignored.ownerWorktree, 'node_modules'), bytes: nodeModulesBytes },
    ],
    total_bytes: distBytes + nodeModulesBytes,
  });
  // The engine's own refusal is in the recorded evidence verbatim, not summarised away. The engine
  // prints its canonical path, so the check uses the fixture's real path.
  assert.equal(
    apply.document.commands.some(record => record.stdout.includes(`refuse | worktree | ${canonicalWorktree} | cleanup.refuse.dirty-worktree`)),
    true,
  );
  // Reclaimed through the documented route: the exact target and the exact worktree path are gone,
  // the record is pruned, and nothing was forced, deinitialized or wildcarded.
  assert.equal(existsSync(ignored.ownerWorktree), false);
  assert.equal(existsSync(ignored.ownerTarget), false);
  assert.equal(owner.worktree.listed, false);
  assert.equal(owner.worktree.branch_present, true);
  assert.equal(owner.exit_clean, false);
  for (const record of apply.document.commands) {
    assert.equal(record.argv.includes('--force'), false, record.argv.join(' '));
    assert.equal(record.argv.some(argument => argument.includes('deinit')), false, record.argv.join(' '));
    assert.equal(record.argv.some(argument => argument.includes('*')), false, record.argv.join(' '));
  }

  // Tracked dirt is still a refusal with zero mutation, ignored outputs next to it or not.
  for (const [name, extra] of [['tracked dirt', {}], ['tracked dirt beside ignored outputs', { ignoredOutputs: true }]]) {
    const dirty = await makeFixture({ shape: 'released', orphans: false, dirty: true, ...extra });
    t.after(() => dirty.teardown());
    const before = await dirty.fingerprint();
    const dirtyApply = await dirty.run(['--apply']);
    assert.equal(dirtyApply.code, 1, `${name}: ${dirtyApply.stdout}`);
    assert.deepEqual(actionPairs(trackOf(dirtyApply.document, 'fixture-owner')), [['reclaim-footprint', 'refuse']], name);
    assert.equal(trackOf(dirtyApply.document, 'fixture-owner').actions[0].reason, 'cleanup.refuse.dirty-worktree', name);
    assert.equal(existsSync(dirty.ownerTarget), true, name);
    assert.equal(existsSync(dirty.ownerWorktree), true, name);
    assert.equal(await dirty.fingerprint(), before, `${name}: nothing may change`);
  }

  // An untracked path that is NOT ignored is real dirt too: the ignored-only route never reaches it.
  const loose = await makeFixture({
    shape: 'released',
    orphans: false,
    ignoredOutputs: true,
    mutateInventory: (document, paths) => {
      writeFileSync(join(paths.ownerWorktree, 'loose.txt'), 'untracked and not ignored\n');
      return document;
    },
  });
  t.after(() => loose.teardown());
  const looseBefore = await loose.fingerprint();
  const looseApply = await loose.run(['--apply']);
  assert.equal(looseApply.code, 1, looseApply.stdout);
  assert.deepEqual(actionPairs(trackOf(looseApply.document, 'fixture-owner')), [['reclaim-footprint', 'refuse']]);
  assert.equal(trackOf(looseApply.document, 'fixture-owner').actions[0].reason, 'cleanup.refuse.dirty-worktree');
  assert.equal(existsSync(join(loose.ownerWorktree, 'loose.txt')), true);
  assert.equal(await loose.fingerprint(), looseBefore);
});

test('the documented inventory contract requires canonical feature paths', async t => {
  // G2's inventory wording has to say what the apply-time shape check enforces: recorded feature
  // worktree paths are canonical `<repo>/.worktrees/<name>` linked checkouts. `--help` is the
  // contract text this tool documents.
  const help = await runScript(['--help']);
  assert.equal(help.code, 0, help.stderr);
  assert.match(help.stdout, /canonical `<repo>\/\.worktrees\/<name>` linked checkout/);
  assert.match(help.stdout, /is not a reclaimable\s+feature path/);
  // The dry run still reports another shape; the action-time refusal is covered by the non-canonical
  // regression in 'apply refusal preserves dirty active unmerged and canonical data'.
  assert.match(help.stdout, /sweeper\.refuse\.reverify-worktree/);
});
