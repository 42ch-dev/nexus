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
                                  human-readable stdout mode; it probes nothing.

stdout (operational invocations) is exactly one JSON object:
  {"version":1,"worktree":"<absolute>","submodules":[{"path","git_dir","head",
   "gitlink","action"}],"ok":true}
  "action" is "initialized" (native init created it in this run) or "validated"
  (already initialized). A submodule HEAD that differs from the recorded gitlink
  is reported on stderr and preserved, never reset. Nothing but the JSON object
  is written to stdout. A refusal still writes that object with "ok":false and an
  empty "submodules" list (the reason is on stderr), so stdout stays parseable.

Exit codes:
  0  checkout valid, submodules validated or initialized
  1  Git or safety refusal; the reason is on stderr and the state is unchanged
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
  let worktree;
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--help' || arg === '-h') return { help: true };
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
  const entries = await readdir(record.dir);
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
  for (const module of await readDeclaredModules(repoDir, owner)) {
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
  if (existsSync(join(worktree, '.gitmodules'))) {
    const unmerged = (await gitOut(worktree, ['ls-files', '-u', '--', '.gitmodules'], owner)).trim();
    if (unmerged !== '') {
      throw new RefusalError(`${owner}: .gitmodules has unmerged index entries; resolve the conflict before initializing submodules`);
    }
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

async function initializeMissing(groups) {
  for (const group of groups) {
    const paths = group.entries.map(entry => entry.relInRepo);
    const result = await runGit(group.repoDir, ['submodule', 'update', '--init', '--recursive', '--', ...paths]);
    if (!result.ok) {
      throw new RefusalError(`native initialization failed in ${group.repoDir} for ${paths.join(', ')} (${firstLine(result.stderr) || 'git exited nonzero'})`);
    }
  }
}

function writeStdout(value) {
  process.stdout.write(`${JSON.stringify(value)}\n`);
}

function refusal(worktree, reason) {
  process.stderr.write(`init-worktree-submodules: refused: ${reason}\n`);
  writeStdout({ version: VERSION, worktree, submodules: [], ok: false });
  return 1;
}

async function run() {
  const options = parseArgs(process.argv.slice(2));
  if (options.help) {
    process.stdout.write(USAGE);
    return 0;
  }
  const worktree = await canonicalize(options.worktree);
  if (!worktree) return refusal(options.worktree, `not an existing path: ${options.worktree}`);
  try {
    let state = await inspect(worktree);
    const initializedBefore = new Map(state.records.filter(record => record.status === 'present').map(record => [record.path, record]));
    const groups = groupMissing(state.records, worktree);
    if (groups.length > 0) {
      await initializeMissing(groups);
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
    if (error instanceof RefusalError) return refusal(worktree, error.message);
    throw error;
  }
}

try {
  process.exitCode = await run();
} catch (error) {
  if (error instanceof UsageError) {
    process.stderr.write(`init-worktree-submodules: ${error.message}\n\n${USAGE}`);
    process.exitCode = 2;
  } else {
    process.stderr.write(`init-worktree-submodules: unexpected failure: ${error?.stack ?? error}\n`);
    process.exitCode = 1;
  }
}
