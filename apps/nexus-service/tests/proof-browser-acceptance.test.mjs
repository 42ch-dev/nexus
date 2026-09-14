import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { after, describe, test } from 'node:test';

const __dirname = dirname(fileURLToPath(import.meta.url));
const scriptRoot = join(__dirname, '..', 'scripts');

/**
 * Focused regression for the proof orchestrator's acceptance derivations and
 * abrupt-restart group-kill path (`scripts/proof-browser.mjs`). Same extraction
 * contract as `proof-browser-children.test.mjs`: the orchestrator is a
 * dependency-free single-file script, so its REAL helpers are extracted
 * verbatim (stable section anchors) into throwaway modules — no copy of the
 * logic, no mocks of the logic itself.
 *
 * Contracts under test:
 * - a fixed DB/LIFE acceptance row can only derive `pass` from the COMPLETE
 *   locked sample count (DB-1: 100 typed competing-write pairs; LIFE-2/-3: 10
 *   typed cycles) — a single-sample smoke observation must fail the row
 *   (QC1-F-002), and provenance must be complete before any pass (QC1-F-003);
 * - the abrupt-restart group kill goes through the shared ESRCH-only
 *   `signalProcessGroup` path and a survivor sweep that DETECTS a process-group
 *   descendant which outlives the killed leader (QC3-C3) — survivors are
 *   evidence, never a silent pass.
 */
function sliceBetween(source, startMarker, endMarker) {
  const start = source.indexOf(startMarker);
  assert.ok(start >= 0, `proof-browser.mjs is missing anchor: ${startMarker}`);
  const end = source.indexOf(endMarker, start + startMarker.length);
  assert.ok(end > start, `proof-browser.mjs is missing anchor: ${endMarker}`);
  return source.slice(start, end);
}

const source = readFileSync(join(scriptRoot, 'proof-browser.mjs'), 'utf8');

// ── Module 1: pure acceptance derivations + the timing helpers they use ──────

const derivationModulePath = join(mkdtempSync(join(tmpdir(), 'proof-browser-accept-')), 'derivations.mjs');
writeFileSync(
  derivationModulePath,
  [
    sliceBetween(source, 'function nearestRankP95', 'function sha256File'),
    sliceBetween(source, '// ── Acceptance derivations', '// ── Environment provenance'),
    'export {',
    '  DB1_REQUIRED_PAIRS,',
    '  LIFE_REQUIRED_CYCLES,',
    '  deriveDb1Criterion,',
    '  deriveCancelCriterion,',
    '  deriveRestartCriterion,',
    '  deriveProvenanceCompleteness,',
    '};',
  ].join('\n'),
);
const derivations = await import(derivationModulePath);

// ── Module 2: child lifecycle (incl. the shared group-kill path) + ps sweep ──

const lifecycleModulePath = join(mkdtempSync(join(tmpdir(), 'proof-browser-groupkill-')), 'group-kill.mjs');
writeFileSync(
  lifecycleModulePath,
  [
    "import { spawn, execFileSync } from 'node:child_process';",
    'const sleep = (ms) => new Promise((r) => setTimeout(r, ms));',
    'const MAX_EVIDENCE_TAIL_CHARS = 4_000;',
    sliceBetween(source, '// ── Child process lifecycle', '// ── HTTP helpers'),
    sliceBetween(source, 'function psRows', 'async function traceDuringEdit'),
    'export {',
    '  trackedChildren,',
    '  retainedExitedOutput,',
    '  spawnLogged,',
    '  stopChild,',
    '  cleanupChildren,',
    '  signalProcessGroup,',
    '  psRows,',
    '  collectDescendants,',
    '  survivingGroupRows,',
    '};',
  ].join('\n'),
);
const lifecycle = await import(lifecycleModulePath);

const workDirs = [derivationModulePath, lifecycleModulePath].map((p) => join(p, '..'));
after(() => {
  for (const dir of workDirs) rmSync(dir, { recursive: true, force: true });
});

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

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// ── Fixtures: typed sample rows exactly as the phase runners emit them ───────

function typedDb1Pair(index) {
  const expectedVersion = 10 + index;
  return index % 2 === 0
    ? {
        index,
        expectedVersion,
        resultingVersion: expectedVersion + 1,
        winner: 'browser',
        browserStatus: 200,
        cliStatus: 76,
        cliAttempts: 1,
        pairMs: 20 + index,
      }
    : {
        index,
        expectedVersion,
        resultingVersion: expectedVersion + 1,
        winner: 'cli',
        browserStatus: 409,
        cliStatus: 0,
        cliAttempts: 2,
        pairMs: 25 + index,
      };
}

function typedCancelCycle(index, cancelAckToTerminalMs = 900) {
  return {
    index,
    sessionId: `ses-${index}`,
    operationId: `op-${index}`,
    cancel: { status: 'cancelled' },
    cancelAcknowledged: true,
    observedStatus: 'cancelled',
    settled: true,
    cancelAckToTerminalMs,
  };
}

function typedRestartCycle(index, restartReadyMs = 1_200) {
  return {
    index,
    sessionId: `ses-${index}`,
    operationId: `op-${index}`,
    groupKillError: null,
    closedSettled: true,
    abruptCloseSettledMs: 120 + index,
    groupDescendantsBefore: 1,
    groupSurvivors: [],
    restartReadyMs,
    httpStatus: 200,
    observedStatus: 'interrupted',
  };
}

describe('proof-browser locked DB-1 acceptance derivation', () => {
  test('the locked counts are the proof-matrix values', () => {
    assert.equal(derivations.DB1_REQUIRED_PAIRS, 100);
    assert.equal(derivations.LIFE_REQUIRED_CYCLES, 10);
  });

  test('the complete 100-pair typed sample passes and carries the latency distribution', () => {
    const pairs = Array.from({ length: derivations.DB1_REQUIRED_PAIRS }, (_, i) => typedDb1Pair(i));
    const verdict = derivations.deriveDb1Criterion(pairs);
    assert.equal(verdict.pass, true);
    assert.equal(verdict.sampleCount, 100);
    assert.equal(verdict.typedPairCount, 100);
    assert.equal(verdict.pairMs.samples.length, 100);
    assert.equal(verdict.pairMs.max, Math.max(...verdict.pairMs.samples));
    // Nearest-rank p95 (proof matrix §1): rank ceil(0.95*100)=95 of the sorted
    // fixture distribution — for n=100 this is NOT the max (that is rank 100).
    const sorted = [...verdict.pairMs.samples].sort((a, b) => a - b);
    assert.equal(verdict.pairMs.p95, sorted[Math.ceil(0.95 * sorted.length) - 1]);
  });

  test('a single-pair smoke sample can never pass the fixed row (regression)', () => {
    const verdict = derivations.deriveDb1Criterion([typedDb1Pair(0)]);
    assert.equal(verdict.sampleCount, 1);
    assert.equal(verdict.pass, false, 'one CAS race must not label DB-1 pass');
  });

  test('an incomplete sample is recorded, not green', () => {
    const pairs = Array.from({ length: 99 }, (_, i) => typedDb1Pair(i));
    const verdict = derivations.deriveDb1Criterion(pairs);
    assert.equal(verdict.sampleCount, 99);
    assert.equal(verdict.typedPairCount, 99);
    assert.equal(verdict.pass, false);
  });

  test('one malformed round fails the otherwise complete sample', () => {
    const pairs = Array.from({ length: 99 }, (_, i) => typedDb1Pair(i));
    pairs.push({
      index: 99,
      expectedVersion: 109,
      resultingVersion: null,
      winner: 'both',
      browserStatus: 200,
      cliStatus: 0,
      cliAttempts: 1,
      pairMs: 30,
    });
    const verdict = derivations.deriveDb1Criterion(pairs);
    assert.equal(verdict.typedPairCount, 99);
    assert.equal(verdict.pass, false);
  });

  test('a lost commit (version skip) is not a CAS result', () => {
    const pairs = Array.from({ length: 99 }, (_, i) => typedDb1Pair(i));
    pairs.push({ ...typedDb1Pair(99), resultingVersion: 110 + 2 });
    const verdict = derivations.deriveDb1Criterion(pairs);
    assert.equal(verdict.typedPairCount, 99);
    assert.equal(verdict.pass, false);
  });

  test('an untyped loser shape (busy after deadline, not a conflict) fails the row', () => {
    const pairs = Array.from({ length: 99 }, (_, i) => typedDb1Pair(i));
    pairs.push({ ...typedDb1Pair(99), cliStatus: 1 });
    const verdict = derivations.deriveDb1Criterion(pairs);
    assert.equal(verdict.pass, false);
  });
});

describe('proof-browser locked LIFE-2 cancel derivation', () => {
  test('the complete 10-cycle typed sample passes with the ack→terminal distribution', () => {
    const cycles = Array.from({ length: 10 }, (_, i) => typedCancelCycle(i, 500 + i * 100));
    const verdict = derivations.deriveCancelCriterion(cycles);
    assert.equal(verdict.pass, true);
    assert.equal(verdict.sampleCount, 10);
    assert.equal(verdict.typedCycleCount, 10);
    assert.equal(verdict.ackToTerminalMs.samples.length, 10);
    assert.equal(verdict.ackToTerminalMs.max, 1_400);
    assert.equal(verdict.ackToTerminalMs.p95, 1_400);
  });

  test('nine cycles are incomplete, never green', () => {
    const verdict = derivations.deriveCancelCriterion(
      Array.from({ length: 9 }, (_, i) => typedCancelCycle(i)),
    );
    assert.equal(verdict.pass, false);
    assert.equal(verdict.sampleCount, 9);
  });

  test('one cycle that never settles to cancelled fails the complete sample', () => {
    const cycles = Array.from({ length: 10 }, (_, i) => typedCancelCycle(i));
    cycles[4] = { ...cycles[4], settled: false, observedStatus: 'running', cancelAcknowledged: true };
    const verdict = derivations.deriveCancelCriterion(cycles);
    assert.equal(verdict.typedCycleCount, 9);
    assert.equal(verdict.pass, false);
  });

  test('the matrix latency envelope gates the verdict (p95 ≤2 s, max ≤3 s)', () => {
    const atLimit = Array.from({ length: 10 }, (_, i) => typedCancelCycle(i, 2_000));
    assert.equal(derivations.deriveCancelCriterion(atLimit).pass, true);

    const overP95 = Array.from({ length: 10 }, (_, i) => typedCancelCycle(i, 2_100));
    assert.equal(derivations.deriveCancelCriterion(overP95).pass, false);

    const mixed = Array.from({ length: 10 }, (_, i) => typedCancelCycle(i, 500));
    mixed[7] = { ...mixed[7], cancelAckToTerminalMs: 3_100 };
    const mixedVerdict = derivations.deriveCancelCriterion(mixed);
    assert.equal(mixedVerdict.ackToTerminalMs.max, 3_100);
    assert.equal(mixedVerdict.pass, false);
  });
});

describe('proof-browser locked LIFE-3 restart derivation', () => {
  test('the complete 10-cycle typed sample passes with the restart-ready distribution', () => {
    const cycles = Array.from({ length: 10 }, (_, i) => typedRestartCycle(i, 1_000 + i * 100));
    const verdict = derivations.deriveRestartCriterion(cycles);
    assert.equal(verdict.pass, true);
    assert.equal(verdict.sampleCount, 10);
    assert.equal(verdict.typedCycleCount, 10);
    assert.equal(verdict.restartReadyMs.samples.length, 10);
    assert.equal(verdict.restartReadyMs.max, 1_900);
  });

  test('four cycles are incomplete, never green', () => {
    const verdict = derivations.deriveRestartCriterion(
      Array.from({ length: 4 }, (_, i) => typedRestartCycle(i)),
    );
    assert.equal(verdict.pass, false);
    assert.equal(verdict.sampleCount, 4);
  });

  test('a surviving group descendant fails the row (QC3-C3 regression)', () => {
    const cycles = Array.from({ length: 10 }, (_, i) => typedRestartCycle(i));
    cycles[3] = {
      ...cycles[3],
      groupSurvivors: [{ pid: 4711, ppid: 1, command: 'python3 mock_acp_workflow.py' }],
    };
    const verdict = derivations.deriveRestartCriterion(cycles);
    assert.equal(verdict.typedCycleCount, 9);
    assert.equal(verdict.pass, false);
  });

  test('an unsettled close, a non-200 observation, and a wrong status each fail the row', () => {
    const unsettled = Array.from({ length: 10 }, (_, i) => typedRestartCycle(i));
    unsettled[2] = { ...unsettled[2], closedSettled: false, abruptCloseSettledMs: null };
    assert.equal(derivations.deriveRestartCriterion(unsettled).pass, false);

    const staleHttp = Array.from({ length: 10 }, (_, i) => typedRestartCycle(i));
    staleHttp[6] = { ...staleHttp[6], httpStatus: 404 };
    assert.equal(derivations.deriveRestartCriterion(staleHttp).pass, false);

    const running = Array.from({ length: 10 }, (_, i) => typedRestartCycle(i));
    running[1] = { ...running[1], observedStatus: 'running' };
    assert.equal(derivations.deriveRestartCriterion(running).pass, false);
  });

  test('the matrix restart-ready bound (≤5 s) gates the verdict', () => {
    const atLimit = Array.from({ length: 10 }, (_, i) => typedRestartCycle(i, 5_000));
    assert.equal(derivations.deriveRestartCriterion(atLimit).pass, true);
    const overLimit = Array.from({ length: 10 }, (_, i) => typedRestartCycle(i, 5_100));
    assert.equal(derivations.deriveRestartCriterion(overLimit).pass, false);
  });
});

describe('proof-browser provenance completeness gate', () => {
  const SHA = 'a'.repeat(64);
  function completeProvenance() {
    return {
      source: { sha: SHA, clean: true, dirtyFileCount: 0 },
      platform: {
        os: 'Darwin 25.6.0',
        arch: 'arm64',
        libc: 'libSystem (darwin)',
        cpuModel: 'Apple M4',
        cpuCount: 10,
        totalMemBytes: 17_179_869_184,
      },
      runtime: { node: 'v24.20.0', chromium: 'Chrome/149.0.0.0', rustc: 'rustc 1.90.0', linker: 'Apple clang version 17' },
      contractHash: 'b'.repeat(64),
      dataset: {
        seedTool: 'cargo run -q -p nexus-core-node --bin native-wire-fixture-seed -- <fixture-home>',
        dbSha256: 'c'.repeat(64),
        dbBytes: 123_456,
        entityCount: 2,
        candidatesObserved: 2,
      },
    };
  }

  test('a fully observed provenance is complete', () => {
    assert.deepEqual(derivations.deriveProvenanceCompleteness(completeProvenance()), {
      complete: true,
      missing: [],
    });
  });

  test('every required fact that cannot be observed blocks acceptance', () => {
    const cases = [
      ['source', { sha: null, clean: true, dirtyFileCount: 0 }, 'source.sha'],
      ['source', { sha: 'abc', clean: true, dirtyFileCount: 0 }, 'source.sha'],
      ['source', { sha: SHA, clean: null, dirtyFileCount: null }, 'source.treeState'],
      ['platform', { os: 'Darwin 25.6.0', arch: 'arm64', libc: null, cpuModel: 'Apple M4', cpuCount: 10, totalMemBytes: 1 }, 'platform.libc'],
      ['runtime', { node: 'v24.20.0', chromium: null, rustc: 'rustc 1.90.0', linker: 'ld64' }, 'runtime.chromium'],
      ['runtime', { node: 'v24.20.0', chromium: 'Chrome/149', rustc: null, linker: 'ld64' }, 'runtime.rustc'],
      ['contractHash', null, 'contractHash'],
      ['dataset', { seedTool: '', dbSha256: 'c'.repeat(64), dbBytes: 1, entityCount: 2, candidatesObserved: 2 }, 'dataset.seedTool'],
      ['dataset', { seedTool: 'seed', dbSha256: 'c'.repeat(64), dbBytes: 1, entityCount: null, candidatesObserved: 2 }, 'dataset.entityCount'],
      ['dataset', { seedTool: 'seed', dbSha256: 'c'.repeat(64), dbBytes: 1, entityCount: 2, candidatesObserved: null }, 'dataset.candidatesObserved'],
      ['dataset', { seedTool: 'seed', dbSha256: null, dbBytes: 1, entityCount: 2, candidatesObserved: 2 }, 'dataset.dbSha256'],
    ];
    for (const [section, patch, expectedMissing] of cases) {
      const provenance = completeProvenance();
      if (section === 'contractHash') provenance.contractHash = patch;
      else if (section === 'dataset') provenance.dataset = patch;
      else provenance[section] = patch;
      const verdict = derivations.deriveProvenanceCompleteness(provenance);
      assert.equal(verdict.complete, false, `${expectedMissing} must block acceptance`);
      assert.ok(verdict.missing.includes(expectedMissing), `${expectedMissing} missing from ${JSON.stringify(verdict.missing)}`);
    }
  });

  test('an absent provenance object cannot pass', () => {
    const verdict = derivations.deriveProvenanceCompleteness(undefined);
    assert.equal(verdict.complete, false);
    assert.ok(verdict.missing.includes('source.sha'));
  });
});

describe('proof-browser abrupt-restart group kill (QC3-C3)', () => {
  test('a parked group descendant is DETECTED before the kill and gone after the shared group kill', async () => {
    // Leader spawns a long-lived grandchild on inherited stdio (so `close`
    // stays pending), announces readiness, then exits — the exact abrupt-kill
    // shape. The grandchild outlives the leader.
    const leaderScript = `
const { spawn } = require('node:child_process');
spawn(process.execPath, ['-e', 'process.stdout.write("grandchild-ready-marker"); setInterval(() => {}, 1_000);'], { stdio: ['ignore', 'inherit', 'inherit'] });
process.stdout.write('leader-ready-marker');
setTimeout(() => process.exit(0), 100);
`;
    const child = lifecycle.spawnLogged(process.execPath, ['-e', leaderScript], {
      label: 'nexus-service',
    });
    await waitForChildOutput(child, 'grandchild-ready-marker');

    // Snapshot the tracked tree while the leader is still alive: after the
    // leader dies, orphans are re-parented and a fresh ps walk could no
    // longer find them.
    const groupBefore = lifecycle
      .collectDescendants([child.pid], lifecycle.psRows())
      .filter((row) => row.pid !== child.pid);
    assert.ok(
      groupBefore.length >= 1,
      'the parked grandchild must be visible in the process tree before the kill',
    );

    await new Promise((resolveExit) => child.once('exit', resolveExit));

    // The survivor sweep must SEE the descendant — this is the observation the
    // old unconditional pid fallback silently skipped.
    const survivorsBefore = lifecycle.survivingGroupRows(groupBefore, child.pid);
    assert.equal(survivorsBefore.length, 1, 'the parked grandchild must be reported as a survivor');
    assert.equal(survivorsBefore[0].pid, groupBefore[0].pid);

    // Shared group-kill path: SIGKILL the whole group, verify nobody survives.
    lifecycle.signalProcessGroup(child.pid, 'SIGKILL');
    await Promise.race([child.__closed, sleep(2_000)]);
    assert.equal(child.__closeSettled, true, 'the killed leader must settle through close');
    assert.equal(
      lifecycle.survivingGroupRows(groupBefore, child.pid).length,
      0,
      'no group descendant may survive the shared group kill',
    );
  });

  test('re-signaling a fully dead group does not throw (ESRCH-only fallback)', async () => {
    const child = lifecycle.spawnLogged(
      process.execPath,
      ['-e', 'process.stdout.write("plain-marker"); setInterval(() => {}, 1_000);'],
      { label: 'nexus-service' },
    );
    await waitForChildOutput(child, 'plain-marker');
    lifecycle.signalProcessGroup(child.pid, 'SIGKILL');
    await Promise.race([child.__closed, sleep(2_000)]);
    assert.equal(child.__closeSettled, true);
    // The group no longer exists: the group signal hits ESRCH, the fallback
    // hits ESRCH on the dead leader, and the helper stays silent — a missing
    // group is a settled state, not an error.
    lifecycle.signalProcessGroup(child.pid, 'SIGKILL');
  });
});
