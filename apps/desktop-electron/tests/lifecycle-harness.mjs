#!/usr/bin/env node
import assert from 'node:assert/strict';
import test from 'node:test';
import {
  attachExistingReadiness,
  joinExitConfirmed,
  mergeUtilityReadyLifecycle,
  remainingCloseBudget,
  resolveCloseLifecycleAfterJoin,
  resolveOpenProofPolicy,
  shouldTreatUtilityExitAsUnexpected,
} from '../dist/lifecycle-coord.js';
import { PullReservationLedger } from '../dist/utility-admission.js';

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

test('PullReservationLedger reserves at admission and releases on settle', () => {
  const ledger = new PullReservationLedger();
  const pull = {
    request_id: 'r1',
    operation: 'pull',
    payload: { operation_id: 'op-a' },
  };
  assert.equal(ledger.tryReserve(pull), true);
  assert.equal(ledger.tryReserve({ ...pull, request_id: 'r2' }), false);
  ledger.release(pull);
  assert.equal(ledger.tryReserve({ ...pull, request_id: 'r3' }), true);
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
