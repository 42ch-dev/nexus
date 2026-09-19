#!/usr/bin/env node
import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import test from 'node:test';
import { terminateChild } from '../scripts/dev.mjs';

class IgnoringChild extends EventEmitter {
  constructor() {
    super();
    this.exitCode = null;
    this.signalCode = null;
    this.reaped = false;
  }

  receiveSignal(signal) {
    if (signal === 'SIGKILL') {
      this.signalCode = signal;
      queueMicrotask(() => {
        this.reaped = true;
        this.emit('exit', null, signal);
      });
    }
  }
}

test('termination escalates after an ignored first signal and waits for reaping', async () => {
  const child = new IgnoringChild();
  const signals = [];

  const terminated = await terminateChild(child, {
    signal: 'SIGTERM',
    timeoutMs: 10,
    sendSignal: (target, signal) => {
      signals.push(signal);
      target.receiveSignal(signal);
    },
  });

  assert.equal(terminated, true);
  assert.deepEqual(signals, ['SIGTERM', 'SIGKILL']);
  assert.equal(child.reaped, true);
  assert.equal(child.signalCode, 'SIGKILL');
});
