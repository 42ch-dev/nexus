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
 *   node --test --test-name-pattern='inventory protects|check-exit distinguishes|capacity watermarks|unreadable or stale' scripts/worktree-sweep.test.mjs
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
 *     a link.
 */
import { strict as assert } from 'node:assert';
import { execFile } from 'node:child_process';
import { createHash } from 'node:crypto';
import { chmodSync, existsSync, mkdirSync, readFileSync, readlinkSync, readdirSync, realpathSync, statSync, symlinkSync, writeFileSync } from 'node:fs';
import { mkdir, mkdtemp, rm, symlink, writeFile } from 'node:fs/promises';
import { cpus, tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';

import { computeAvailableK, evaluateWatermarks } from './worktree-sweep.mjs';

const exec = promisify(execFile);
const SCRIPT = fileURLToPath(new URL('./worktree-sweep.mjs', import.meta.url));
const WORKFLOW = 'fixture-wf';
const GIB = 1024 ** 3;
const BUDGET = 120 * GIB;
const ESTIMATE = 20 * GIB;
const SIBLING_ID = 'fixture-sibling';

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
  const root = await mkdtemp(join(tmpdir(), 'v1197-p1-t2-'));
  const main = join(root, 'main');
  const cache = join(root, 'cache');
  const harness = join(root, 'harness');
  const home = join(root, 'home');
  const ownerWorktree = join(root, '.worktrees', 'fixture-owner');
  const peerWorktree = join(root, '.worktrees', 'fixture-peer');
  const integrationWorktree = join(root, '.worktrees', 'iteration-fixture');
  const ownerTarget = join(cache, 'nexus-target-fixture-owner');
  const peerTarget = join(cache, 'nexus-target-fixture-peer');
  const canonicalTarget = join(cache, 'nexus-target');
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
  await git(['worktree', 'add', '-b', 'iteration/fixture', integrationWorktree], main);
  await git(['worktree', 'add', '-b', 'feat/fixture-owner', ownerWorktree], main);
  await git(['worktree', 'add', '-b', 'feat/fixture-peer', peerWorktree], main);
  await git(['worktree', 'add', '--detach', join(root, '.worktrees', 'fixture-orphan')], main);

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
        status: shape === 'released' ? 'Done' : 'InProgress',
        metadata: { track_branches: ['feat/fixture-owner', 'feat/fixture-peer'] },
        ...(shape === 'leased'
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
  await mkdir(join(cache, 'nexus-target-orphan-leftover'), { recursive: true });
  await writeFile(join(cache, 'nexus-target-orphan-leftover', 'junk.bin'), 'orphan\n');

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
  if (options.mutateInventory !== undefined) inventory = options.mutateInventory(inventory, paths);
  const inventoryPath = join(root, 'inventory.json');
  await writeFile(inventoryPath, `${JSON.stringify(inventory, null, 2)}\n`);

  if (shape === 'reclaimed') {
    await git(['worktree', 'remove', ownerWorktree], main);
    await git(['branch', '-d', 'feat/fixture-owner'], main);
    await rm(ownerTarget, { recursive: true, force: true });
  }

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
    env: { ...process.env, HOME: home, XDG_CACHE_HOME: cache },
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
      for (const path of [ownerWorktree, peerWorktree, integrationWorktree, join(root, '.worktrees', 'fixture-orphan')]) {
        if (!existsSync(path)) continue;
        await gitTolerant(['worktree', 'remove', path], main);
        // Test-owned disposable trees only: Git refuses to remove a worktree holding submodule
        // gitlinks, so drop the fixture's own bytes and let the prune below forget the entry.
        if (existsSync(path)) await rm(path, { recursive: true, force: true });
      }
      await gitTolerant(['worktree', 'prune'], main);
      for (const branch of ['feat/fixture-owner', 'feat/fixture-peer', 'iteration/fixture']) {
        await gitTolerant(['branch', '-d', branch], main);
      }
      await rm(root, { recursive: true, force: true });
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

  const apply = await reclaimed.run(['--apply']);
  assert.equal(apply.code, 2);
  assert.equal(apply.document, null);
  assert.match(apply.stderr, /--apply is unavailable/);

  const applyWithCheck = await reclaimed.run(['--apply', '--check-convergence']);
  assert.equal(applyWithCheck.code, 2);
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
