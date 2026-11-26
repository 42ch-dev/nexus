//
// Regression coverage for `.githooks/pre-push` lint scoping: a change set that
// `.oxlintrc.json` ignores entirely used to abort the push, because oxlint exits
// 1 ("No files found to lint") when every path it is given is ignored.
//
// Each case runs the REAL hook against a disposable git repo with a stub `pnpm`
// on PATH, so the assertions stay on hook behaviour (which files reach oxlint,
// whether build/typecheck still run) without building or linting the workspace.
// The stub models oxlint's failure: it exits 1 when every file it receives is
// ignored, which is exactly what the hook must never hand it.

import { after, before, test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import {
  chmodSync,
  copyFileSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const HOOK = path.join(REPO_ROOT, '.githooks', 'pre-push');

const GENERATED = 'packages/contracts/src/generated/api.ts';
const SOURCE = 'packages/contracts/src/index.ts';

const IDENTITY = {
  GIT_AUTHOR_NAME: 'pre-push-sim',
  GIT_AUTHOR_EMAIL: 'pre-push-sim@example.invalid',
  GIT_COMMITTER_NAME: 'pre-push-sim',
  GIT_COMMITTER_EMAIL: 'pre-push-sim@example.invalid',
};

const STUB_PNPM = `#!/bin/sh
printf '%s\\n' "$*" >> "$STUB_LOG"
if [ "$1" = "exec" ] && [ "$2" = "oxlint" ]; then
  shift 2
  live=0
  for file do
    case "$file" in
      *src/generated/*) ;;
      *) live=$((live + 1)) ;;
    esac
  done
  if [ "$live" -eq 0 ]; then
    echo "No files found to lint. Please check your paths and ignore patterns." >&2
    exit 1
  fi
fi
exit 0
`;

const root = mkdtempSync(path.join(tmpdir(), 'pre-push-hook-'));
const stubLog = path.join(root, 'stub.log');

function write(relative, content) {
  const file = path.join(root, relative);
  mkdirSync(path.dirname(file), { recursive: true });
  writeFileSync(file, content);
}

function git(...args) {
  const result = spawnSync('git', args, {
    cwd: root,
    encoding: 'utf8',
    env: { ...process.env, ...IDENTITY },
  });
  assert.equal(result.status, 0, `git ${args.join(' ')} failed: ${result.stderr}`);
  return result.stdout.trim();
}

function commit(message) {
  git('add', '-A');
  git('commit', '-q', '-m', message);
  return git('rev-parse', 'HEAD');
}

// Run the hook exactly as Git would: pushed refspec on stdin, cwd at the
// work-tree root, the stub `pnpm` first on PATH.
function push(head, base) {
  writeFileSync(stubLog, '');
  const result = spawnSync('sh', [path.join(root, '.githooks', 'pre-push')], {
    cwd: root,
    encoding: 'utf8',
    input: `refs/heads/sim ${head} refs/heads/sim ${base}\n`,
    env: {
      ...process.env,
      PATH: `${path.join(root, 'bin')}${path.delimiter}${process.env.PATH}`,
      STUB_LOG: stubLog,
    },
  });
  return {
    status: result.status,
    output: `${result.stdout}${result.stderr}`,
    log: readFileSync(stubLog, 'utf8'),
  };
}

function oxlintInvocation(log) {
  return log.split('\n').find((line) => line.startsWith('exec oxlint')) ?? '';
}

before(() => {
  git('init', '-q', '-b', 'main');
  write('.oxlintrc.json', JSON.stringify({ ignorePatterns: ['packages/contracts/src/generated/**'] }, null, 2));
  write('packages/contracts/package.json', JSON.stringify({ name: '@sim/contracts', version: '0.0.0' }, null, 2));
  write(GENERATED, 'export const generated = 1;\n');
  write(SOURCE, 'export const source = 1;\n');
  write('bin/pnpm', STUB_PNPM);
  chmodSync(path.join(root, 'bin', 'pnpm'), 0o755);
  mkdirSync(path.join(root, '.githooks'), { recursive: true });
  copyFileSync(HOOK, path.join(root, '.githooks', 'pre-push'));
  commit('base');
});

after(() => {
  rmSync(root, { recursive: true, force: true });
});

test('generated-only change set skips oxlint but still builds and typechecks', () => {
  const prev = git('rev-parse', 'HEAD');
  write(GENERATED, 'export const generated = 2;\n');
  const run = push(commit('generated only'), prev);

  assert.equal(run.status, 0, run.output);
  assert.match(run.output, /Skipping oxlint: every changed lint file is ignored by \.oxlintrc\.json\./);
  assert.doesNotMatch(run.log, /exec oxlint/, 'ignored paths must not reach oxlint');
  assert.match(run.log, /--filter \.\/packages\/contracts\.\.\. run --if-present build/);
  assert.match(run.log, /--filter \.\/packages\/contracts run --if-present typecheck/);
});

test('ordinary source change is still linted', () => {
  const prev = git('rev-parse', 'HEAD');
  write(SOURCE, 'export const source = 2;\n');
  const run = push(commit('source only'), prev);

  assert.equal(run.status, 0, run.output);
  assert.match(oxlintInvocation(run.log), /packages\/contracts\/src\/index\.ts/);
  assert.doesNotMatch(run.output, /Skipping oxlint/);
});

test('mixed change set lints only the non-ignored path', () => {
  const prev = git('rev-parse', 'HEAD');
  write(GENERATED, 'export const generated = 3;\n');
  write(SOURCE, 'export const source = 3;\n');
  const run = push(commit('mixed'), prev);

  assert.equal(run.status, 0, run.output);
  const invocation = oxlintInvocation(run.log);
  assert.match(invocation, /packages\/contracts\/src\/index\.ts/);
  assert.doesNotMatch(invocation, /generated/);
  assert.doesNotMatch(run.output, /Skipping oxlint/);
});
