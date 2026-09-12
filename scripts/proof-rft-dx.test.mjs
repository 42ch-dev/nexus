import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  nearestRankP95,
  maxSample,
  evaluateDx2,
  evaluateDx3,
  isCargoFamilyCommand,
  isTauriFamilyCommand,
  parsePsLines,
  countCargoTraces,
  evidenceFilename,
  markerSource,
  markerProbeUrl,
  parseArgs,
  SURFACE_CONFIG,
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
