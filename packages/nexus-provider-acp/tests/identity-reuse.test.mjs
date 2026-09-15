import { describe, test } from 'node:test';
import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { realpathSync } from 'node:fs';
import {
  observeProcessIdentity,
  queryOsProcessIdentity,
  reapChild,
} from '../dist/index.js';

function resolveNode() {
  return realpathSync(execFileSync('which', ['node'], { encoding: 'utf8' }).trim());
}

describe('real OS identity fencing', () => {
  test('mismatched birth prevents signal and reap stays unconfirmed', async () => {
    const child = spawn(resolveNode(), ['-e', 'setInterval(()=>{},1e6)'], {
      stdio: ['pipe', 'pipe', 'pipe'],
    });
    const identity = observeProcessIdentity(child);
    const stale = {
      pid: identity.pid,
      process_birth: 'stale-birth-token',
      group_id: identity.group_id,
    };
    const reap = await reapChild(child, stale, 200);
    assert.equal(reap.confirmed, false);
    assert.equal(reap.signal, 'identity_mismatch');
    const live = queryOsProcessIdentity(identity.pid);
    assert.ok(live);
    assert.notEqual(live.process_birth, stale.process_birth);
    try {
      child.kill('SIGKILL');
    } catch {
      // ignore
    }
  });

  test('observed identity uses OS birth and pgid not synthetic pid=group', () => {
    const child = spawn(resolveNode(), ['-e', 'setInterval(()=>{},1e6)'], {
      stdio: ['pipe', 'pipe', 'pipe'],
    });
    const identity = observeProcessIdentity(child);
    const os = queryOsProcessIdentity(identity.pid);
    assert.deepEqual(identity, os);
    assert.ok(identity.process_birth && identity.process_birth.length > 0);
    assert.ok(identity.group_id && identity.group_id.length > 0);
    const second = queryOsProcessIdentity(identity.pid);
    assert.deepEqual(second, identity);
    try {
      child.kill('SIGKILL');
    } catch {
      // ignore
    }
  });
});
