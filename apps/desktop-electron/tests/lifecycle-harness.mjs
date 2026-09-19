#!/usr/bin/env node
/**
 * Lifecycle coordination harness (P0-T4 helpers).
 *
 * The proof shell's renderer-reattach/open-policy helpers
 * (resolveOpenProofPolicy and friends) were retired with the proof main that
 * imported them (P0-T7); what remains is the shared crash-backoff schedule,
 * the generation/stale-frame predicates and the owner admission ledger.
 */
import assert from 'node:assert/strict';
import test from 'node:test';
import {
  CRASH_BACKOFF_DELAYS_MS,
  crashBackoffDelayMs,
  isConfirmedCloseReport,
  isStaleGeneration,
} from '../dist/lifecycle-coord.js';
import { admitUtilityOperation } from '../dist/utility-admission.js';

test('crash backoff is the frozen 500/1000/2000/4000/8000 schedule, then exhausted', () => {
  assert.deepEqual(CRASH_BACKOFF_DELAYS_MS, [500, 1_000, 2_000, 4_000, 8_000]);
  assert.deepEqual(
    [1, 2, 3, 4, 5].map((attempt) => crashBackoffDelayMs(attempt)),
    [500, 1_000, 2_000, 4_000, 8_000],
  );
  assert.equal(crashBackoffDelayMs(6), null, 'five attempts only');
  assert.equal(crashBackoffDelayMs(0), null);
  assert.equal(crashBackoffDelayMs(1.5), null);
});

test('stale owner generations are ignored', () => {
  assert.equal(isStaleGeneration(3, 3), false);
  assert.equal(isStaleGeneration(2, 3), true);
  assert.equal(isStaleGeneration(4, 3), true);
});

test('only a closed, cleanup-confirmed report is close success', () => {
  assert.equal(isConfirmedCloseReport({ state: 'closed', cleanup_confirmed: true, pending_operations: [] }), true);
  assert.equal(isConfirmedCloseReport({ state: 'closed', cleanup_confirmed: false, pending_operations: [] }), false);
  assert.equal(isConfirmedCloseReport({ state: 'interrupted', cleanup_confirmed: true, pending_operations: [] }), false);
  assert.equal(isConfirmedCloseReport({ state: 'interrupted', cleanup_confirmed: false, pending_operations: ['op'] }), false);
  assert.equal(isConfirmedCloseReport(null), false);
  assert.equal(isConfirmedCloseReport(undefined), false);
  assert.equal(isConfirmedCloseReport('closed'), false);
  assert.equal(isConfirmedCloseReport({}), false);
});

test('service owner admission refuses work that races a live or unconfirmed owner', () => {
  assert.deepEqual(admitUtilityOperation('start', 'none'), { ok: true });
  assert.deepEqual(admitUtilityOperation('start', 'closed'), { ok: true });
  assert.deepEqual(admitUtilityOperation('start', 'running'), {
    ok: false,
    code: 'owner_busy',
    message: 'a service is already running in this owner',
  });
  assert.deepEqual(admitUtilityOperation('start', 'closing'), {
    ok: false,
    code: 'busy',
    message: 'a service close is still in flight',
  });
  assert.deepEqual(admitUtilityOperation('reset-local-state', 'none'), { ok: true });
  assert.deepEqual(admitUtilityOperation('reset-local-state', 'closed'), { ok: true });
  assert.deepEqual(admitUtilityOperation('reset-local-state', 'running'), {
    ok: false,
    code: 'busy',
    message: 'local-state reset requires the service to be closed first',
  });
  assert.deepEqual(admitUtilityOperation('reset-local-state', 'closing'), {
    ok: false,
    code: 'busy',
    message: 'local-state reset requires the service to be closed first',
  });
  // An unconfirmed close retains the owner: only a close retry may proceed.
  assert.equal(admitUtilityOperation('start', 'unconfirmed').code, 'interrupted');
  assert.equal(admitUtilityOperation('reset-local-state', 'unconfirmed').code, 'interrupted');
  assert.deepEqual(admitUtilityOperation('close', 'unconfirmed'), { ok: true });
  assert.deepEqual(admitUtilityOperation('close', 'none'), { ok: true });
  assert.deepEqual(admitUtilityOperation('close', 'running'), { ok: true });
});
