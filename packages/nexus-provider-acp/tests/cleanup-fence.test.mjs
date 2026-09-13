import { describe, test } from 'node:test';
import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { realpathSync } from 'node:fs';
import { reapChild } from '../dist/index.js';
import { observeProcessIdentity } from '../dist/index.js';

function resolveNode() {
  const which = execFileSync('which', ['node'], { encoding: 'utf8' }).trim();
  return realpathSync(which);
}

describe('unconfirmed cleanup fencing', () => {
  test('pid mismatch is not confirmed and never signals a reused identity', async () => {
    const child = spawn(resolveNode(), ['-e', 'setInterval(() => {}, 1e6)'], {
      stdio: ['pipe', 'pipe', 'pipe'],
    });
    const identity = observeProcessIdentity(child);
    const mismatched = { ...identity, pid: identity.pid + 99_999 };
    const reap = await reapChild(child, mismatched, 50);
    try {
      child.kill('SIGKILL');
    } catch {
      // ignore
    }
    assert.equal(reap.confirmed, false);
    assert.equal(reap.signal, 'identity_mismatch');
  });
});
