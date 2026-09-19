import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  nearestRankP95,
  evaluateDx2,
  evaluateDx3,
  isCargoFamilyCommand,
  isTauriFamilyCommand,
  countCargoTraces,
  evidenceFilename,
  markerSource,
  markerProbeUrl,
  parseArgs,
  requireOutDir,
  SURFACE_CONFIG,
  summarizeIntervalSamples,
  getDx1RootPids,
  parseViteOriginFromChunk,
  isPidInForest,
  buildFailurePayload,
  writeEvidence,
  collectProcessTreePids,
  measureColdLaunchSample,
  recordFailureAfterCleanup,
  assertEndpointOwnedByForest,
} from './proof-rft-dx.mjs';

test('nearestRankP95 uses nearest-rank p95', () => {
  const samples = [10, 20, 30, 40, 50, 60, 70, 80, 90, 100];
  assert.equal(nearestRankP95(samples), 100);
  assert.equal(nearestRankP95([1, 2, 3, 4]), 4);
});

test('evaluateDx2 and evaluateDx3 thresholds', () => {
  assert.equal(evaluateDx2([100, 200, 300]).pass, true);
  assert.equal(evaluateDx2([100, 2000, 300]).pass, false);
  assert.equal(evaluateDx3([1000, 4000, 3000]).pass, true);
  assert.equal(evaluateDx3([6000]).pass, false);
});

test('cargo and tauri family detection', () => {
  assert.equal(isCargoFamilyCommand('/usr/bin/cargo build -p nexus42'), true);
  assert.equal(isCargoFamilyCommand('node scripts/proof-rft-dx.mjs'), false);
  assert.equal(isTauriFamilyCommand('/path/tauri dev'), true);
});

test('countCargoTraces walks descendant process tree', () => {
  const ps = `100 1 node pnpm dev\n101 100 node vite\n102 101 /usr/bin/cargo build\n103 100 node helper`;
  const trace = countCargoTraces(100, ps);
  assert.equal(trace.cargoCount, 1);
  assert.match(trace.cargoCommands[0], /cargo build/);
});

test('markerSource and markerProbeUrl', () => {
  assert.match(markerSource(3), /sample-3/);
  const url = markerProbeUrl('http://127.0.0.1:5173', SURFACE_CONFIG.web, '/repo');
  assert.equal(url, 'http://127.0.0.1:5173/src/proof-rft-dx-marker.ts');
});

test('parseArgs reads surface and sample flags', () => {
  const args = parseArgs(['--surface', 'web', '--samples', '5', '--cold-samples', '2', '--port', '18420', '--out', '/tmp/out']);
  assert.equal(args.surface, 'web');
  assert.equal(args.samples, 5);
  assert.equal(args.coldSamples, 2);
  assert.equal(args.port, 18420);
});

test('evidenceFilename encodes pass/fail without overwriting semantics', () => {
  const name = evidenceFilename({ runKind: 'web-stable-loop', pass: false, startedAt: '2026-09-13T10:00:00.000Z' });
  assert.match(name, /fail\.json$/);
});

test('requireOutDir rejects missing --out for all modes', () => {
  assert.throws(() => requireOutDir({ out: null }), /--out <dir> is required/);
  assert.equal(requireOutDir({ out: '/tmp/evidence' }), '/tmp/evidence');
});

test('retired desktop-web surface is not offered by the runner', () => {
  assert.equal(SURFACE_CONFIG['desktop-web'], undefined);
  assert.deepEqual(Object.keys(SURFACE_CONFIG), ['web', 'studio', 'shared-ui']);
});

test('shared-ui DX-1 roots include watcher and vite child', () => {
  const roots = getDx1RootPids({
    viteChild: { pid: 200 },
    watcherChild: { pid: 150 },
    config: SURFACE_CONFIG['shared-ui'],
  });
  assert.deepEqual(roots, [200, 150]);
});

test('summarizeIntervalSamples detects cargo across interval observations', () => {
  const summary = summarizeIntervalSamples([
    { processCount: 2, cargoCount: 0, tauriCount: 0, cargoCommands: [], tauriCommands: [], tracedPids: [1, 2] },
    { processCount: 3, cargoCount: 1, tauriCount: 0, cargoCommands: ['/usr/bin/cargo build'], tauriCommands: [], tracedPids: [1, 2, 3] },
  ]);
  assert.equal(summary.anyCargo, true);
  assert.equal(summary.cargoPositiveSamples, 1);
  assert.equal(summary.cargoCount, 1);
});

test('isPidInForest accepts descendants only', () => {
  const ps = `10 1 pnpm dev\n11 10 node vite\n99 1 foreign-server`;
  assert.equal(isPidInForest(11, [10], ps), true);
  assert.equal(isPidInForest(99, [10], ps), false);
});

test('parseViteOriginFromChunk captures Local origin', () => {
  const origin = parseViteOriginFromChunk('\n  ➜  Local:   http://127.0.0.1:5173/\n');
  assert.equal(origin, 'http://127.0.0.1:5173');
});

test('buildFailurePayload preserves error and cleanup outcome', () => {
  const payload = buildFailurePayload({
    runKind: 'web-stable-loop',
    startedAt: '2026-09-13T10:00:00.000Z',
    command: 'node scripts/proof-rft-dx.mjs',
    environment: { sourceSha: 'abc' },
    error: new Error('boom'),
    cleanupOutcome: { restoredMarker: true },
    partial: { surface: 'web' },
  });
  assert.equal(payload.pass, false);
  assert.equal(payload.error, 'boom');
  assert.equal(payload.cleanupOutcome.restoredMarker, true);
  assert.equal(payload.surface, 'web');
});

test('writeEvidence records failure JSON without overwriting', async () => {
  const outDir = await mkdtemp(join(tmpdir(), 'proof-rft-dx-'));
  const payload = buildFailurePayload({
    runKind: 'inject-fail-demo',
    startedAt: '2026-09-13T12:00:00.000Z',
    command: 'node scripts/proof-rft-dx.mjs --inject-fail',
    environment: { sourceSha: 'deadbeef' },
    error: new Error('Injected failure for evidence path verification'),
    cleanupOutcome: { childrenStopped: true },
  });
  const first = await writeEvidence(outDir, payload);
  const saved = JSON.parse(await readFile(first, 'utf8'));
  assert.equal(saved.pass, false);
  assert.match(saved.error, /Injected failure/);
  await assert.rejects(() => writeEvidence(outDir, payload), /Evidence file already exists/);
  await rm(outDir, { recursive: true, force: true });
});

test('recordDaemonOwnership marks first ensure start as runner-owned', async () => {
  const { recordDaemonOwnership } = await import('./proof-rft-dx.mjs');
  assert.deepEqual(recordDaemonOwnership(false), { startedByRunner: true, wasRunning: false });
  assert.deepEqual(recordDaemonOwnership(true), { startedByRunner: false, wasRunning: true });
});


test('collectProcessTreePids includes descendants', () => {
  const ps = `100 1 pnpm dev\n101 100 node vite\n102 101 esbuild\n103 50 foreign`;
  const pids = collectProcessTreePids(100, ps);
  assert.deepEqual(pids.sort((a, b) => a - b), [100, 101, 102]);
});

const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));

test('measureColdLaunchSample starts clock at spawn and includes origin resolve in elapsed', async () => {
  const order = [];
  const result = await measureColdLaunchSample({
    spawnChild: () => {
      order.push('spawn');
      return { pid: 42 };
    },
    resolveOrigin: async () => {
      order.push('resolve');
      await sleep(60);
      return 'http://127.0.0.1:5173';
    },
    waitForServed: async () => {
      order.push('served');
      await sleep(10);
    },
  });
  assert.deepEqual(order, ['spawn', 'resolve', 'served']);
  assert.ok(result.originResolveMs >= 55, 'origin resolve is recorded separately');
  assert.ok(result.elapsedMs >= result.originResolveMs, 'elapsed is not post-listen only');
  assert.ok(result.elapsedMs >= 65, 'elapsed includes spawn through served-page, not listener-to-page only');
});

test('recordFailureAfterCleanup records cleanup outcome after cleanup runs', async () => {
  const outDir = await mkdtemp(join(tmpdir(), 'proof-rft-dx-cleanup-'));
  const evidencePath = await recordFailureAfterCleanup({
    outDir,
    cleanup: async () => ({ tempDirRemoved: true }),
    buildPayload: cleanupOutcome =>
      buildFailurePayload({
        runKind: 'cleanup-order-demo',
        startedAt: '2026-09-13T12:00:00.000Z',
        command: 'node scripts/proof-rft-dx.mjs --inject-fail',
        environment: { sourceSha: 'deadbeef' },
        error: new Error('boom'),
        cleanupOutcome,
      }),
  });
  assert.ok(evidencePath);
  const saved = JSON.parse(await readFile(evidencePath, 'utf8'));
  assert.equal(saved.cleanupOutcome.tempDirRemoved, true);
  await rm(outDir, { recursive: true, force: true });
});

test('assertEndpointOwnedByForest refuses foreign listeners', async () => {
  await assert.rejects(
    () => assertEndpointOwnedByForest(5173, [100], { listenerPid: 200, psOutput: '200 1 foreign\n' }),
    /foreign PID/,
  );
});