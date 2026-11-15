// Native config publication through the real unsigned native binding.
//
// The Rust publisher proves probe/policy/staging semantics in
// `crates/nexus-home-layout/src/config_lock.rs`; this suite proves the built
// `.node` payload exposes the same publication to JavaScript: an arbitrary
// absolute target, a write-denied target that keeps its bytes, a preserved
// `0640` policy, and the refusal envelope (`invalid_input` / `forbidden`).

import { describe, test } from 'node:test';
import assert from 'node:assert/strict';
import {
  chmodSync,
  chownSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { publishDocument } from '../dist/index.js';
import { parseNativeCoreError } from '../dist/errors.js';
import { loadNativeBinding } from '../dist/loader.js';

function makeHome() {
  const home = mkdtempSync(join(tmpdir(), 'nexus-publication-'));
  return home;
}

function makeTarget(home, contents) {
  const directory = join(home, 'agent-host');
  mkdirSync(directory, { recursive: true });
  const target = join(directory, 'config.toml');
  writeFileSync(target, contents);
  return target;
}

function wireCode(error) {
  return parseNativeCoreError(error)?.code ?? null;
}

/**
 * A group this process may assign that differs from its effective gid, or null
 * when it belongs to no other group. Without one a staged-and-renamed file
 * always inherits the process egid, so a captured-group assertion cannot fail.
 */
function foreignGroup() {
  const egid = process.getegid?.();
  return process.getgroups?.().find((gid) => gid !== egid) ?? null;
}

describe('native config publication (real binding)', () => {
  test('publishes an arbitrary absolute target path and leaves no staging residue', async () => {
    const home = makeHome();
    try {
      const target = makeTarget(home, 'name = "old"\n');

      await publishDocument(target, 'name = "new"\n');

      assert.equal(readFileSync(target, 'utf8'), 'name = "new"\n');
      assert.deepEqual(readdirSync(join(home, 'agent-host')), ['config.toml']);
    } finally {
      rmSync(home, { recursive: true, force: true });
    }
  });

  test('a relative target is refused as invalid_input on both sides', async () => {
    // The facade check runs first (a plain programming-error guard) ...
    await assert.rejects(publishDocument('config.toml', 'name = "x"\n'), (error) =>
      /absolute path/.test(error.message),
    );
    // ... and the native side re-checks, so a direct binding user gets the wire
    // `invalid_input` envelope rather than a filesystem guess.
    const binding = loadNativeBinding();
    const error = await binding.publishDocument('config.toml', 'name = "x"\n').then(
      () => null,
      (rejection) => rejection,
    );
    assert.ok(error, 'a relative path must be rejected');
    assert.equal(wireCode(error), 'invalid_input');
  });

  test('a write-denied target refuses as forbidden and keeps its bytes', async () => {
    const home = makeHome();
    try {
      const target = makeTarget(home, 'name = "locked"\n');
      chmodSync(target, 0o444);

      const error = await publishDocument(target, 'name = "new"\n').then(
        () => null,
        (rejection) => rejection,
      );

      assert.ok(error, 'a write-denied target must be refused');
      assert.equal(wireCode(error), 'forbidden');
      // The envelope is content-free by construction: an empty `details` is not
      // even serialised, and a message-only assertion would pass a regression
      // that moved the contents into it.
      assert.deepEqual(
        parseNativeCoreError(error)?.details ?? {},
        {},
        'the refusal carries no details',
      );
      assert.equal(readFileSync(target, 'utf8'), 'name = "locked"\n');
      assert.deepEqual(readdirSync(join(home, 'agent-host')), ['config.toml']);
    } finally {
      const target = join(home, 'agent-host', 'config.toml');
      if (existsSync(target)) chmodSync(target, 0o644);
      rmSync(home, { recursive: true, force: true });
    }
  });

  test('an existing 0640 target keeps its mode and group', async (t) => {
    const group = foreignGroup();
    if (group === null) {
      t.skip(
        'no supplementary group distinct from the process egid: a captured-group assertion cannot fail here',
      );
      return;
    }
    const home = makeHome();
    try {
      const target = makeTarget(home, 'name = "old"\n');
      chmodSync(target, 0o640);
      chownSync(target, -1, group);
      assert.equal(statSync(target).gid, group, 'the fixture group differs from the process egid');

      await publishDocument(target, 'name = "new"\n');

      const after = statSync(target);
      assert.equal(after.mode & 0o777, 0o640, 'the captured mode is reapplied');
      assert.equal(after.gid, group, 'the captured group is reapplied');
      assert.equal(readFileSync(target, 'utf8'), 'name = "new"\n');
    } finally {
      rmSync(home, { recursive: true, force: true });
    }
  });
});
