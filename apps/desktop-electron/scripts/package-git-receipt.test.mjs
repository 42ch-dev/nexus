import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';

const runGit = (cwd, args) => execFileSync('git', args, { cwd, encoding: 'utf8' });

function createFixtureRepo() {
  const root = mkdtempSync(join(tmpdir(), 'nexus-package-git-receipt-'));
  runGit(root, ['init', '-q']);
  runGit(root, ['config', 'user.email', 'fixture@example.test']);
  runGit(root, ['config', 'user.name', 'Fixture']);
  writeFileSync(join(root, '.gitignore'), readFileSync(new URL('../../../.gitignore', import.meta.url))
    .toString()
    .split(/\r?\n/)
    .filter((line) => line === '/artifacts/')
    .join('\n') + '\n');
  writeFileSync(join(root, 'tracked.txt'), 'baseline\n');
  runGit(root, ['add', '.gitignore', 'tracked.txt']);
  runGit(root, ['commit', '-qm', 'fixture baseline']);
  return root;
}

function receiptDirty(root) {
  // Match package.mjs gitReceipt(): git status --porcelain, trimmed, non-empty.
  return runGit(root, ['status', '--porcelain']).trim().length > 0;
}

test('artifacts-only staging noise leaves the receipt clean', (t) => {
  const root = createFixtureRepo();
  t.after(() => rmSync(root, { recursive: true, force: true }));

  mkdirSync(join(root, 'artifacts', 'desktop', '.staging-arm64-test'), { recursive: true });
  writeFileSync(join(root, 'artifacts', 'desktop', '.staging-arm64-test', 'output'), 'build output');

  assert.equal(runGit(root, ['status', '--porcelain']).trim(), '');
  assert.equal(receiptDirty(root), false);
});

test('tracked modifications and untracked source files remain dirty', (t) => {
  const root = createFixtureRepo();
  t.after(() => rmSync(root, { recursive: true, force: true }));

  writeFileSync(join(root, 'tracked.txt'), 'modified\n');
  writeFileSync(join(root, 'new-source.txt'), 'untracked source\n');

  const status = runGit(root, ['status', '--porcelain']);
  assert.match(status, /tracked\.txt/);
  assert.match(status, /new-source\.txt/);
  assert.equal(receiptDirty(root), true);
});
