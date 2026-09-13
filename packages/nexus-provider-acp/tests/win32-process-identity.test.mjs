import { describe, test } from 'node:test';
import assert from 'node:assert/strict';
import { PassThrough } from 'node:stream';
import {
  identityStillMatches,
  observeProcessIdentity,
  queryOsProcessIdentity,
  reapChild,
  setCommandRunners,
  setPlatformOverride,
} from '../dist/index.js';

/**
 * Windows lane, exercised on Darwin through the injectable command seam.
 *
 * The point of these tests is that the Windows path performs a REAL identity
 * query and a REAL tree kill by command, and that the identity re-check gates the
 * kill — so a recycled PID is never signalled.
 */
function fakeChild(pid) {
  return {
    pid,
    exitCode: null,
    signalCode: null,
    stdin: new PassThrough(),
    stdout: new PassThrough(),
    stderr: new PassThrough(),
    kill: () => {
      throw new Error('raw kill must not be used on win32');
    },
  };
}

function withWin32(handlers, body) {
  const calls = [];
  setPlatformOverride('win32');
  setCommandRunners(
    (file, args) => {
      calls.push({ kind: 'sync', file, args: [...args] });
      return handlers.sync(file, args) ?? 'null';
    },
    async (file, args) => {
      calls.push({ kind: 'async', file, args: [...args] });
      handlers.async?.(file, args);
    },
  );
  return Promise.resolve()
    .then(() => body(calls))
    .finally(() => {
      setPlatformOverride(null);
      setCommandRunners(null, null);
    });
}

const CREATION_DATE_JSON = JSON.stringify({
  CreationDate: '/Date(1699999999000)/',
  ParentProcessId: 4242,
});

describe('win32 process identity', () => {
  test('parses CreationDate and parent PID as the identity', () =>
    withWin32({ sync: () => CREATION_DATE_JSON }, () => {
      const identity = queryOsProcessIdentity(777);
      assert.deepEqual(identity, {
        pid: 777,
        process_birth: '1699999999000',
        group_id: '4242',
      });
    }));

  test('an absent process yields no identity rather than a match', () =>
    withWin32({ sync: () => 'null' }, () => {
      assert.equal(queryOsProcessIdentity(778), null);
      assert.equal(observeProcessIdentityFor(778), null);
    }));

  test('identity comparison refuses a reused PID (different creation time)', () =>
    withWin32(
      {
        sync: () =>
          JSON.stringify({
            CreationDate: '/Date(1700000000001)/',
            ParentProcessId: 4242,
          }),
      },
      () => {
        const child = fakeChild(777);
        const bound = {
          pid: 777,
          process_birth: '1699999999000',
          group_id: '4242',
        };
        assert.equal(
          identityStillMatches(child, bound),
          false,
          'a different creation time must not match',
        );
      },
    ));

  test('identity comparison accepts the same birth and parent', () =>
    withWin32({ sync: () => CREATION_DATE_JSON }, () => {
      const child = fakeChild(777);
      const bound = {
        pid: 777,
        process_birth: '1699999999000',
        group_id: '4242',
      };
      assert.equal(identityStillMatches(child, bound), true);
    }));
});

describe('win32 owned-tree termination', () => {
  test('kills the owned tree by command, never by raw signal', () =>
    withWin32(
      { sync: () => CREATION_DATE_JSON },
      async (calls) => {
        const child = fakeChild(777);
        const bound = {
          pid: 777,
          process_birth: '1699999999000',
          group_id: '4242',
        };
        // reapChild re-checks identity, then signals the owned tree.
        await reapChild(child, bound, 50).catch(() => undefined);

        const kills = calls.filter((c) => c.args.includes('/T'));
        assert.equal(kills.length >= 1, true, 'the owned tree must be killed');
        assert.equal(kills[0].args.includes('/F'), true, 'the kill must be forceful');
        assert.equal(kills[0].args.includes('777'), true, 'the kill must target the owned pid');
      },
    ));

  test('a mismatched identity performs NO kill', () =>
    withWin32(
      {
        sync: () =>
          JSON.stringify({
            CreationDate: '/Date(1700000000001)/',
            ParentProcessId: 9999,
          }),
      },
      async (calls) => {
        const child = fakeChild(777);
        const bound = {
          pid: 777,
          process_birth: '1699999999000',
          group_id: '4242',
        };
        const result = await reapChild(child, bound, 50);
        assert.equal(result.confirmed, false, 'a mismatched identity is unconfirmed');
        assert.equal(
          calls.filter((c) => c.args.includes('/T')).length,
          0,
          'a mismatched identity must never be signalled',
        );
      },
    ));
});

/** `observeProcessIdentity` needs a child-like object; this keeps the case local. */
function observeProcessIdentityFor(pid) {
  try {
    return observeProcessIdentity(fakeChild(pid));
  } catch {
    return null;
  }
}
