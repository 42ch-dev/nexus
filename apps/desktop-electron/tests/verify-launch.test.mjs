import assert from 'node:assert/strict';
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { parseArgs, verifyLaunch } from '../scripts/verify-launch.mjs';

function makeFakeApp(root, name, executableSource, { executableName = 'Nexus', infoPlist } = {}) {
  const appPath = join(root, `${name}.app`);
  mkdirSync(join(appPath, 'Contents', 'MacOS'), { recursive: true });
  const executable = join(appPath, 'Contents', 'MacOS', executableName);
  writeFileSync(executable, executableSource);
  chmodSync(executable, 0o755);
  if (infoPlist !== undefined) writeFileSync(join(appPath, 'Contents', 'Info.plist'), infoPlist);
  return appPath;
}

const WINDOW_MS = 500;
// Fixtures whose stub exits on its own must not let a delayed exit event lose
// the race to the window timer: under the suite's default test-file concurrency
// an exit event can be delayed past 500ms (observed ~750ms), flipping the
// expected FAIL to PASS. These fixtures use a window that absorbs that jitter;
// their cost stays the child's actual exit time, not the window.
const EXIT_WINDOW_MS = 5000;
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
    const record = await verifyLaunch({ appPath, timeoutMs: EXIT_WINDOW_MS });
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
    const record = await verifyLaunch({ appPath, timeoutMs: EXIT_WINDOW_MS });
    assert.equal(record.verdict, 'FAIL');
    assert.equal(record.liveness, false);
    assert.equal(record.exitCode, 0);
    assert.match(record.reason, /exited within/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

/**
 * Poll until `pid` is no longer signalable, or `timeoutMs` elapses. Keeps the
 * descendant assertion independent of scheduling jitter.
 */
async function processReaped(pid, timeoutMs) {
  const isAlive = () => {
    try {
      process.kill(pid, 0);
      return true;
    } catch {
      return false;
    }
  };
  const deadline = Date.now() + timeoutMs;
  while (isAlive() && Date.now() < deadline) {
    await new Promise((resolveWait) => setTimeout(resolveWait, 25));
  }
  return !isAlive();
}

test('early exit with a descendant holding the pipes: FAIL and the descendant is reaped', async () => {
  const root = mkdtempSync(join(tmpdir(), 'nexus-verify-launch-'));
  try {
    const pidFile = join(root, 'descendant.pid');
    // The leader background-forks a long-lived descendant that inherits the
    // captured pipes, records its pid, then exits early. Without process-group
    // termination the inherited pipe keeps the probe pending and the descendant
    // survives; the probe must terminate the group and destroy the streams.
    const appPath = makeFakeApp(root, 'DescendantHeldPipes', `#!/bin/sh\nsleep 30 &\necho $! > '${pidFile}'\nexit 1\n`);
    const record = await verifyLaunch({ appPath, timeoutMs: EXIT_WINDOW_MS });
    assert.equal(record.verdict, 'FAIL');
    assert.equal(record.exitCode, 1);
    const descendantPid = Number(readFileSync(pidFile, 'utf8').trim());
    assert.ok(Number.isInteger(descendantPid) && descendantPid > 0, 'fixture recorded the descendant pid');
    assert.ok(await processReaped(descendantPid, 5000), `descendant ${descendantPid} was not reaped by probe cleanup`);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('early exit with a SIGTERM-ignoring descendant: FAIL and the descendant is escalated and reaped', async () => {
  const root = mkdtempSync(join(tmpdir(), 'nexus-verify-launch-'));
  try {
    const readyFile = join(root, 'stubborn.pid');
    // The descendant installs a SIGTERM handler and only then records its pid,
    // so by the time the leader exits (after observing the ready file) it is
    // genuinely SIGTERM-resistant — only the probe's awaited SIGKILL escalation
    // can reap it. It inherits the captured stderr pipe, and the leader exits
    // early, so the early-exit FAIL must still resolve promptly and then leave
    // the owned group member reaped.
    const descendantSource = 'process.on("SIGTERM", () => {}); '
      + `require("node:fs").writeFileSync(${JSON.stringify(readyFile)}, String(process.pid)); `
      + 'setInterval(() => {}, 1000)';
    const leaderSource = '#!/usr/bin/env node\n'
      + 'const fs = require("node:fs");\n'
      + 'const { spawn } = require("node:child_process");\n'
      + `const ready = ${JSON.stringify(readyFile)};\n`
      + `spawn(process.execPath, ["-e", ${JSON.stringify(descendantSource)}], { stdio: ["ignore", "ignore", "inherit"] });\n`
      + 'const wait = () => (fs.existsSync(ready) ? process.exit(1) : setTimeout(wait, 20));\n'
      + 'wait();\n';
    const appPath = makeFakeApp(root, 'StubbornDescendant', leaderSource);
    const startedAt = Date.now();
    const record = await verifyLaunch({ appPath, timeoutMs: EXIT_WINDOW_MS });
    const elapsedMs = Date.now() - startedAt;
    assert.equal(record.verdict, 'FAIL');
    assert.equal(record.exitCode, 1);
    // The descendant outlived SIGTERM, so the probe must have taken the bounded
    // grace wait before escalating — an immediate return would mean the fixture
    // was reaped by SIGTERM (no escalation exercised).
    assert.ok(elapsedMs >= 2000, `probe returned in ${elapsedMs}ms, before the SIGKILL escalation`);
    assert.ok(elapsedMs < EXIT_WINDOW_MS + 5000, `probe returned in ${elapsedMs}ms, not bounded`);
    const descendantPid = Number(readFileSync(readyFile, 'utf8').trim());
    assert.ok(Number.isInteger(descendantPid) && descendantPid > 0, 'fixture recorded the descendant pid');
    assert.ok(
      await processReaped(descendantPid, 5000),
      `SIGTERM-resistant descendant ${descendantPid} was not escalated by probe cleanup`,
    );
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('bootstrap marker straddling the retention boundary still FAILs (no false PASS)', async () => {
  const root = mkdtempSync(join(tmpdir(), 'nexus-verify-launch-'));
  try {
    // First chunk ends with a partial marker (well under the retention cap, so
    // the naive append→trim→scan order keeps it); the second chunk completes
    // the marker at its head but exceeds the cap, so trimming the front before
    // scanning would drop the split marker entirely and the alive-at-window-end
    // path would return a false PASS. Detection must survive the retention seam.
    const appPath = makeFakeApp(
      root,
      'StraddlingMarker',
      '#!/usr/bin/env node\n'
        + 'process.stderr.write("padding [desktop] boot");\n'
        + 'setTimeout(() => { process.stderr.write("strap failed" + "x".repeat(65536)); }, 50);\n'
        + 'setTimeout(() => {}, 30000);\n',
    );
    const record = await verifyLaunch({ appPath, timeoutMs: 1500 });
    assert.equal(record.verdict, 'FAIL');
    assert.equal(record.liveness, true);
    assert.match(record.reason, /bootstrap failed/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('derives the probed executable from Info.plist CFBundleExecutable', async () => {
  const root = mkdtempSync(join(tmpdir(), 'nexus-verify-launch-'));
  try {
    const appPath = makeFakeApp(root, 'DerivedExecutable', SLEEP_SCRIPT, {
      executableName: 'NexusAlt',
      infoPlist: '<?xml version="1.0" encoding="UTF-8"?>\n'
        + '<plist version="1.0"><dict><key>CFBundleExecutable</key><string>NexusAlt</string></dict></plist>\n',
    });
    const record = await verifyLaunch({ appPath, timeoutMs: WINDOW_MS });
    assert.equal(record.verdict, 'PASS');
    assert.match(record.command, /Contents\/MacOS\/NexusAlt$/);
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

test('window-end PASS requires the app to be alive: a late exit event cannot lock in a PASS', async () => {
  const root = mkdtempSync(join(tmpdir(), 'nexus-verify-launch-'));
  try {
    // The stub stays alive, but the injected liveness probe reports the app
    // already gone at the window end — the exact state a dead app is in while
    // its `exit` event is still queued behind the window timer (event-loop
    // lag). The previous implementation ignored liveness and recorded a PASS;
    // the probe must FAIL instead, so this test is red on the old code.
    const appPath = makeFakeApp(root, 'DeadAtWindowEnd', SLEEP_SCRIPT);
    const record = await verifyLaunch({
      appPath,
      timeoutMs: WINDOW_MS,
      isProcessAlive: () => false,
    });
    assert.equal(record.verdict, 'FAIL');
    assert.equal(record.liveness, false);
    assert.match(record.reason, /not alive at the 500ms window end/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('window-end PASS is still recorded when the liveness probe reports the app alive', async () => {
  const root = mkdtempSync(join(tmpdir(), 'nexus-verify-launch-'));
  try {
    const appPath = makeFakeApp(root, 'AliveProbe', SLEEP_SCRIPT);
    const record = await verifyLaunch({
      appPath,
      timeoutMs: WINDOW_MS,
      isProcessAlive: () => true,
    });
    assert.equal(record.verdict, 'PASS');
    assert.equal(record.liveness, true);
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
