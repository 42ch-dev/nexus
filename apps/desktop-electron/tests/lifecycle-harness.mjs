#!/usr/bin/env node
import assert from 'node:assert/strict';
import test from 'node:test';
import {
  joinExitConfirmed,
  remainingCloseBudget,
  resolveOpenProofPolicy,
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
