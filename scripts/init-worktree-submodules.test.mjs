#!/usr/bin/env node
/**
 * Focused contract tests for the G1 linked-worktree submodule initializer.
 *
 *   node --test scripts/init-worktree-submodules.test.mjs
 *
 * Scope: the initializer's own observable contract — native per-checkout
 * metadata, validated no-op repeats that preserve an intentional pin, and
 * fail-closed refusals — against a hermetic temporary Git superproject this
 * file creates. Fail-closed coverage includes unresolved index entries
 * (`.gitmodules` and gitlinks, with and without the working-tree file) and
 * operational filesystem failures, which must keep the JSON stdout contract.
 * The refusal object also has to tell the truth about what a failure left
 * behind: `init.refuse.preflight` (this run attempted no mutation) is
 * distinguished from `init.refuse.partial` (native Git had already initialized
 * at least one submodule, nothing is rolled back, and `initialized_paths` names
 * the paths it left in place). Nothing here touches the product repository, its
 * `.agents` submodule, a package manager or the network.
 *
 * Fixture transport: Git has refused the `file` protocol for submodule clones
 * since 2.38, and that policy is read from protected config, so a
 * repository-local setting cannot relax it. The fixture therefore publishes its
 * local submodule origins as `ssh://localhost/<absolute-path>` URLs and hands
 * its git calls an ssh shim (`GIT_SSH_COMMAND`) that runs `git-upload-pack` on
 * this machine. Every clone stays local, no protocol override is needed
 * anywhere, and the initializer under test gets a clean environment.
 *
 * Fixture lifetime: linked worktrees are removed with non-force `worktree
 * remove` + `prune` after the fixture-created per-worktree submodule metadata is
 * deinitialized, branches are deleted with `branch -d`, absence of every owned
 * directory is proven, and only then is the temporary root deleted. A teardown
 * failure keeps the root and fails the test instead of hiding behind it.
 */

import { strict as assert } from 'node:assert';
import { execFile, spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync } from 'node:fs';
import { chmod, mkdir, mkdtemp, readFile, readlink, realpath, rename, rm, symlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { basename, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { after, before, test } from 'node:test';

const execFileAsync = promisify(execFile);

const SCRIPT = fileURLToPath(new URL('./init-worktree-submodules.mjs', import.meta.url));
const GITLINK_MODE = '160000';
const FEATURE_A = 'feature-a';
const FEATURE_B = 'feature-b';

const SSH_SHIM = `#!/bin/sh
# Fixture-local ssh transport: run the remote git command on this machine.
while [ $# -gt 0 ]; do
  case "$1" in
    -o|-p|-i|-F|-l|-c|-m|-e|-b|-E|-I|-L|-R|-Q|-S|-W|-w) shift 2 ;;
    -*) shift ;;
    *) shift; break ;;
  esac
done
exec sh -c "$*"
`;

/**
 * Git plumbing that writes index entries (`update-index --index-info`) reads its
 * payload from stdin, which `execFile` cannot feed.
 */
function runWithInput(file, args, options, input) {
  return new Promise((resolve, reject) => {
    const child = spawn(file, args, options);
    let stdout = '';
    let stderr = '';
    child.stdout.setEncoding('utf8').on('data', chunk => { stdout += chunk; });
    child.stderr.setEncoding('utf8').on('data', chunk => { stderr += chunk; });
    child.on('error', reject);
    child.on('close', code => resolve({ ok: code === 0, code: code ?? -1, stdout, stderr }));
    child.stdin.on('error', () => {});
    child.stdin.end(input);
  });
}

async function run(file, args, options = {}) {
  const { input, ...execOptions } = options;
  if (input !== undefined) return await runWithInput(file, args, execOptions, input);
  try {
    const { stdout, stderr } = await execFileAsync(file, args, { encoding: 'utf8', maxBuffer: 32 * 1024 * 1024, ...execOptions });
    return { ok: true, code: 0, stdout, stderr };
  } catch (error) {
    return {
      ok: false,
      code: typeof error.code === 'number' ? error.code : -1,
      stdout: typeof error.stdout === 'string' ? error.stdout : '',
      stderr: typeof error.stderr === 'string' ? error.stderr : String(error.message ?? error),
    };
  }
}

function firstLine(text) {
  for (const line of String(text ?? '').split('\n')) {
    if (line.trim() !== '') return line.trim();
  }
  return '';
}

function record(label, command, result) {
  const stdout = result.stdout.trim() === '' ? '' : ` | stdout: ${result.stdout.trim().split('\n').join(' ; ')}`;
  const stderr = result.stderr.trim() === '' ? '' : ` | stderr: ${firstLine(result.stderr)}`;
  console.log(`EVIDENCE ${label}: ${command} => exit ${result.code}${stdout}${stderr}`);
}

async function canonical(cwd, raw) {
  return await realpath(resolve(cwd, String(raw).trim()));
}

async function digest(file) {
  return createHash('sha256').update(await readFile(file)).digest('hex');
}

class WorktreeFixture {
  static async create() {
    const root = await realpath(await mkdtemp(join(tmpdir(), 'v1197-p1-t1-')));
    const fixture = new WorktreeFixture(root);
    await fixture.build();
    return fixture;
  }

  constructor(root) {
    this.root = root;
    this.shim = join(root, 'fixture-ssh.sh');
    this.nestedOrigin = join(root, 'nested-origin');
    this.subOrigin = join(root, 'sub-origin');
    this.superproject = join(root, 'superproject');
    this.worktreesRoot = join(this.superproject, '.worktrees');
    this.extraWorktrees = [];
    // Hermetic and clean: the fixture's own ssh transport, no user git config,
    // no protocol override, no index side effects, no credential prompt.
    this.env = {
      ...process.env,
      GIT_SSH_COMMAND: this.shim,
      GIT_CONFIG_GLOBAL: '/dev/null',
      GIT_OPTIONAL_LOCKS: '0',
      GIT_TERMINAL_PROMPT: '0',
      GIT_AUTHOR_NAME: 'fixture',
      GIT_AUTHOR_EMAIL: 'fixture@example.invalid',
      GIT_COMMITTER_NAME: 'fixture',
      GIT_COMMITTER_EMAIL: 'fixture@example.invalid',
    };
  }

  worktree(name) {
    return join(this.worktreesRoot, name);
  }

  worktreeGitDir(name) {
    return join(this.superproject, '.git', 'worktrees', name);
  }

  originUrl(absolutePath) {
    return `ssh://localhost${absolutePath}`;
  }

  modulesEntry(name, path, absoluteOrigin) {
    return `[submodule "${name}"]\n\tpath = ${path}\n\turl = ${this.originUrl(absoluteOrigin)}\n`;
  }

  async git(args, { cwd = this.superproject } = {}) {
    return await run('git', args, { cwd, env: this.env });
  }

  async gitOk(args, options) {
    const result = await this.git(args, options);
    assert.equal(result.ok, true, `git ${args.join(' ')} failed: ${firstLine(result.stderr)}`);
    return result.stdout;
  }

  async head(dir) {
    return (await this.gitOk(['-C', dir, 'rev-parse', 'HEAD'])).trim();
  }

  async wrapper(args) {
    return await run(process.execPath, [SCRIPT, ...args], { cwd: this.root, env: this.env });
  }

  /** Git plumbing whose payload arrives on stdin (index stages). */
  async gitInput(args, input) {
    const result = await run('git', args, { cwd: this.superproject, env: this.env, input });
    assert.equal(result.ok, true, `git ${args.join(' ')} failed: ${firstLine(result.stderr)}`);
    return result.stdout;
  }

  async commit(dir, file, content, message) {
    await writeFile(join(dir, file), content);
    await this.gitOk(['add', file], { cwd: dir });
    await this.gitOk(['commit', '-q', '-m', message], { cwd: dir });
  }

  async build() {
    await writeFile(this.shim, SSH_SHIM, { mode: 0o755 });
    await mkdir(this.worktreesRoot, { recursive: true });

    await this.gitOk(['init', '-q', '--initial-branch=main', this.nestedOrigin], { cwd: this.root });
    await this.commit(this.nestedOrigin, 'nested.txt', 'nested one\n', 'nested one');
    this.nestedOlder = await this.head(this.nestedOrigin);
    await this.commit(this.nestedOrigin, 'nested.txt', 'nested two\n', 'nested two');
    this.nestedPinned = await this.head(this.nestedOrigin);

    await this.gitOk(['init', '-q', '--initial-branch=main', this.subOrigin], { cwd: this.root });
    // Both commits keep the nested submodule registered, so checking out the
    // older commit leaves a clean working tree (only its content differs).
    await writeFile(join(this.subOrigin, 'sub.txt'), 'sub one\n');
    await writeFile(join(this.subOrigin, '.gitmodules'), this.modulesEntry('nested', 'nested', this.nestedOrigin));
    await this.gitOk(['update-index', '--add', '--cacheinfo', `${GITLINK_MODE},${this.nestedPinned},nested`], { cwd: this.subOrigin });
    await this.gitOk(['add', '.gitmodules', 'sub.txt'], { cwd: this.subOrigin });
    await this.gitOk(['commit', '-q', '-m', 'sub one with nested submodule'], { cwd: this.subOrigin });
    this.subOlder = await this.head(this.subOrigin);
    await writeFile(join(this.subOrigin, 'sub.txt'), 'sub two\n');
    await this.gitOk(['add', 'sub.txt'], { cwd: this.subOrigin });
    await this.gitOk(['commit', '-q', '-m', 'sub two'], { cwd: this.subOrigin });
    this.subPinned = await this.head(this.subOrigin);

    await this.gitOk(['init', '-q', '--initial-branch=main', this.superproject], { cwd: this.root });
    await this.commit(this.superproject, '.gitignore', '.worktrees/\n', 'ignore linked worktrees');
    await writeFile(join(this.superproject, '.gitmodules'), this.modulesEntry('sub', 'sub', this.subOrigin));
    await this.gitOk(['update-index', '--add', '--cacheinfo', `${GITLINK_MODE},${this.subPinned},sub`]);
    await this.gitOk(['add', '.gitmodules']);
    await this.gitOk(['commit', '-q', '-m', 'register submodule']);
    // Main keeps its own native metadata: the fixture needs one real main
    // pointer to reproduce the copied-pointer failure and to compare checkouts.
    await this.gitOk(['submodule', 'update', '--init', '--recursive']);
    await this.gitOk(['worktree', 'add', '-q', this.worktree(FEATURE_A), '-b', FEATURE_A]);
    await this.gitOk(['worktree', 'add', '-q', this.worktree(FEATURE_B), '-b', FEATURE_B]);
  }

  /** Linked worktree outside `<main>/.worktrees/`, created by the location test. */
  async addNonCanonicalWorktree() {
    const path = join(this.root, 'linked-elsewhere');
    const branch = 'fixture-elsewhere';
    await this.gitOk(['worktree', 'add', '-q', path, '-b', branch]);
    this.extraWorktrees.push({ path, name: basename(path), branch });
    return { path, branch };
  }

  async teardown() {
    const failures = [];
    const features = [FEATURE_A, FEATURE_B].map(name => ({ path: this.worktree(name), name, branch: name }));
    const owned = [...features, ...this.extraWorktrees];

    for (const worktree of owned) {
      if (!existsSync(worktree.path)) {
        continue;
      }
      // Restore fixture state first (recorded submodule pins, and the file edits
      // this file's tests create) so an earlier assertion failure cannot leave
      // the fixture unrestorable. Failures are reported, never swallowed.
      const restorePins = await this.git(['-C', worktree.path, 'submodule', 'update', '--init', '--recursive']);
      if (!restorePins.ok) {
        failures.push(`restore submodule update failed in ${worktree.path}: ${firstLine(restorePins.stderr)}`);
      }
      const restoreFiles = await this.git(['-C', worktree.path, 'submodule', 'foreach', '--recursive', 'git checkout -- . && git clean -qfd']);
      if (!restoreFiles.ok) {
        failures.push(`restore submodule foreach failed in ${worktree.path}: ${firstLine(restoreFiles.stderr)}`);
      }
      const status = await this.git(['-C', worktree.path, 'status', '--porcelain', '--ignore-submodules=all']);
      if (!status.ok) {
        failures.push(`status failed in ${worktree.path}: ${firstLine(status.stderr)}`);
        continue;
      }
      if (status.stdout.trim() !== '') {
        failures.push(`${worktree.path} still has local changes (${firstLine(status.stdout.trim())}); left in place`);
        continue;
      }
      const deinit = await this.git(['-C', worktree.path, 'submodule', 'deinit', '--all']);
      if (!deinit.ok) {
        failures.push(`submodule deinit --all failed in ${worktree.path}: ${firstLine(deinit.stderr)}`);
      }
      // Git 2.54 refuses non-force `worktree remove` for any worktree whose
      // tree contains submodules ("working trees containing submodules cannot
      // be moved or removed") even after deinit. The attempt is kept as
      // evidence, and the PM-authorized fallback for this disposable fixture is
      // to delete the fixture-owned worktree directory and prune its record.
      const removed = await this.git(['worktree', 'remove', worktree.path]);
      record(`teardown-worktree-remove-${worktree.name}`, `git -C <fixture-main> worktree remove <${worktree.name}>`, removed);
      if (!removed.ok) {
        if (!/containing submodules/.test(removed.stderr)) {
          failures.push(`worktree remove ${worktree.path} failed: ${firstLine(removed.stderr)}`);
          continue;
        }
        await rm(worktree.path, { recursive: true, force: true });
      }
      const pruned = await this.git(['worktree', 'prune']);
      if (!pruned.ok) {
        failures.push(`worktree prune failed: ${firstLine(pruned.stderr)}`);
      }
      if (existsSync(worktree.path)) {
        failures.push(`${worktree.path} still exists after non-force removal`);
      }
      if (existsSync(this.worktreeGitDir(worktree.name))) {
        failures.push(`${this.worktreeGitDir(worktree.name)} still exists after prune`);
      }
      const deleted = await this.git(['branch', '-d', worktree.branch]);
      if (!deleted.ok) {
        failures.push(`branch -d ${worktree.branch} failed: ${firstLine(deleted.stderr)}`);
      }
    }

    const listed = await this.git(['worktree', 'list', '--porcelain']);
    if (!listed.ok) {
      failures.push(`worktree list failed: ${firstLine(listed.stderr)}`);
    } else {
      const remaining = listed.stdout.split('\n').filter(line => line.startsWith('worktree ')).map(line => line.slice(9).trim());
      const unexpected = remaining.filter(path => path !== this.superproject);
      if (unexpected.length > 0) {
        failures.push(`worktrees still registered: ${unexpected.join(', ')}`);
      }
    }

    if (failures.length > 0) {
      throw new Error(`fixture teardown failed (${this.root} retained for inspection):\n- ${failures.join('\n- ')}`);
    }
    await rm(this.root, { recursive: true, force: true });
    assert.equal(existsSync(this.root), false, `fixture root ${this.root} was not removed`);
  }
}

async function checkoutFacts(fixture, checkout) {
  const sub = join(checkout, 'sub');
  const nested = join(sub, 'nested');
  const facts = {
    commonDir: await canonical(checkout, await fixture.gitOk(['-C', checkout, 'rev-parse', '--git-common-dir'])),
    subGitDir: await canonical(sub, await fixture.gitOk(['-C', sub, 'rev-parse', '--absolute-git-dir'])),
    nestedGitDir: await canonical(nested, await fixture.gitOk(['-C', nested, 'rev-parse', '--absolute-git-dir'])),
    subToplevel: await canonical(sub, await fixture.gitOk(['-C', sub, 'rev-parse', '--show-toplevel'])),
    nestedToplevel: await canonical(nested, await fixture.gitOk(['-C', nested, 'rev-parse', '--show-toplevel'])),
    subIndex: await canonical(sub, await fixture.gitOk(['-C', sub, 'rev-parse', '--git-path', 'index'])),
    nestedIndex: await canonical(nested, await fixture.gitOk(['-C', nested, 'rev-parse', '--git-path', 'index'])),
    subHead: await fixture.head(sub),
    nestedHead: await fixture.head(nested),
    subGitlink: (await fixture.gitOk(['-C', checkout, 'ls-files', '-s', '--', 'sub'])).split('\t')[0].split(' ')[1],
    subModulesUrl: (await fixture.gitOk(['-C', checkout, 'config', '--file', '.gitmodules', '--get', 'submodule.sub.url'])).trim(),
    subOriginUrl: (await fixture.gitOk(['-C', sub, 'config', '--get', 'remote.origin.url'])).trim(),
  };
  return facts;
}

let fixture;

before(async () => {
  fixture = await WorktreeFixture.create();
});

after(async () => {
  await fixture.teardown();
});

test('fresh linked worktrees use independent native metadata', async () => {
  const worktreeA = fixture.worktree(FEATURE_A);
  const worktreeB = fixture.worktree(FEATURE_B);

  // Red: reproduce the historical topology — a main-checkout pointer copied into
  // a linked checkout. Raw Git must fail there before any native initialization.
  const mainPointer = await readFile(join(fixture.superproject, 'sub', '.git'), 'utf8');
  assert.match(mainPointer, /^gitdir: /, 'main submodule must expose a native gitdir pointer');
  await mkdir(join(worktreeA, 'sub'), { recursive: true });
  await writeFile(join(worktreeA, 'sub', '.git'), mainPointer);
  const traversal = await fixture.git(['-C', join(worktreeA, 'sub'), 'rev-parse', '--show-toplevel']);
  record('red-copied-pointer', `git -C <A>/sub rev-parse --show-toplevel`, traversal);
  assert.equal(traversal.ok, false, 'the copied main pointer must not resolve in a linked checkout');
  assert.match(traversal.stderr, /not a git repository|invalid gitfile format|no such file/i);

  const refused = await fixture.wrapper(['--worktree', worktreeA]);
  record('red-refusal', `node scripts/init-worktree-submodules.mjs --worktree <A>`, refused);
  assert.equal(refused.code, 1, `initializer must refuse a copied pointer: ${refused.stdout}${refused.stderr}`);
  assert.equal(JSON.parse(refused.stdout).ok, false);
  assert.match(refused.stderr, /refused/);
  assert.match(refused.stderr, /sub|administrative directory|per-worktree subtree/, 'the refusal must name the submodule metadata problem');
  assert.equal(await readFile(join(worktreeA, 'sub', '.git'), 'utf8'), mainPointer, 'refusal must leave the copied pointer untouched');
  assert.equal(existsSync(join(fixture.worktreeGitDir(FEATURE_A), 'modules')), false, 'refusal must not create metadata');

  // Restore only the exact fixture-created bytes, then initialize natively.
  await rm(join(worktreeA, 'sub'), { recursive: true, force: true });

  const freshA = await fixture.wrapper(['--worktree', worktreeA]);
  assert.equal(freshA.code, 0, freshA.stderr);
  const freshAJson = JSON.parse(freshA.stdout);
  assert.deepEqual(Object.keys(freshAJson), ['version', 'worktree', 'submodules', 'ok']);
  assert.equal(freshAJson.version, 1);
  assert.equal(freshAJson.worktree, worktreeA);
  assert.equal(freshAJson.ok, true);
  assert.deepEqual(freshAJson.submodules.map(entry => entry.path), ['sub', 'sub/nested']);
  assert.deepEqual(freshAJson.submodules.map(entry => entry.action), ['initialized', 'initialized']);
  assert.deepEqual(freshAJson.submodules.map(entry => entry.head), [fixture.subPinned, fixture.nestedPinned]);
  assert.deepEqual(freshAJson.submodules.map(entry => entry.gitlink), [fixture.subPinned, fixture.nestedPinned]);

  const freshB = await fixture.wrapper(['--worktree', worktreeB]);
  assert.equal(freshB.code, 0, freshB.stderr);
  assert.deepEqual(JSON.parse(freshB.stdout).submodules.map(entry => entry.action), ['initialized', 'initialized']);

  // Every checkout: its own gitdir, its own index, its own working tree.
  const checkouts = { main: fixture.superproject, a: worktreeA, b: worktreeB };
  const facts = {};
  for (const [label, checkout] of Object.entries(checkouts)) {
    facts[label] = await checkoutFacts(fixture, checkout);
    assert.equal(facts[label].commonDir, await canonical(checkout, join(fixture.superproject, '.git')));
    assert.equal(facts[label].subToplevel, await realpath(join(checkout, 'sub')), `${label} submodule must resolve its own working tree`);
    assert.equal(facts[label].nestedToplevel, await realpath(join(checkout, 'sub', 'nested')));
    assert.equal(facts[label].subHead, fixture.subPinned);
    assert.equal(facts[label].nestedHead, fixture.nestedPinned);
    assert.equal(facts[label].subGitlink, fixture.subPinned);
    assert.equal(facts[label].subOriginUrl, facts[label].subModulesUrl);
    assert.equal(facts[label].subOriginUrl, fixture.originUrl(fixture.subOrigin));

    const status = await fixture.git(['-C', checkout, 'status', '--porcelain', '--ignored=matching']);
    record(`${label}-status`, `git -C ${checkout} status --porcelain --ignored=matching`, status);
    assert.equal(status.ok, true, `${label} status must work: ${firstLine(status.stderr)}`);
    // The main fixture checkout legitimately reports its own ignored
    // `.worktrees/` directory; a linked checkout must report nothing at all.
    const expected = label === 'main' ? ['!! .worktrees/'] : [];
    assert.deepEqual(status.stdout.split('\n').filter(line => line.trim() !== ''), expected);

    const recursive = await fixture.git(['-C', checkout, 'submodule', 'status', '--recursive']);
    record(`${label}-recursive-submodule-status`, `git -C ${checkout} submodule status --recursive`, recursive);
    assert.equal(recursive.ok, true, `${label} recursive submodule status must work: ${firstLine(recursive.stderr)}`);
    const lines = recursive.stdout.split('\n').filter(line => line.trim() !== '');
    assert.deepEqual(lines.map(line => line.slice(1).split(' ')[0]), [fixture.subPinned, fixture.nestedPinned]);
    assert.deepEqual(lines.map(line => line[0]), [' ', ' ']);

    // Content bytes, not path appearance.
    assert.equal(await readFile(join(checkout, 'sub', 'sub.txt'), 'utf8'), 'sub two\n');
    assert.equal(await readFile(join(checkout, 'sub', 'nested', 'nested.txt'), 'utf8'), 'nested two\n');
  }

  record('git-common-dir', `git -C <A> rev-parse --git-common-dir`, await fixture.git(['-C', worktreeA, 'rev-parse', '--git-common-dir']));
  record('sub-absolute-git-dir', `git -C <A>/sub rev-parse --absolute-git-dir`, await fixture.git(['-C', join(worktreeA, 'sub'), 'rev-parse', '--absolute-git-dir']));
  console.log(`EVIDENCE git-version: ${(await fixture.gitOk(['--version'])).trim()}`);

  const subGitDirs = [facts.main.subGitDir, facts.a.subGitDir, facts.b.subGitDir];
  const subIndexes = [facts.main.subIndex, facts.a.subIndex, facts.b.subIndex];
  const nestedGitDirs = [facts.main.nestedGitDir, facts.a.nestedGitDir, facts.b.nestedGitDir];
  assert.equal(new Set(subGitDirs).size, 3, `submodule gitdirs must stay private: ${subGitDirs.join(', ')}`);
  assert.equal(new Set(subIndexes).size, 3, `submodule indexes must stay private: ${subIndexes.join(', ')}`);
  assert.equal(new Set(nestedGitDirs).size, 3, `nested gitdirs must stay private: ${nestedGitDirs.join(', ')}`);
  assert.equal(facts.main.subGitDir, join(await realpath(fixture.superproject), '.git', 'modules', 'sub'));
  assert.equal(facts.a.subGitDir, join(fixture.worktreeGitDir(FEATURE_A), 'modules', 'sub'));
  assert.equal(facts.b.subGitDir, join(fixture.worktreeGitDir(FEATURE_B), 'modules', 'sub'));
  assert.equal(facts.a.nestedGitDir, join(fixture.worktreeGitDir(FEATURE_A), 'modules', 'sub', 'modules', 'nested'));
  console.log(`EVIDENCE distinct-gitdirs: main ${facts.main.subGitDir} | A ${facts.a.subGitDir} | B ${facts.b.subGitDir}`);
  console.log(`EVIDENCE distinct-indexes: main ${facts.main.subIndex} | A ${facts.a.subIndex} | B ${facts.b.subIndex}`);
});

test('repeat preserves pin and rejects dirty or copied metadata', async () => {
  const worktreeA = fixture.worktree(FEATURE_A);
  const worktreeB = fixture.worktree(FEATURE_B);

  const first = await fixture.wrapper(['--worktree', worktreeA]);
  assert.equal(first.code, 0, first.stderr);
  assert.deepEqual(JSON.parse(first.stdout).submodules.map(entry => entry.action), ['validated', 'validated']);
  const subIndexA = JSON.parse(first.stdout).submodules[0].git_dir + '/index';
  const indexDigest = await digest(subIndexA);

  const stable = await fixture.wrapper(['--worktree', worktreeA]);
  assert.equal(stable.code, 0, stable.stderr);
  assert.equal(stable.stdout, first.stdout, 'a validated repeat must produce identical JSON');
  assert.equal(await digest(subIndexA), indexDigest, 'a validated repeat must not rewrite the submodule index');

  // An intentionally different but clean submodule HEAD is reported, never reset.
  await fixture.gitOk(['-C', join(worktreeA, 'sub'), 'checkout', '-q', fixture.subOlder]);
  const bBefore = await checkoutFacts(fixture, worktreeB);
  const mainBefore = await checkoutFacts(fixture, fixture.superproject);
  const pinned = await fixture.wrapper(['--worktree', worktreeA]);
  assert.equal(pinned.code, 0, pinned.stderr);
  const pinnedJson = JSON.parse(pinned.stdout);
  assert.deepEqual(pinnedJson.submodules.map(entry => entry.action), ['validated', 'validated']);
  assert.equal(pinnedJson.submodules[0].head, fixture.subOlder);
  assert.equal(pinnedJson.submodules[0].gitlink, fixture.subPinned);
  assert.match(pinned.stderr, /differs from the recorded gitlink/);
  assert.equal(await fixture.head(join(worktreeA, 'sub')), fixture.subOlder, 'the initializer must preserve a different clean HEAD');
  assert.equal(await fixture.head(join(worktreeB, 'sub')), fixture.subPinned);
  assert.equal(await fixture.head(join(fixture.superproject, 'sub')), fixture.subPinned);
  assert.deepEqual(await checkoutFacts(fixture, worktreeB), bBefore, 'B must stay untouched');
  assert.deepEqual(await checkoutFacts(fixture, fixture.superproject), mainBefore, 'main must stay untouched');

  // Dirt is refused with no mutation.
  await writeFile(join(worktreeA, 'sub', 'dirt-untracked.txt'), 'untracked\n');
  await writeFile(join(worktreeA, 'sub', 'sub.txt'), 'modified by the test\n');
  const indexDigestAtRefusal = await digest(subIndexA);
  const dirty = await fixture.wrapper(['--worktree', worktreeA]);
  assert.equal(dirty.code, 1);
  assert.equal(JSON.parse(dirty.stdout).ok, false);
  assert.match(dirty.stderr, /local changes/);
  assert.equal(existsSync(join(worktreeA, 'sub', 'dirt-untracked.txt')), true, 'refusal must leave the dirt in place');
  assert.equal(await readFile(join(worktreeA, 'sub', 'sub.txt'), 'utf8'), 'modified by the test\n');
  assert.equal(await fixture.head(join(worktreeA, 'sub')), fixture.subOlder, 'refusal must not move HEAD');
  assert.equal(await digest(subIndexA), indexDigestAtRefusal, 'refusal must not rewrite the index');

  // Restore only the dirt this test created.
  await rm(join(worktreeA, 'sub', 'dirt-untracked.txt'), { force: true });
  await fixture.gitOk(['-C', join(worktreeA, 'sub'), 'checkout', '--', 'sub.txt']);

  // A copied main pointer in a linked checkout is refused and left untouched.
  const mainPointer = await readFile(join(fixture.superproject, 'sub', '.git'), 'utf8');
  await fixture.gitOk(['-C', worktreeB, 'submodule', 'deinit', '--all']);
  await rm(join(worktreeB, 'sub'), { recursive: true, force: true });
  await mkdir(join(worktreeB, 'sub'), { recursive: true });
  await writeFile(join(worktreeB, 'sub', '.git'), mainPointer);
  const copied = await fixture.wrapper(['--worktree', worktreeB]);
  assert.equal(copied.code, 1);
  assert.equal(JSON.parse(copied.stdout).ok, false);
  assert.match(copied.stderr, /refused/);
  assert.equal(await readFile(join(worktreeB, 'sub', '.git'), 'utf8'), mainPointer);

  await rm(join(worktreeB, 'sub'), { recursive: true, force: true });
  const reinitialized = await fixture.wrapper(['--worktree', worktreeB]);
  assert.equal(reinitialized.code, 0, reinitialized.stderr);
  assert.deepEqual(JSON.parse(reinitialized.stdout).submodules.map(entry => entry.action), ['initialized', 'initialized']);
  assert.equal(await fixture.head(join(worktreeB, 'sub')), fixture.subPinned);

  // Leave A on the recorded pin so the fixture worktree stays removable.
  await fixture.gitOk(['-C', worktreeA, 'submodule', 'update', '--init', 'sub']);
  assert.equal(await fixture.head(join(worktreeA, 'sub')), fixture.subPinned);
});

test('nested initialization never resets an initialized child', async () => {
  const worktreeA = fixture.worktree(FEATURE_A);
  const worktreeB = fixture.worktree(FEATURE_B);
  const nestedAdmin = join(fixture.worktreeGitDir(FEATURE_A), 'modules', 'sub', 'modules', 'nested');

  // Both the parent and the nested child deliberately sit on another clean commit.
  await fixture.gitOk(['-C', join(worktreeA, 'sub'), 'checkout', '-q', fixture.subOlder]);
  await fixture.gitOk(['-C', join(worktreeA, 'sub', 'nested'), 'checkout', '-q', fixture.nestedOlder]);
  const indexDigest = await digest(join(fixture.worktreeGitDir(FEATURE_A), 'modules', 'sub', 'index'));

  const noop = await fixture.wrapper(['--worktree', worktreeA]);
  assert.equal(noop.code, 0, noop.stderr);
  assert.deepEqual(JSON.parse(noop.stdout).submodules.map(entry => entry.action), ['validated', 'validated']);
  assert.deepEqual(JSON.parse(noop.stdout).submodules.map(entry => entry.head), [fixture.subOlder, fixture.nestedOlder]);
  assert.equal(await fixture.head(join(worktreeA, 'sub')), fixture.subOlder);
  assert.equal(await fixture.head(join(worktreeA, 'sub', 'nested')), fixture.nestedOlder);

  // Drop only the nested child's working tree and per-worktree metadata: the
  // parent stays initialized, so a recursive reset would be wrong.
  await rm(join(worktreeA, 'sub', 'nested'), { recursive: true, force: true });
  await rm(nestedAdmin, { recursive: true, force: true });
  const bBefore = await checkoutFacts(fixture, worktreeB);
  const mainBefore = await checkoutFacts(fixture, fixture.superproject);
  record('missing-nested-recv', `git -C <A>/sub submodule status --recursive`, await fixture.git(['-C', join(worktreeA, 'sub'), 'submodule', 'status', '--recursive']));

  const repaired = await fixture.wrapper(['--worktree', worktreeA]);
  assert.equal(repaired.code, 0, repaired.stderr);
  const repairedJson = JSON.parse(repaired.stdout);
  const parent = repairedJson.submodules.find(entry => entry.path === 'sub');
  const child = repairedJson.submodules.find(entry => entry.path === 'sub/nested');
  assert.equal(parent.action, 'validated');
  assert.equal(parent.head, fixture.subOlder, 'initializing a missing child must not move the initialized parent');
  assert.equal(child.action, 'initialized');
  assert.equal(child.head, fixture.nestedPinned);
  assert.equal(child.git_dir, nestedAdmin);
  assert.equal(await fixture.head(join(worktreeA, 'sub')), fixture.subOlder);
  assert.equal(await fixture.head(join(worktreeA, 'sub', 'nested')), fixture.nestedPinned);
  assert.equal(await canonical(join(worktreeA, 'sub'), await fixture.gitOk(['-C', join(worktreeA, 'sub', 'nested'), 'rev-parse', '--show-toplevel'])), await realpath(join(worktreeA, 'sub', 'nested')));
  assert.equal(await digest(join(fixture.worktreeGitDir(FEATURE_A), 'modules', 'sub', 'index')), indexDigest, 'the initialized parent index must not be rewritten');
  assert.deepEqual(await checkoutFacts(fixture, worktreeB), bBefore, 'B must stay untouched');
  assert.deepEqual(await checkoutFacts(fixture, fixture.superproject), mainBefore, 'main must stay untouched');

  // Leave A on the recorded pins so the fixture worktree stays removable.
  await fixture.gitOk(['-C', worktreeA, 'submodule', 'update', '--init', '--recursive']);
  assert.equal(await fixture.head(join(worktreeA, 'sub')), fixture.subPinned);
  assert.equal(await fixture.head(join(worktreeA, 'sub', 'nested')), fixture.nestedPinned);
});

test('fail-closed invocation and checkout location checks', async () => {
  // --help must print usage and exit 0 without probing Git at all.
  const probeBin = join(fixture.root, 'probe-bin');
  await mkdir(probeBin, { recursive: true });
  await writeFile(join(probeBin, 'git'), '#!/bin/sh\necho "git must not run for --help" >&2\nexit 97\n', { mode: 0o755 });
  const help = await run(process.execPath, [SCRIPT, '--help'], {
    cwd: fixture.root,
    env: { ...fixture.env, PATH: `${probeBin}:${process.env.PATH}` },
  });
  assert.equal(help.code, 0, help.stderr);
  assert.match(help.stdout, /Usage: node scripts\/init-worktree-submodules\.mjs/);
  assert.doesNotMatch(help.stdout, /^\{/, 'help is the only non-JSON stdout mode');
  assert.equal(help.stderr, '');

  // `-h` is the same standalone mode: usage, exit 0, no probing.
  const shortHelp = await run(process.execPath, [SCRIPT, '-h'], {
    cwd: fixture.root,
    env: { ...fixture.env, PATH: `${probeBin}:${process.env.PATH}` },
  });
  assert.equal(shortHelp.code, 0, shortHelp.stderr);
  assert.match(shortHelp.stdout, /Usage: node scripts\/init-worktree-submodules\.mjs/);
  assert.equal(shortHelp.stderr, '');

  for (const args of [
    [],
    ['--worktree'],
    ['--worktree', 'relative/path'],
    ['--worktree', fixture.superproject, '--worktree', fixture.worktree(FEATURE_A)],
    ['--unknown'],
    // Help is a standalone mode: mixed with anything else it is not a valid
    // invocation, and the extra argument must never be silently ignored.
    ['--help', '--unknown'],
    ['--worktree', fixture.worktree(FEATURE_A), '--help'],
    ['--help', '--help'],
    ['--help', '-h'],
  ]) {
    const invalid = await fixture.wrapper(args);
    assert.equal(invalid.code, 2, `expected exit 2 for ${JSON.stringify(args)}: ${invalid.stdout}${invalid.stderr}`);
    assert.equal(invalid.stdout, '', 'invalid invocations must not write stdout');
  }

  // The main worktree is refused.
  const main = await fixture.wrapper(['--worktree', fixture.superproject]);
  assert.equal(main.code, 1);
  assert.match(main.stderr, /main worktree/);
  assert.equal(JSON.parse(main.stdout).ok, false);

  // A path that is not a worktree root of any repository is refused.
  const unrelated = join(fixture.root, 'unrelated');
  await mkdir(unrelated, { recursive: true });
  const notARepo = await fixture.wrapper(['--worktree', unrelated]);
  assert.equal(notARepo.code, 1);
  assert.equal(JSON.parse(notARepo.stdout).ok, false);

  // A linked worktree outside `<main>/.worktrees/` is refused.
  const elsewhere = await fixture.addNonCanonicalWorktree();
  const nonCanonical = await fixture.wrapper(['--worktree', elsewhere.path]);
  assert.equal(nonCanonical.code, 1);
  assert.match(nonCanonical.stderr, /must live directly under/);

  // A symlink escape is refused and left untouched.
  const escape = join(fixture.worktreesRoot, 'escape');
  await symlink(fixture.superproject, escape);
  const escaped = await fixture.wrapper(['--worktree', escape]);
  assert.equal(escaped.code, 1);
  assert.equal(JSON.parse(escaped.stdout).ok, false);
  assert.equal(await readlink(escape), fixture.superproject, 'the symlink must stay untouched');
  await rm(escape, { force: true });
});

test('an unresolved .gitmodules index conflict is refused without the worktree file', async () => {
  const worktreeA = fixture.worktree(FEATURE_A);
  const gitmodules = join(worktreeA, '.gitmodules');
  const index = join(fixture.worktreeGitDir(FEATURE_A), 'index');
  const gitmodulesBlob = (await fixture.gitOk(['-C', worktreeA, 'rev-parse', 'HEAD:.gitmodules'])).trim();
  const otherBlob = (await fixture.gitOk(['-C', worktreeA, 'rev-parse', 'HEAD:.gitignore'])).trim();

  const baseline = await fixture.wrapper(['--worktree', worktreeA]);
  assert.equal(baseline.code, 0, `the fixture must start from an initialized checkout: ${baseline.stdout}${baseline.stderr}`);

  // An index that still carries the unresolved `.gitmodules` conflict *and* the
  // unresolved submodule gitlink, with the working-tree `.gitmodules` gone. A
  // preflight gated on that file's existence skipped this state entirely and
  // reported an empty — therefore "successful" — submodule list.
  await rm(gitmodules, { force: true });
  await fixture.gitOk(['-C', worktreeA, 'update-index', '--force-remove', '--', '.gitmodules', 'sub']);
  await fixture.gitInput(['-C', worktreeA, 'update-index', '--index-info'], [
    `100644 ${gitmodulesBlob} 1\t.gitmodules`,
    `100644 ${otherBlob} 2\t.gitmodules`,
    `100644 ${gitmodulesBlob} 3\t.gitmodules`,
    `160000 ${fixture.subOlder} 1\tsub`,
    `160000 ${fixture.subPinned} 2\tsub`,
    `160000 ${fixture.subOlder} 3\tsub`,
  ].join('\n') + '\n');
  record('unmerged-index', `git -C <A> status --porcelain`, await fixture.git(['-C', worktreeA, 'status', '--porcelain']));
  record('unmerged-index-entries', `git -C <A> ls-files -u`, await fixture.git(['-C', worktreeA, 'ls-files', '-u']));
  const conflictedDigest = await digest(index);
  const conflicted = await fixture.wrapper(['--worktree', worktreeA]);
  record('unmerged-index-refusal', `node scripts/init-worktree-submodules.mjs --worktree <A>`, conflicted);
  const leftBehind = { gitmodules: existsSync(gitmodules), index: await digest(index) };
  // Restore before asserting so a failing run cannot strand the shared fixture.
  await fixture.gitOk(['-C', worktreeA, 'read-tree', 'HEAD']);
  await fixture.gitOk(['-C', worktreeA, 'checkout', 'HEAD', '--', '.gitmodules']);

  assert.equal(conflicted.code, 1, `an unresolved .gitmodules conflict must be refused: ${conflicted.stdout}${conflicted.stderr}`);
  const conflictedJson = JSON.parse(conflicted.stdout);
  assert.equal(conflictedJson.ok, false);
  assert.deepEqual(conflictedJson.submodules, []);
  assert.match(conflicted.stderr, /unmerged entry '\.gitmodules'/, 'the refusal must name the unmerged entry');
  assert.equal(leftBehind.gitmodules, false, 'the refusal must not materialize .gitmodules');
  assert.equal(leftBehind.index, conflictedDigest, 'the refusal must not rewrite the conflicted index');
});

test('an index gitlink that .gitmodules does not declare is refused', async () => {
  const worktreeA = fixture.worktree(FEATURE_A);
  const gitmodules = join(worktreeA, '.gitmodules');
  const index = join(fixture.worktreeGitDir(FEATURE_A), 'index');

  const baseline = await fixture.wrapper(['--worktree', worktreeA]);
  assert.equal(baseline.code, 0, `the fixture must start from an initialized checkout: ${baseline.stdout}${baseline.stderr}`);

  // The declaration file is gone while the index still commits the gitlink, so
  // nothing declares that submodule and its state cannot be established. An
  // empty ok list would report a checkout that was never checked.
  const digestBefore = await digest(index);
  await rm(gitmodules, { force: true });
  const undeclared = await fixture.wrapper(['--worktree', worktreeA]);
  record('undeclared-gitlink-absent-gitmodules', `node scripts/init-worktree-submodules.mjs --worktree <A>`, undeclared);
  const leftBehind = { gitmodules: existsSync(gitmodules), index: await digest(index) };
  await fixture.gitOk(['-C', worktreeA, 'checkout', 'HEAD', '--', '.gitmodules']);

  assert.equal(undeclared.code, 1, `a gitlink without a declaration must be refused: ${undeclared.stdout}${undeclared.stderr}`);
  const undeclaredJson = JSON.parse(undeclared.stdout);
  assert.equal(undeclaredJson.ok, false);
  assert.deepEqual(undeclaredJson.submodules, []);
  assert.match(undeclared.stderr, /records gitlink\(s\) that \.gitmodules does not declare: sub/);
  assert.equal(leftBehind.gitmodules, false, 'the refusal must not materialize .gitmodules');
  assert.equal(leftBehind.index, digestBefore, 'the refusal must not rewrite the index');
});

test('an unresolved gitlink index entry is refused and the resolved checkout validates', async () => {
  const worktreeA = fixture.worktree(FEATURE_A);
  const index = join(fixture.worktreeGitDir(FEATURE_A), 'index');

  const baseline = await fixture.wrapper(['--worktree', worktreeA]);
  assert.equal(baseline.code, 0, `the fixture must start from an initialized checkout: ${baseline.stdout}${baseline.stderr}`);

  // An unresolved *gitlink* while the declaration file is intact: the submodule
  // the superproject records is a conflict, not a state to initialize from.
  await fixture.gitOk(['-C', worktreeA, 'update-index', '--force-remove', '--', 'sub']);
  await fixture.gitInput(['-C', worktreeA, 'update-index', '--index-info'], [
    `160000 ${fixture.subOlder} 1\tsub`,
    `160000 ${fixture.subPinned} 2\tsub`,
    `160000 ${fixture.subOlder} 3\tsub`,
  ].join('\n') + '\n');
  const conflictedDigest = await digest(index);
  const conflicted = await fixture.wrapper(['--worktree', worktreeA]);
  record('unmerged-gitlink', `node scripts/init-worktree-submodules.mjs --worktree <A>`, conflicted);
  const leftBehind = await digest(index);
  await fixture.gitOk(['-C', worktreeA, 'read-tree', 'HEAD']);
  const recovered = await fixture.wrapper(['--worktree', worktreeA]);
  record('resolved-index', `node scripts/init-worktree-submodules.mjs --worktree <A>`, recovered);

  assert.equal(conflicted.code, 1, `an unresolved gitlink must be refused: ${conflicted.stdout}${conflicted.stderr}`);
  assert.equal(JSON.parse(conflicted.stdout).ok, false);
  assert.match(conflicted.stderr, /unmerged entry 'sub'/);
  assert.equal(leftBehind, conflictedDigest, 'the refusal must not rewrite the conflicted index');
  assert.equal(recovered.code, 0, recovered.stderr);
  assert.equal(JSON.parse(recovered.stdout).ok, true, 'a resolved checkout must validate again');
});

test('operational filesystem failures keep the JSON stdout contract', async () => {
  const worktreeA = fixture.worktree(FEATURE_A);
  const stranded = join(worktreeA, 'sub');

  // A not-initialized submodule path that exists but cannot be listed: an
  // ordinary filesystem failure rather than a Git or safety refusal. It must
  // still be reported through the same JSON object on stdout with exit 1.
  await fixture.gitOk(['-C', worktreeA, 'submodule', 'deinit', '--all']);
  await rm(stranded, { recursive: true, force: true });
  await mkdir(stranded, { recursive: true });
  await chmod(stranded, 0o000);
  const failed = await fixture.wrapper(['--worktree', worktreeA]);
  record('unreadable-submodule-path', `node scripts/init-worktree-submodules.mjs --worktree <A>`, failed);
  await chmod(stranded, 0o700);
  await rm(stranded, { recursive: true, force: true });

  assert.equal(failed.code, 1, `an operational failure must exit 1: ${failed.stdout}${failed.stderr}`);
  const json = JSON.parse(failed.stdout);
  assert.deepEqual(Object.keys(json), ['version', 'worktree', 'submodules', 'ok', 'refusal']);
  assert.equal(json.worktree, worktreeA);
  assert.equal(json.ok, false);
  assert.deepEqual(json.submodules, []);
  // The refusal names its own class: this one is a preflight refusal (the read failed before any
  // mutation was attempted), so nothing was changed and nothing is enumerated.
  assert.equal(json.refusal.code, 'init.refuse.preflight');
  assert.equal(json.refusal.initialized_paths, null);
  assert.match(json.refusal.detail, /cannot be read/);
  assert.match(failed.stderr, /refused/);
  assert.match(failed.stderr, /init\.refuse\.preflight: no mutation was attempted/);
  assert.ok(failed.stderr.includes(stranded), `the refusal must name the unreadable path: ${failed.stderr}`);
});

test('a partial native initialization is reported as partial, never as untouched', async t => {
  // F-003: the initializer promised "state unchanged" for every exit-1 failure, but native
  // initialization is not atomic — `git submodule update --init --recursive` runs once per repository
  // group and a later failure leaves the earlier work in place. Rolling Git initialization back is
  // riskier than telling the truth, so the refusal now distinguishes a PREFLIGHT refusal (this run
  // attempted no mutation) from a PARTIAL initialization, and names the paths the failure left
  // initialized instead of claiming an unchanged state.
  const worktreeA = fixture.worktree(FEATURE_A);
  const subModuleDir = join(fixture.worktreeGitDir(FEATURE_A), 'modules', 'sub');

  // Control — a preflight refusal: the main worktree is rejected by validation, which reads before
  // it ever mutates anything.
  const preflight = await fixture.wrapper(['--worktree', fixture.superproject]);
  assert.equal(preflight.code, 1, preflight.stdout);
  const preflightJson = JSON.parse(preflight.stdout);
  assert.equal(preflightJson.refusal.code, 'init.refuse.preflight');
  assert.equal(preflightJson.refusal.initialized_paths, null);
  assert.deepEqual(preflightJson.submodules, []);
  assert.match(preflight.stderr, /init\.refuse\.preflight: no mutation was attempted/);
  assert.doesNotMatch(preflight.stderr, /NOT rolled back/);

  const parkedSub = join(fixture.root, 'sub-origin-parked');
  const parkedNested = join(fixture.root, 'nested-origin-parked');
  try {
    // Case A — the parent clones, its nested child cannot: the failure leaves the parent initialized.
    // The per-worktree submodule metadata is dropped with the working tree so the native command must
    // really reach both origins instead of reusing an existing module directory.
    await fixture.gitOk(['-C', worktreeA, 'submodule', 'deinit', '--all']);
    await rm(join(worktreeA, 'sub'), { recursive: true, force: true });
    await rm(subModuleDir, { recursive: true, force: true });
    await rename(fixture.nestedOrigin, parkedNested);
    const partial = await fixture.wrapper(['--worktree', worktreeA]);
    record('partial-init-nested-unreachable', `node scripts/init-worktree-submodules.mjs --worktree <A>`, partial);
    assert.equal(partial.code, 1, partial.stdout);
    const partialJson = JSON.parse(partial.stdout);
    assert.equal(partialJson.ok, false);
    assert.deepEqual(partialJson.submodules, []);
    assert.equal(partialJson.refusal.code, 'init.refuse.partial');
    // Exactly which paths the failure left initialized — re-observed, not inferred.
    assert.deepEqual(partialJson.refusal.initialized_paths, ['sub']);
    assert.match(partial.stderr, /init\.refuse\.partial: at least one submodule was initialized by this run and is NOT rolled back/);
    assert.match(partial.stderr, /already initialized: sub/);
    assert.equal(existsSync(join(worktreeA, 'sub', '.git')), true, 'the report is the truth: the child really is initialized');
    assert.equal(existsSync(join(worktreeA, 'sub', 'nested', '.git')), false, 'the nested child is not initialized');

    // Case B — the native command completed nothing at all: still a partial (mutating) refusal, and
    // the empty enumeration says so instead of pretending an untouched checkout.
    await rm(join(worktreeA, 'sub'), { recursive: true, force: true });
    await rm(subModuleDir, { recursive: true, force: true });
    await rename(fixture.subOrigin, parkedSub);
    const nothing = await fixture.wrapper(['--worktree', worktreeA]);
    record('partial-init-parent-unreachable', `node scripts/init-worktree-submodules.mjs --worktree <A>`, nothing);
    assert.equal(nothing.code, 1, nothing.stdout);
    const nothingJson = JSON.parse(nothing.stdout);
    assert.equal(nothingJson.refusal.code, 'init.refuse.partial');
    assert.deepEqual(nothingJson.refusal.initialized_paths, []);
    assert.match(nothing.stderr, /no submodule path is initialized there yet/);
    assert.equal(existsSync(join(worktreeA, 'sub', '.git')), false);
  } finally {
    if (existsSync(parkedSub)) await rename(parkedSub, fixture.subOrigin);
    if (existsSync(parkedNested)) await rename(parkedNested, fixture.nestedOrigin);
  }

  // The partial state is not a trap: a re-run against the restored origins validates the initialized
  // subset and finishes the rest, which is the manual-safe continuation the refusal points at.
  const finished = await fixture.wrapper(['--worktree', worktreeA]);
  assert.equal(finished.code, 0, `${finished.stdout}${finished.stderr}`);
  assert.deepEqual(JSON.parse(finished.stdout).submodules.map(entry => entry.action), ['initialized', 'initialized']);
  assert.equal(await fixture.head(join(worktreeA, 'sub')), fixture.subPinned);
  assert.equal(await fixture.head(join(worktreeA, 'sub', 'nested')), fixture.nestedPinned);
});

