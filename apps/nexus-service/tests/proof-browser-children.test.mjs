import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { after, describe, test } from 'node:test';

const __dirname = dirname(fileURLToPath(import.meta.url));
const scriptRoot = join(__dirname, '..', 'scripts');

/**
 * Focused regression for the proof orchestrator's child lifecycle, evidence
 * bookkeeping, and adapter restore gate (`scripts/proof-browser.mjs`). The
 * orchestrator is a dependency-free single-file script, so these tests
 * extract its real helpers verbatim (stable section anchors) into throwaway
 * modules and drive them against real child processes / controllable process
 * seams — no copy of the logic, no mocks of the logic itself.
 *
 * Contracts under test:
 * - a child that emits `exit` while its piped stdio is still open must stay
 *   in the set scanned by `drainExitedChildrenForEvidence` until its `close`
 *   callback captures the bounded retained output — bytes the child (or its
 *   process-group descendants) write after `exit` must land in the failure
 *   evidence;
 * - stop and cleanup must be bounded at EVERY wait: a failed spawn (which
 *   never emits `exit`), SIGTERM-ignoring children, the post-SIGKILL wait,
 *   and exited-but-not-closed children with a surviving group descendant
 *   must all settle instead of hanging cleanup;
 * - the adapter restore gate must fail the proof when the restore build
 *   fails or the restored adapter still serves the edited marker.
 */
function sliceBetween(source, startMarker, endMarker) {
  const start = source.indexOf(startMarker);
  assert.ok(start >= 0, `proof-browser.mjs is missing anchor: ${startMarker}`);
  const end = source.indexOf(endMarker, start + startMarker.length);
  assert.ok(end > start, `proof-browser.mjs is missing anchor: ${endMarker}`);
  return source.slice(start, end);
}

const source = readFileSync(join(scriptRoot, 'proof-browser.mjs'), 'utf8');
const moduleSource = [
  "import { spawn } from 'node:child_process';",
  'const sleep = (ms) => new Promise((r) => setTimeout(r, ms));',
  sliceBetween(source, '// ── Child process lifecycle', '// ── HTTP helpers'),
  sliceBetween(source, 'const MAX_EVIDENCE_TAIL_CHARS', '// ── Direct CLI writer'),
'export { trackedChildren, retainedExitedOutput, trackChild, drainExitedChildrenForEvidence, spawnLogged, stopChild, cleanupChildren, collectChildOutput, redactChildOutput, MAX_EVIDENCE_TAIL_CHARS, RETAINED_EXITED_OUTPUT_MAX };',
].join('\n');

const workDir = mkdtempSync(join(tmpdir(), 'proof-browser-children-'));
const modulePath = join(workDir, 'child-lifecycle.mjs');
writeFileSync(modulePath, moduleSource);
const lifecycle = await import(modulePath);

// The adapter restore gate, extracted the same way: the real
// `requireRestoredAdapterBuild` / `assertRestoredAdapterBaseline` functions.
// Their process seams (spawnSync, readiness probes, page evaluate) are
// injected as controllable fakes so the gate DECISIONS are driven without
// running pnpm or a browser.
const gateWorkDir = mkdtempSync(join(tmpdir(), 'proof-browser-gate-'));
const gateModulePath = join(gateWorkDir, 'adapter-restore-gate.mjs');
const gateModuleSource = [
  // Late-bound seams: these wrappers forward to the CURRENT globalThis fake
  // at call time, so each test can install its own after the module import.
  // A value snapshot (`const spawnSync = globalThis.__rftSpawnSync`) would
  // capture `undefined` at import and every gate call would fail.
  'const spawnSync = (...args) => globalThis.__rftSpawnSync(...args);',
  'const repoRoot = globalThis.__rftRepoRoot;',
  'const waitForHttpOk = (...args) => globalThis.__rftWaitForHttpOk(...args);',
  'const waitForPageCondition = (...args) => globalThis.__rftWaitForPageCondition(...args);',
  'const evaluate = (...args) => globalThis.__rftEvaluate(...args);',
  sliceBetween(source, 'const MAX_EVIDENCE_TAIL_CHARS', '// ── Direct CLI writer'),
  sliceBetween(source, '// ── Adapter edit restore gate', '/**\n * TS SDK adapter edit sample'),
  'export { ADAPTER_SESSION_SCRIPT, requireRestoredAdapterBuild, assertRestoredAdapterBaseline };',
].join('\n');
writeFileSync(gateModulePath, gateModuleSource);
const gate = await import(gateModulePath);

/** Wait (bounded) until a spawned child's captured output contains `text`. */
function waitForChildOutput(child, text, timeoutMs = 5_000) {
  return new Promise((resolve, reject) => {
    const deadline = Date.now() + timeoutMs;
    const poll = () => {
      if (String(child.output()).includes(text)) return resolve();
      if (Date.now() > deadline) return reject(new Error(`child output never contained: ${text}`));
      setTimeout(poll, 20);
    };
    poll();
  });
}

after(() => {
  rmSync(workDir, { recursive: true, force: true });
  rmSync(gateWorkDir, { recursive: true, force: true });
});

// A child that writes, parks a process-group descendant on its inherited
// stdout (so `close` cannot fire), then exits — the exact exit-before-close
// window. The descendant appends the trailing bytes after our `exit` and dies.
const exitBeforeCloseScript = `
const { spawn } = require('node:child_process');
process.stdout.write('pre-exit-marker');
const holder = spawn(process.execPath, ['-e', 'setTimeout(() => { process.stdout.write("trailing-after-exit-marker"); }, 250)'], { stdio: ['ignore', 'inherit', 'inherit'] });
setTimeout(() => process.exit(0), 30);
`;

describe('proof-browser exited-child bookkeeping', () => {
  test('a child that exits before its stdio closes stays drainable and its trailing bytes are retained', async () => {
    const child = lifecycle.spawnLogged(process.execPath, ['-e', exitBeforeCloseScript], {
      label: 'nexus-service',
    });

    await new Promise((resolveExit) => child.once('exit', resolveExit));
    assert.equal(child.exitCode, 0);
    assert.ok(!child.__closeSettled, 'close must not fire while the stdio pipe is still held');
    assert.ok(
      lifecycle.trackedChildren.has(child),
      'an exited-but-not-closed child must remain in the set the evidence drain scans',
    );

    // Catch-path order: drain (bounded) first, then evidence assembly.
    await lifecycle.drainExitedChildrenForEvidence({ timeoutMs: 2_000 });

    assert.equal(child.__closeSettled, true, 'the drain must await the pending close');
    assert.ok(
      !lifecycle.trackedChildren.has(child),
      'the child is removed only after its close callback captured the output',
    );
    const retained = lifecycle.retainedExitedOutput.get(child.__evidenceKey);
    assert.ok(retained, 'the close callback must retain the captured tail');
    assert.match(retained, /pre-exit-marker/);
    assert.match(retained, /trailing-after-exit-marker/, 'bytes written after exit must be retained');

    // The failure evidence collector exposes the retained tail, redacted.
    const rows = lifecycle.collectChildOutput('pre-exit-marker');
    assert.equal(rows[child.__evidenceKey], retained.split('pre-exit-marker').join('<fixture-home>'));
  });
});

describe('proof-browser bounded child stop and cleanup', () => {
  test('stopChild reaps a live child, settles its lifecycle through close, and keeps the evidence tail', async () => {
    const child = lifecycle.spawnLogged(
      process.execPath,
      ['-e', 'process.stdout.write("alive-marker"); setInterval(() => {}, 1_000);'],
      { label: 'nexus-service' },
    );
    await waitForChildOutput(child, 'alive-marker');

    const startedAt = Date.now();
    await lifecycle.stopChild(child, { graceMs: 500, killMs: 500, closeMs: 1_000 });
    const elapsedMs = Date.now() - startedAt;

    assert.ok(elapsedMs < 5_000, `stopChild must stay bounded (took ${elapsedMs}ms)`);
    assert.ok(child.signalCode !== null || child.exitCode !== null, 'the child must be reaped');
    assert.equal(child.__closeSettled, true, 'stopChild must settle the lifecycle through close');
    assert.ok(!lifecycle.trackedChildren.has(child), 'a stopped child must leave the tracked set');
    assert.match(
      lifecycle.retainedExitedOutput.get(child.__evidenceKey) ?? '',
      /alive-marker/,
      'the captured tail must still be retained after a stop',
    );
  });

  test('stopChild stays bounded when the child ignores SIGTERM and must be SIGKILLed', async () => {
    const child = lifecycle.spawnLogged(
      process.execPath,
      ['-e', "process.on('SIGTERM', () => {}); process.stdout.write('ready-marker'); setInterval(() => {}, 1_000);"],
      { label: 'nexus-service' },
    );
    await waitForChildOutput(child, 'ready-marker');

    const startedAt = Date.now();
    await lifecycle.stopChild(child, { graceMs: 400, killMs: 500, closeMs: 1_000 });
    const elapsedMs = Date.now() - startedAt;

    assert.ok(elapsedMs >= 400, 'the SIGTERM grace must elapse before escalation');
    assert.ok(elapsedMs < 4_000, `post-SIGKILL stop must stay bounded (took ${elapsedMs}ms)`);
    assert.equal(child.signalCode, 'SIGKILL', 'a SIGTERM-ignoring child must be SIGKILLed');
    assert.equal(child.__closeSettled, true, 'the lifecycle must settle through close after the kill');
    assert.ok(!lifecycle.trackedChildren.has(child));
  });

  test('a spawn failure settles the lifecycle and stop/cleanup stay bounded', async () => {
    const child = lifecycle.spawnLogged('rft-definitely-missing-binary', ['--version'], {
      label: 'adapter-build-x',
    });

    await child.__closed;

    assert.equal(child.__closeSettled, true, 'a spawn-errored child must settle on error');
    assert.ok(child.__error, 'the spawn error must be captured on the child');
    assert.ok(!lifecycle.trackedChildren.has(child), 'a spawn-errored child must leave the tracked set');

    // Regression: an ENOENT child never emits `exit`; stop/cleanup used to
    // wait for that exit forever.
    const startedAt = Date.now();
    await lifecycle.stopChild(child);
    await lifecycle.cleanupChildren();
    assert.ok(Date.now() - startedAt < 2_000, 'stop/cleanup of a spawn-errored child must return immediately');
  });

  test('cleanupChildren kills a surviving group descendant of an exited child and drains its close, bounded', async () => {
    const child = lifecycle.spawnLogged(
      process.execPath,
      [
        '-e',
        [
          "const { spawn } = require('node:child_process');",
          "process.stdout.write('pre-exit-marker');",
          "spawn(process.execPath, ['-e', 'process.stdout.write(\"trailing-after-exit-marker\"); setInterval(() => {}, 1_000);'], { stdio: ['ignore', 'inherit', 'inherit'] });",
          'setTimeout(() => process.exit(0), 1_000);',
        ].join('\n'),
      ],
      { label: 'nexus-service' },
    );

    await new Promise((resolveExit) => child.once('exit', resolveExit));
    assert.ok(!child.__closeSettled, 'close must be pending while the descendant holds the pipes');

    const startedAt = Date.now();
    await lifecycle.cleanupChildren();
    const elapsedMs = Date.now() - startedAt;

    assert.ok(elapsedMs < 5_000, `cleanup must stay bounded (took ${elapsedMs}ms)`);
    assert.equal(
      child.__closeSettled,
      true,
      'cleanup must sweep the group and await the pending close — the descendant never exits on its own',
    );
    assert.ok(!lifecycle.trackedChildren.has(child));
    assert.match(
      lifecycle.retainedExitedOutput.get(child.__evidenceKey) ?? '',
      /trailing-after-exit-marker/,
      'bytes the descendant wrote before the sweep must be retained',
    );
  });
});

describe('proof-browser adapter restore gate', () => {
  const MARKER = 'RFTADAPTER7:';

  test('a failed restore build throws with bounded output', () => {
    globalThis.__rftSpawnSync = () => ({
      status: 1,
      stdout: 'x'.repeat(9_000),
      stderr: 'adapter-restore-build-error-tail',
    });
    let thrown = null;
    assert.throws(
      () => gate.requireRestoredAdapterBuild(),
      (err) => {
        thrown = err.message;
        return /adapter restore build failed \(exit 1\)/.test(err.message);
      },
    );
    assert.ok(thrown.includes('adapter-restore-build-error-tail'), 'the build stderr tail must be reported');
    assert.ok(thrown.length <= 4_300, `failure output must stay bounded (got ${thrown.length} chars)`);
  });

  test('a restore build spawn error throws instead of passing', () => {
    globalThis.__rftSpawnSync = () => ({
      status: null,
      error: new Error('spawn pnpm ENOENT'),
      stdout: '',
      stderr: '',
    });
    assert.throws(
      () => gate.requireRestoredAdapterBuild(),
      /adapter restore build failed \(exit null: spawn pnpm ENOENT\):/,
    );
  });

  test('a successful restore build passes the gate', () => {
    globalThis.__rftSpawnSync = () => ({ status: 0, stdout: '', stderr: '' });
    gate.requireRestoredAdapterBuild();
  });

  test('the restored baseline fails the proof when the marker still streams or no terminal settles', async () => {
    globalThis.__rftWaitForHttpOk = async () => undefined;
    globalThis.__rftWaitForPageCondition = async () => undefined;

    globalThis.__rftEvaluate = async () => ({ text: 'RFTADAPTER7:stale edited dist', terminal: true });
    await assert.rejects(
      () => gate.assertRestoredAdapterBaseline({}, MARKER),
      /restored adapter still emits the edited marker/,
    );

    globalThis.__rftEvaluate = async () => ({ text: 'partial', terminal: false });
    await assert.rejects(
      () => gate.assertRestoredAdapterBaseline({}, MARKER),
      /restored adapter baseline did not reach a terminal event/,
    );

    globalThis.__rftEvaluate = async () => ({ text: 'raw fixture reply', terminal: true });
    await gate.assertRestoredAdapterBaseline({}, MARKER);
  });
});
