import assert from 'node:assert/strict';
import { chmodSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { parseArgs, verifyLaunch } from '../scripts/verify-launch.mjs';

function makeFakeApp(root, name, executableSource) {
  const appPath = join(root, `${name}.app`);
  mkdirSync(join(appPath, 'Contents', 'MacOS'), { recursive: true });
  const executable = join(appPath, 'Contents', 'MacOS', 'Nexus');
  writeFileSync(executable, executableSource);
  chmodSync(executable, 0o755);
  return appPath;
}

const WINDOW_MS = 500;
const SLEEP_SCRIPT = '#!/bin/sh\nexec sleep 30\n';

test('stays alive through the window with no marker: PASS', async () => {
  const root = mkdtempSync(join(tmpdir(), 'nexus-verify-launch-'));
  try {
    const appPath = makeFakeApp(root, 'Alive', SLEEP_SCRIPT);
    const record = await verifyLaunch({ appPath, timeoutMs: WINDOW_MS });
    assert.equal(record.verdict, 'PASS');
    assert.equal(record.liveness, true);
    assert.equal(record.timeoutMs, WINDOW_MS);
    assert.match(record.command, /Contents\/MacOS\/Nexus$/);
    assert.doesNotMatch(record.stderrTail, /bootstrap failed/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('exits 1 with the bootstrap-failed marker within the window: FAIL', async () => {
  const root = mkdtempSync(join(tmpdir(), 'nexus-verify-launch-'));
  try {
    const appPath = makeFakeApp(
      root,
      'BootstrapFailed',
      '#!/bin/sh\necho "[desktop] bootstrap failed: fixture ENOENT" >&2\nexit 1\n',
    );
    const record = await verifyLaunch({ appPath, timeoutMs: WINDOW_MS });
    assert.equal(record.verdict, 'FAIL');
    assert.equal(record.liveness, false);
    assert.equal(record.exitCode, 1);
    assert.match(record.reason, /bootstrap failed/);
    assert.match(record.stderrTail, /\[desktop\] bootstrap failed/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('exits 0 early within the window: FAIL (no exit-code success pattern-matching)', async () => {
  const root = mkdtempSync(join(tmpdir(), 'nexus-verify-launch-'));
  try {
    const appPath = makeFakeApp(root, 'EarlyExit', '#!/bin/sh\nexit 0\n');
    const record = await verifyLaunch({ appPath, timeoutMs: WINDOW_MS });
    assert.equal(record.verdict, 'FAIL');
    assert.equal(record.liveness, false);
    assert.equal(record.exitCode, 0);
    assert.match(record.reason, /exited within/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('non-darwin platform fails closed with an unsupported-platform diagnostic', async () => {
  const root = mkdtempSync(join(tmpdir(), 'nexus-verify-launch-'));
  try {
    const appPath = makeFakeApp(root, 'LinuxApp', SLEEP_SCRIPT);
    const record = await verifyLaunch({ appPath, timeoutMs: WINDOW_MS, platform: 'linux' });
    assert.equal(record.verdict, 'FAIL');
    assert.equal(record.liveness, false);
    assert.match(record.reason, /unsupported platform: linux/);
    assert.match(record.reason, /fails closed/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('argument contract: --app required, --timeout-ms defaults to 20000', () => {
  const args = parseArgs(['--app', 'Nexus.app']);
  assert.equal(args.app, 'Nexus.app');
  assert.equal(args.timeoutMs, 20000);
  assert.throws(() => parseArgs([]), /--app is required/);
  assert.throws(() => parseArgs(['--app']), /--app requires a value/);
  assert.throws(() => parseArgs(['--app', 'Nexus.app', '--timeout-ms', '0']), /--timeout-ms requires/);
  assert.throws(() => parseArgs(['--app', 'Nexus.app', '--verbose']), /unsupported option/);
});
