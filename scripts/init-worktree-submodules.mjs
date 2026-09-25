#!/usr/bin/env node
/**
 * G1 — initialize and validate linked-worktree submodule metadata with native Git.
 *
 * Native Git owns submodule metadata: every checkout gets its own administrative
 * directory (`.git/worktrees/<name>/modules/<submodule>`), its own index and its
 * own HEAD. This tool only asks Git to establish that metadata where it is
 * missing and validates what is already there. It never transplants a main
 * checkout's pointer, never repairs existing bad metadata, never forces and
 * never shares one index between checkouts.
 *
 * Initialization is native and therefore NOT transactional: when a native command
 * fails after an earlier one succeeded, the initialized subset is reported (refusal
 * code `init.refuse.partial`, naming the paths the failure left in place) and is
 * never rolled back. Every refusal that names no initialized submodule — a refusal
 * raised before any mutation, and an attempted command that completed none — is
 * `init.refuse.preflight`, which promises that nothing is initialized because
 * nothing was changed.
 *
 *   node scripts/init-worktree-submodules.mjs --worktree <absolute-checkout>
 *   node scripts/init-worktree-submodules.mjs --help
 */

import { execFile } from 'node:child_process';
import { existsSync } from 'node:fs';
import { lstat, readdir, realpath } from 'node:fs/promises';
import { basename, dirname, isAbsolute, join, relative, resolve, sep } from 'node:path';
import { promisify } from 'node:util';

const execFileAsync = promisify(execFile);

const VERSION = 1;
const GITLINK_MODE = '160000';
const WORKTREE_NAME = /^[A-Za-z0-9][A-Za-z0-9._-]*$/;

const USAGE = `Usage: node scripts/init-worktree-submodules.mjs --worktree <absolute-checkout>
       node scripts/init-worktree-submodules.mjs --help

Initialize the missing submodules of one linked worktree with native Git
(\`git submodule update --init --recursive\`) and validate the submodules that are
already initialized. The checkout must be a linked worktree registered in its
repository and located at <main-checkout>/.worktrees/<name>.

Options:
  --worktree <absolute-checkout>  Linked worktree to initialize (absolute path).
  --help, -h                      Print this text and exit 0. This is the only
                                  human-readable stdout mode; it is accepted only
                                  as the sole argument and it probes nothing.

stdout (operational invocations) is exactly one JSON object:
  {"version":1,"worktree":"<absolute>","submodules":[{"path","git_dir","head",
   "gitlink","action"}],"ok":true}
  "action" is "initialized" (native init created it in this run) or "validated"
  (already initialized). A submodule HEAD that differs from the recorded gitlink
  is reported on stderr and preserved, never reset. Nothing but the JSON object
  is written to stdout. A refusal writes the same object with "ok":false, an empty
  "submodules" list and a "refusal" object {"code","detail","initialized_paths"}
  (the reason is also on stderr), so stdout stays parseable. That contract holds
  for every operational failure, including filesystem errors that are not Git
  refusals, and it tells the truth about what the failure left behind:

    "init.refuse.preflight"  this report names no initialized submodule: the run
                             attempted no mutation at all, or its native command
                             completed none, so the checkout is exactly as it was.
                             Nothing is rolled back.
    "init.refuse.partial"    native Git had already initialized at least one
                             submodule when the failure happened, and
                             "initialized_paths" lists those paths (at least one).
                             Initialization is NOT transactional and nothing is
                             rolled back: re-run to validate the initialized
                             subset and finish the rest.
    "initialized_paths"      the submodule paths that are initialized in the
                             checkout at the moment of the failure, re-observed
                             read-only — which paths the failure therefore left in
                             place. A non-empty list (>= 1) always accompanies
                             "init.refuse.partial"; [] means an attempted native
                             command completed no submodule; null means nothing is
                             enumerated, either because the refusal was raised
                             before any mutation or because the checkout could not
                             be re-read (an unknown state, never an empty one).

Exit codes:
  0  checkout valid, submodules validated or initialized
  1  Git, safety or operational failure; the reason is on stderr, and the
     "refusal" object above says whether the state is untouched (preflight) or
     partially initialized (and which paths)
  2  invalid invocation (usage on stderr, no stdout)
`;

class UsageError extends Error {}
class RefusalError extends Error {}

function firstLine(text) {
  for (const line of String(text ?? '').split('\n')) {
    if (line.trim() !== '') return line.trim();
  }
  return '';
}

function isInside(parent, child) {
  const rel = relative(parent, child);
  return rel !== '' && !rel.startsWith('..') && !isAbsolute(rel);
}

async function canonicalize(target) {
  try {
    return await realpath(target);
  } catch {
    return null;
  }
}

async function runGit(cwd, args) {
  try {
    const { stdout } = await execFileAsync('git', args, {
      cwd,
      encoding: 'utf8',
      // No optional index refreshes (validating must not rewrite an index) and
      // no interactive credential prompt (this tool is not an authentication UI).
      env: { ...process.env, GIT_OPTIONAL_LOCKS: '0', GIT_TERMINAL_PROMPT: '0' },
      maxBuffer: 32 * 1024 * 1024,
    });
    return { ok: true, stdout, stderr: '' };
  } catch (error) {
    return {
      ok: false,
      stdout: typeof error.stdout === 'string' ? error.stdout : '',
      stderr: typeof error.stderr === 'string' ? error.stderr : String(error.message ?? error),
    };
  }
}

async function gitOut(cwd, args, owner) {
  const result = await runGit(cwd, args);
  if (!result.ok) {
    throw new RefusalError(`${owner}: git ${args.join(' ')} failed (${firstLine(result.stderr) || 'no diagnostic'})`);
  }
  return result.stdout;
}

function parseArgs(argv) {
  // Help is a standalone mode: anywhere else the token is just an unknown
  // argument, so a stray `--help` can never silently swallow the rest.
  if (argv.length === 1 && (argv[0] === '--help' || argv[0] === '-h')) return { help: true };
  let worktree;
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--worktree') {
      const value = argv[index + 1];
      if (value === undefined || value.startsWith('--')) {
        throw new UsageError('--worktree requires a value');
      }
      if (worktree !== undefined) throw new UsageError('--worktree may be given exactly once');
      worktree = value;
      index += 1;
      continue;
    }
    throw new UsageError(`unknown argument: ${arg}`);
  }
  if (worktree === undefined) throw new UsageError('--worktree <absolute-checkout> is required');
  if (!isAbsolute(worktree)) throw new UsageError(`--worktree must be an absolute path: ${worktree}`);
  return { help: false, worktree };
}

async function listWorktrees(cwd, owner) {
  const out = await gitOut(cwd, ['worktree', 'list', '--porcelain'], owner);
  const worktrees = [];
  for (const line of out.split('\n')) {
    if (!line.startsWith('worktree ')) continue;
    const real = await canonicalize(line.slice('worktree '.length).trim());
    if (real) worktrees.push(real);
  }
  return worktrees;
}

async function readDeclaredModules(repoDir, owner) {
  if (!existsSync(join(repoDir, '.gitmodules'))) return [];
  const result = await runGit(repoDir, ['config', '--file', '.gitmodules', '--get-regexp', '^submodule\\..*$']);
  if (!result.ok) {
    if (result.stdout.trim() === '' && firstLine(result.stderr) === '') return [];
    throw new RefusalError(`${owner}: cannot parse ${join(repoDir, '.gitmodules')} (${firstLine(result.stderr) || 'git config failed'})`);
  }
  const sections = new Map();
  for (const line of result.stdout.split('\n')) {
    if (line.trim() === '') continue;
    const separator = line.indexOf(' ');
    if (separator === -1) continue;
    const match = /^submodule\.(.+)\.(path|url)$/.exec(line.slice(0, separator));
    if (!match) continue;
    const section = sections.get(match[1]) ?? {};
    section[match[2]] = line.slice(separator + 1);
    sections.set(match[1], section);
  }
  const declared = [];
  for (const [name, section] of sections) {
    if (!section.path) throw new RefusalError(`${owner}: submodule '${name}' in .gitmodules declares no path`);
    if (!section.url) throw new RefusalError(`${owner}: submodule '${name}' in .gitmodules declares no url`);
    declared.push({ name, path: section.path, url: section.url });
  }
  return declared;
}

/**
 * Read one repository's index and fail closed on a submodule-relevant conflict.
 *
 * This deliberately does not depend on the `.gitmodules` file existing in the
 * working tree. An index that still carries an unmerged `.gitmodules` entry or an
 * unmerged gitlink describes a conflict that was never resolved, and
 * `readDeclaredModules` cannot see it once the file is gone. Returns the
 * committed (stage 0) gitlink paths so the caller can prove that `.gitmodules`
 * actually declares them.
 */
async function readIndexSubmoduleState(repoDir, scope, owner) {
  const out = await gitOut(repoDir, ['ls-files', '-s', '-z'], owner);
  const gitlinks = [];
  for (const entry of out.split('\0')) {
    if (entry === '') continue;
    const tab = entry.indexOf('\t');
    if (tab === -1) continue;
    const [mode, , stage] = entry.slice(0, tab).split(' ');
    const path = entry.slice(tab + 1);
    if (stage !== '0') {
      if (path !== '.gitmodules' && mode !== GITLINK_MODE) continue;
      throw new RefusalError(`${owner}: ${scope} has an unmerged entry '${path}' (mode ${mode} stage ${stage}); resolve the conflict before initializing submodules`);
    }
    if (mode === GITLINK_MODE) gitlinks.push(path);
  }
  return gitlinks;
}

async function readGitlink(repoDir, relInParent, owner) {
  const out = await gitOut(repoDir, ['ls-files', '-s', '--', relInParent], owner);
  const lines = out.split('\n').filter(line => line.trim() !== '');
  if (lines.length !== 1) {
    throw new RefusalError(`${owner}: expected one index entry in ${repoDir} but found ${lines.length}`);
  }
  const [mode, sha, stage] = lines[0].split('\t')[0].split(' ');
  if (mode !== GITLINK_MODE || stage !== '0') {
    throw new RefusalError(`${owner}: index entry is mode ${mode} stage ${stage}, not a committed gitlink (${GITLINK_MODE})`);
  }
  return sha;
}

async function assertMissingPathIsVacant(record, owner) {
  const stat = await lstat(record.dir).catch(() => null);
  if (!stat) return;
  if (stat.isSymbolicLink()) {
    throw new RefusalError(`${owner}: submodule '${record.path}' path ${record.dir} is a symlink; refusing to initialize over it`);
  }
  if (!stat.isDirectory()) {
    throw new RefusalError(`${owner}: submodule '${record.path}' path ${record.dir} is not a directory; refusing to initialize over it`);
  }
  const entries = await readdir(record.dir).catch(error => {
    throw new RefusalError(`${owner}: submodule '${record.path}' is not initialized and ${record.dir} cannot be read (${error?.code ?? firstLine(error?.message ?? error)}); refusing to initialize over it`);
  });
  if (entries.length > 0) {
    throw new RefusalError(`${owner}: submodule '${record.path}' is not initialized but ${record.dir} is not empty (${entries.slice(0, 3).join(', ')}); refusing to initialize over existing files`);
  }
}

async function validateSubmodule(record, worktreeGitDir, owner) {
  const absGitDir = await runGit(record.dir, ['rev-parse', '--absolute-git-dir']);
  if (!absGitDir.ok) {
    throw new RefusalError(`${owner}: submodule '${record.path}' at ${record.dir} does not resolve its own administrative directory (${firstLine(absGitDir.stderr) || 'git rev-parse failed'}); malformed or copied pointers are refused, never repaired`);
  }
  const gitDir = await canonicalize(absGitDir.stdout.trim());
  if (!gitDir) {
    throw new RefusalError(`${owner}: submodule '${record.path}' administrative directory ${absGitDir.stdout.trim()} does not exist`);
  }
  const relAdmin = relative(worktreeGitDir, gitDir);
  if (relAdmin === '' || relAdmin.startsWith('..') || isAbsolute(relAdmin) || relAdmin.split(sep)[0] !== 'modules') {
    throw new RefusalError(`${owner}: submodule '${record.path}' administrative directory ${gitDir} is outside the native per-worktree subtree ${join(worktreeGitDir, 'modules')}; metadata from another checkout must not be reused`);
  }
  const showTop = await runGit(record.dir, ['rev-parse', '--show-toplevel']);
  const topLevel = showTop.ok ? await canonicalize(showTop.stdout.trim()) : null;
  if (topLevel !== await canonicalize(record.dir)) {
    throw new RefusalError(`${owner}: submodule '${record.path}' resolves the working tree ${topLevel ?? (firstLine(showTop.stderr) || 'nothing')} instead of ${record.dir}`);
  }
  const head = (await gitOut(record.dir, ['rev-parse', 'HEAD'], owner)).trim();
  const coreWorktree = await runGit(record.dir, ['config', '--get', 'core.worktree']);
  const declaredWorktree = coreWorktree.ok ? coreWorktree.stdout.trim() : '';
  if (declaredWorktree !== '') {
    const resolved = await canonicalize(resolve(gitDir, declaredWorktree));
    if (resolved !== await canonicalize(record.dir)) {
      throw new RefusalError(`${owner}: submodule '${record.path}' core.worktree resolves to ${resolved} instead of ${record.dir}`);
    }
  }
  const origin = await runGit(record.dir, ['config', '--get', 'remote.origin.url']);
  const originUrl = origin.ok ? origin.stdout.trim() : '';
  if (originUrl !== record.url) {
    throw new RefusalError(`${owner}: submodule '${record.path}' origin url ${originUrl || '(unset)'} does not match the .gitmodules url ${record.url}`);
  }
  const unmerged = (await gitOut(record.dir, ['ls-files', '-u'], owner)).trim();
  if (unmerged !== '') {
    throw new RefusalError(`${owner}: submodule '${record.path}' at ${record.dir} has unmerged index entries; resolve the conflict before initializing submodules`);
  }
  // --ignore-submodules keeps a deliberately different submodule HEAD (and a
  // not-yet-initialized nested submodule) out of the dirt verdict; the nested
  // submodule is judged by its own validated state.
  const dirty = firstLine(await gitOut(record.dir, ['status', '--porcelain', '--ignore-submodules=all'], owner));
  if (dirty !== '') {
    throw new RefusalError(`${owner}: submodule '${record.path}' at ${record.dir} has local changes (${dirty}); refusing to touch it`);
  }
  const indexPathRaw = (await gitOut(record.dir, ['rev-parse', '--git-path', 'index'], owner)).trim();
  const indexPath = await canonicalize(resolve(record.dir, indexPathRaw));
  if (!indexPath || !isInside(gitDir, indexPath)) {
    throw new RefusalError(`${owner}: submodule '${record.path}' index path ${indexPath ?? indexPathRaw} is outside its administrative directory ${gitDir}`);
  }
  return { git_dir: gitDir, head, index_path: indexPath };
}

async function walkSubmodules(checkout, repoDir, prefix, worktreeGitDir, records, owner) {
  const scope = prefix === '' ? 'the superproject index' : `the index of submodule '${prefix}'`;
  const gitlinks = await readIndexSubmoduleState(repoDir, scope, owner);
  const declared = await readDeclaredModules(repoDir, owner);
  const undeclared = gitlinks.filter(gitlink => !declared.some(module => module.path === gitlink));
  if (undeclared.length > 0) {
    throw new RefusalError(`${owner}: ${scope} records gitlink(s) that .gitmodules does not declare: ${undeclared.join(', ')}; refusing to report a checkout whose submodule state was not established`);
  }
  for (const module of declared) {
    const path = prefix === '' ? module.path : `${prefix}/${module.path}`;
    const record = {
      name: module.name,
      url: module.url,
      path,
      dir: join(checkout, path),
      gitlink: await readGitlink(repoDir, module.path, owner),
    };
    if (!existsSync(join(record.dir, '.git'))) {
      await assertMissingPathIsVacant(record, owner);
      records.push({ ...record, status: 'missing' });
      continue;
    }
    records.push({ ...record, status: 'present', ...(await validateSubmodule(record, worktreeGitDir, owner)) });
    await walkSubmodules(checkout, record.dir, path, worktreeGitDir, records, owner);
  }
}

async function inspect(worktree) {
  const owner = `worktree ${worktree}`;
  const topLevel = await canonicalize((await gitOut(worktree, ['rev-parse', '--show-toplevel'], owner)).trim());
  if (topLevel !== worktree) {
    throw new RefusalError(`${owner}: not the root of a Git working tree (git reports ${topLevel ?? 'nothing'})`);
  }
  const gitDir = await canonicalize((await gitOut(worktree, ['rev-parse', '--absolute-git-dir'], owner)).trim());
  const commonDir = await canonicalize(resolve(worktree, (await gitOut(worktree, ['rev-parse', '--git-common-dir'], owner)).trim()));
  if (!gitDir || !commonDir) {
    throw new RefusalError(`${owner}: Git reported no usable administrative directories`);
  }
  const worktrees = await listWorktrees(worktree, owner);
  const [mainRoot] = worktrees;
  if (!mainRoot) throw new RefusalError(`${owner}: repository reports no worktrees`);
  if (!worktrees.includes(worktree)) {
    throw new RefusalError(`${owner}: not registered in this repository's worktree list (a copied checkout is not a worktree)`);
  }
  const worktreesRoot = join(mainRoot, '.worktrees');
  if (worktree === mainRoot) {
    throw new RefusalError(`${owner}: refusing the main worktree; only linked worktrees under ${worktreesRoot} are initialized`);
  }
  const worktreesRootStat = await lstat(worktreesRoot).catch(() => null);
  if (!worktreesRootStat || !worktreesRootStat.isDirectory() || worktreesRootStat.isSymbolicLink()) {
    throw new RefusalError(`${owner}: ${worktreesRoot} must be a real directory (missing or a symlink)`);
  }
  if (dirname(worktree) !== worktreesRoot) {
    throw new RefusalError(`${owner}: linked worktrees must live directly under ${worktreesRoot}`);
  }
  if (!WORKTREE_NAME.test(basename(worktree))) {
    throw new RefusalError(`${owner}: unsupported worktree directory name ${basename(worktree)}`);
  }
  if (dirname(gitDir) !== join(commonDir, 'worktrees')) {
    throw new RefusalError(`${owner}: administrative directory ${gitDir} is not the native linked-worktree directory ${join(commonDir, 'worktrees')}`);
  }
  const records = [];
  await walkSubmodules(worktree, worktree, '', gitDir, records, owner);
  const seen = new Map();
  for (const record of records) {
    if (record.status !== 'present') continue;
    for (const key of ['git_dir', 'index_path']) {
      const id = `${key} ${record[key]}`;
      const holder = seen.get(id);
      if (holder) {
        throw new RefusalError(`${owner}: submodules '${holder}' and '${record.path}' share ${key} ${record[key]}; submodule metadata must stay private to a checkout`);
      }
      seen.set(id, record.path);
    }
  }
  return { worktree, mainRoot, gitDir, commonDir, records };
}

function groupMissing(records, checkout) {
  const present = records.filter(record => record.status === 'present');
  const missing = records.filter(record => record.status === 'missing');
  const groups = new Map();
  for (const record of missing) {
    const ancestor = present
      .filter(candidate => record.path.startsWith(`${candidate.path}/`))
      .sort((left, right) => right.path.length - left.path.length)[0];
    const repoDir = ancestor ? ancestor.dir : checkout;
    const relInRepo = ancestor ? record.path.slice(ancestor.path.length + 1) : record.path;
    const group = groups.get(repoDir) ?? { repoDir, entries: [] };
    group.entries.push({ record, relInRepo });
    groups.set(repoDir, group);
  }
  const groupsToInitialize = [];
  for (const group of groups.values()) {
    // Only missing top-level entries are targeted; `--recursive` covers nested
    // ones, and an already initialized submodule is never passed to Git.
    const entries = group.entries.filter(({ record }) =>
      !group.entries.some(other => other.record !== record && record.path.startsWith(`${other.record.path}/`)));
    groupsToInitialize.push({ repoDir: group.repoDir, entries });
  }
  return groupsToInitialize;
}

async function initializeMissing(groups, mutation) {
  for (const group of groups) {
    const paths = group.entries.map(entry => entry.relInRepo);
    // From here on a mutating command is issued, so this run can no longer claim it did nothing on a
    // failure. The flag is set BEFORE the command runs because a native command that fails part-way
    // through its own work has still changed the tree; it does not by itself mean something is
    // initialized (the command can complete no submodule at all), which is why the refusal class is
    // decided by re-observing the checkout instead.
    mutation.attempted = true;
    const result = await runGit(group.repoDir, ['submodule', 'update', '--init', '--recursive', '--', ...paths]);
    if (!result.ok) {
      throw new RefusalError(`native initialization failed in ${group.repoDir} for ${paths.join(', ')} (${firstLine(result.stderr) || 'git exited nonzero'})`);
    }
  }
}

/**
 * Which submodule paths are initialized in the checkout at the moment of a failure, observed by a
 * read-only re-inspection rather than inferred from this run's own bookkeeping — a native command
 * that failed part-way through its work leaves state that no bookkeeping recorded. `null` means the
 * re-inspection itself could not be completed, which is reported as an unknown state, never as an
 * empty one.
 */
async function initializedPathsAt(worktree) {
  try {
    const state = await inspect(worktree);
    return state.records.filter(record => record.status === 'present').map(record => record.path);
  } catch {
    return null;
  }
}

function writeStdout(value) {
  process.stdout.write(`${JSON.stringify(value)}\n`);
}

/**
 * The one refusal report. The class follows the checkout as re-observed after the failure, not this
 * run's intent: `init.refuse.partial` means at least one submodule is initialized there and
 * `initialized_paths` lists those paths, while `init.refuse.preflight` means the report names no
 * initialized path — no mutation was attempted, or the attempted command completed no submodule —
 * and `initialized_paths` is `null` when nothing could be enumerated because the checkout could not
 * be re-read (an unknown state, never reported as an empty one).
 */
async function refusal(worktree, reason, mutation) {
  const initializedPaths = mutation.attempted ? await initializedPathsAt(worktree) : null;
  const partial = initializedPaths !== null && initializedPaths.length > 0;
  const code = partial ? 'init.refuse.partial' : 'init.refuse.preflight';
  const note = partial
    ? `${code}: at least one submodule was initialized by this run and is NOT rolled back; already initialized: ${initializedPaths.join(', ')}. Re-run to validate the initialized subset and finish the rest, or inspect with \`git submodule status --recursive\`.`
    : initializedPaths === null && mutation.attempted
      ? `${code}: the checkout could not be re-read, so no initialized path is enumerated (unknown state, never an empty one); nothing is rolled back.`
      : initializedPaths === null
        ? `${code}: no mutation was attempted by this run, so the checkout is unchanged and nothing is enumerated.`
        : `${code}: the native command completed no submodule before it failed, so the checkout is unchanged and no initialized path is enumerated.`;
  process.stderr.write(`init-worktree-submodules: refused: ${reason}\ninit-worktree-submodules: ${note}\n`);
  writeStdout({ version: VERSION, worktree, submodules: [], ok: false, refusal: { code, detail: reason, initialized_paths: initializedPaths } });
  return 1;
}

async function run() {
  const options = parseArgs(process.argv.slice(2));
  if (options.help) {
    process.stdout.write(USAGE);
    return 0;
  }
  const worktree = await canonicalize(options.worktree);
  if (!worktree) return await refusal(options.worktree, `not an existing path: ${options.worktree}`, { attempted: false });
  // `attempted` records that a mutating command was issued; it does not decide the refusal class,
  // which is read from the checkout re-observed after a failure (see `refusal`).
  const mutation = { attempted: false };
  try {
    let state = await inspect(worktree);
    const initializedBefore = new Map(state.records.filter(record => record.status === 'present').map(record => [record.path, record]));
    const groups = groupMissing(state.records, worktree);
    if (groups.length > 0) {
      await initializeMissing(groups, mutation);
      const after = await inspect(worktree);
      const stillMissing = after.records.filter(record => record.status === 'missing');
      if (stillMissing.length > 0) {
        throw new RefusalError(`worktree ${worktree}: native initialization left ${stillMissing.map(record => record.path).join(', ')} uninitialized`);
      }
      for (const record of after.records) {
        const prior = initializedBefore.get(record.path);
        if (!prior) continue;
        for (const key of ['git_dir', 'head', 'index_path']) {
          if (record[key] !== prior[key]) {
            throw new RefusalError(`worktree ${worktree}: native initialization moved the already initialized submodule '${record.path}' (${key} ${prior[key]} -> ${record[key]}); state preserved for manual inspection`);
          }
        }
      }
      state = after;
    }
    const submodules = state.records.map(record => ({
      path: record.path,
      git_dir: record.git_dir,
      head: record.head,
      gitlink: record.gitlink,
      action: initializedBefore.has(record.path) ? 'validated' : 'initialized',
    }));
    for (const record of submodules) {
      if (record.head !== record.gitlink) {
        process.stderr.write(`init-worktree-submodules: ${record.path}: HEAD ${record.head} differs from the recorded gitlink ${record.gitlink} (reported, never reset)\n`);
      }
    }
    writeStdout({ version: VERSION, worktree, submodules, ok: true });
    return 0;
  } catch (error) {
    if (error instanceof RefusalError) return await refusal(worktree, error.message, mutation);
    // Every other operational failure keeps the documented stdout contract too:
    // the refusal object is still the only thing on stdout and the exit code is
    // 1, never a bare stack with empty stdout. Invalid invocation is unaffected
    // because parseArgs runs before this block and throws UsageError (exit 2).
    process.stderr.write(`init-worktree-submodules: unexpected failure: ${error?.stack ?? error}\n`);
    return await refusal(worktree, `unexpected operational failure: ${firstLine(error?.stack ?? error)}`, mutation);
  }
}

try {
  process.exitCode = await run();
} catch (error) {
  if (error instanceof UsageError) {
    process.stderr.write(`init-worktree-submodules: ${error.message}\n\n${USAGE}`);
    process.exitCode = 2;
  } else {
    // Backstop for a failure `run()` could not report itself, i.e. writing the
    // refusal object was impossible too (for example stdout is already gone).
    // Operational failures inside `run()` are normalized into that JSON object.
    process.stderr.write(`init-worktree-submodules: fatal: ${error?.stack ?? error}\n`);
    process.exitCode = 1;
  }
}
