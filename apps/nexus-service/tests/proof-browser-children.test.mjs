import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { after, describe, test } from 'node:test';

const __dirname = dirname(fileURLToPath(import.meta.url));
const scriptRoot = join(__dirname, '..', 'scripts');

/**
 * Focused regression for the proof orchestrator's exited-child bookkeeping
 * (`scripts/proof-browser.mjs`). The orchestrator is a dependency-free
 * single-file script, so these tests extract its real child-lifecycle and
 * evidence helpers verbatim (stable section anchors) into a throwaway module
 * and drive them against real child processes — no copy of the logic, no
 * mocks.
 *
 * Contract under test: a child that emits `exit` while its piped stdio is
 * still open must stay in the set scanned by `drainExitedChildrenForEvidence`
 * until its `close` callback captures the bounded retained output — bytes the
 * child (or its process-group descendants) write after `exit` must land in
 * the failure evidence.
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
  'export { trackedChildren, retainedExitedOutput, trackChild, drainExitedChildrenForEvidence, spawnLogged, cleanupChildren, collectChildOutput, redactChildOutput, MAX_EVIDENCE_TAIL_CHARS, RETAINED_EXITED_OUTPUT_MAX };',
].join('\n');

const workDir = mkdtempSync(join(tmpdir(), 'proof-browser-children-'));
const modulePath = join(workDir, 'child-lifecycle.mjs');
writeFileSync(modulePath, moduleSource);
const lifecycle = await import(modulePath);

after(() => {
  rmSync(workDir, { recursive: true, force: true });
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
