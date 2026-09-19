#!/usr/bin/env node
import assert from 'node:assert/strict';
import test from 'node:test';
import {
  CRASH_BACKOFF_DELAYS_MS,
  attachExistingReadiness,
  crashBackoffDelayMs,
  isConfirmedCloseReport,
  isStaleGeneration,
  joinExitConfirmed,
  mainStateOpenPolicyAfterRendererCrash,
  mergeUtilityReadyLifecycle,
  remainingCloseBudget,
  rendererDetachedAfterCreateWindow,
  resolveCloseLifecycleAfterJoin,
  resolveOpenProofPolicy,
  shouldTreatUtilityExitAsUnexpected,
} from '../dist/lifecycle-coord.js';
import { admitUtilityOperation } from '../dist/utility-admission.js';

test('remainingCloseBudget never exceeds absolute deadline', () => {
  const deadline = 10_000;
  assert.equal(remainingCloseBudget(deadline, 9_000), 1_000);
  assert.equal(remainingCloseBudget(deadline, 10_500), 0);
});

test('resolveOpenProofPolicy attaches replacement window to live owner', () => {
  assert.equal(
    resolveOpenProofPolicy({
      reopenRequired: false,
      rendererDetached: true,
      phase: 'open',
      ownerAlive: true,
    }),
    'attach_existing_owner',
  );
});

test('starting phase renderer crash uses attach-existing policy', () => {
  assert.equal(
    resolveOpenProofPolicy({
      reopenRequired: false,
      rendererDetached: true,
      phase: 'starting',
      ownerAlive: true,
    }),
    'attach_existing_owner',
  );
  assert.equal(attachExistingReadiness('starting'), 'starting');
});

test('normal flow: compatibility spawn then open dispatches utility open', () => {
  const base = {
    reopenRequired: false,
    rendererDetached: false,
    ownerAlive: true,
  };

  // compatibility spawns utility; utility-ready leaves owner alive in starting/idle
  assert.equal(
    resolveOpenProofPolicy({ ...base, phase: 'starting' }),
    'open_utility',
  );
  assert.equal(
    resolveOpenProofPolicy({ ...base, phase: 'idle' }),
    'open_utility',
  );

  // same owner alive+starting without detached renderer must not attach
  assert.notEqual(
    resolveOpenProofPolicy({ ...base, phase: 'starting' }),
    'attach_existing_owner',
  );
});


test('initial bootstrap createWindow clears replacement marker', () => {
  assert.equal(
    rendererDetachedAfterCreateWindow({ initialBootstrap: true, rendererDetached: true }),
    false,
  );
});

test('replacement createWindow preserves crash marker until attach/open settlement', () => {
  assert.equal(
    rendererDetachedAfterCreateWindow({ initialBootstrap: false, rendererDetached: true }),
    true,
  );
});

test('main-state: crash → replacement createWindow → open uses attach policy', () => {
  const outcome = mainStateOpenPolicyAfterRendererCrash({
    phase: 'starting',
    ownerAlive: true,
    initialBootstrap: false,
  });
  assert.equal(outcome.rendererDetachedAfterCrash, true);
  assert.equal(outcome.rendererDetachedAfterCreateWindow, true);
  assert.equal(outcome.openPolicy, 'attach_existing_owner');
});

test('main-state: initial window never inherits stale detached marker', () => {
  const outcome = mainStateOpenPolicyAfterRendererCrash({
    phase: 'idle',
    ownerAlive: false,
    initialBootstrap: true,
  });
  assert.equal(outcome.rendererDetachedAfterCreateWindow, false);
  assert.equal(outcome.openPolicy, 'open_utility');
});

test('detached replacement after starting crash attaches existing owner', () => {
  assert.equal(
    resolveOpenProofPolicy({
      reopenRequired: false,
      rendererDetached: true,
      phase: 'starting',
      ownerAlive: true,
    }),
    'attach_existing_owner',
  );
});

test('mergeUtilityReadyLifecycle keeps starting until utility reports open', () => {
  assert.deepEqual(
    mergeUtilityReadyLifecycle({ phase: 'starting', owner_alive: true }, { phase: 'starting', owner_alive: true }),
    { phase: 'starting', owner_alive: true },
  );
  assert.deepEqual(
    mergeUtilityReadyLifecycle({ phase: 'starting', owner_alive: true }, { phase: 'open', owner_alive: true }),
    { phase: 'open', owner_alive: true },
  );
});

test('resolveOpenProofPolicy requires fence before reopen', () => {
  assert.equal(
    resolveOpenProofPolicy({
      reopenRequired: true,
      rendererDetached: false,
      phase: 'interrupted',
      ownerAlive: false,
    }),
    'reopen_after_fence',
  );
});

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

test('joinExitConfirmed blocks reopen when kill join is unconfirmed', () => {
  assert.equal(joinExitConfirmed(false), false);
  assert.equal(joinExitConfirmed(true), true);
});

test('closeInitiated is per-generation — new owner exit is not suppressed', () => {
  assert.equal(shouldTreatUtilityExitAsUnexpected(null, 2), true);
  assert.equal(shouldTreatUtilityExitAsUnexpected(1, 2), true);
  assert.equal(shouldTreatUtilityExitAsUnexpected(2, 2), false);
});

test('unconfirmed close join must not report closed', () => {
  const outcome = resolveCloseLifecycleAfterJoin({
    confirmed: false,
    closeOk: true,
    ownerStillReferenced: true,
  });
  assert.equal(outcome.phase, 'interrupted');
  assert.equal(outcome.reopenRequired, true);
  assert.equal(outcome.owner_alive, true);
});

test('close then new owner unexpected exit is treated as interrupted', () => {
  const priorCloseGeneration = 1;
  const newOwnerGeneration = 2;
  assert.equal(
    shouldTreatUtilityExitAsUnexpected(priorCloseGeneration, newOwnerGeneration),
    true,
  );
});
