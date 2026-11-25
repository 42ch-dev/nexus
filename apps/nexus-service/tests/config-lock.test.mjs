import assert from 'node:assert/strict';
import {
  chmodSync,
  chownSync,
  closeSync,
  fstatSync,
  openSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
  utimesSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { pathToFileURL } from 'node:url';
import { before, describe, test } from 'node:test';
import { parse, stringify } from 'smol-toml';

const serviceRoot = join(dirname(fileURLToPath(import.meta.url)), '..');

const NEXUS_DIR = '.nexus42';
const CONFIG_NAME = 'config.toml';
const LOCK_NAME = 'config.toml.lock';

const configPathFor = (home) => join(home, NEXUS_DIR, CONFIG_NAME);
const lockPathFor = (home) => join(home, NEXUS_DIR, LOCK_NAME);

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

function foreignGroup() {
  const egid = process.getegid?.();
  return process.getgroups?.().find((gid) => gid !== egid) ?? null;
}

/** A daemon home seeded with the core-owned keys the profile write must keep. */
function seededHome() {
  const home = mkdtempSync(join(tmpdir(), 'nexus-config-lock-'));
  mkdirSync(join(home, NEXUS_DIR), { recursive: true });
  writeFileSync(
    configPathFor(home),
    'active_creator_id = "ctr_localaaaaaaaaaaaa"\n\n[active_workspace_slug_by_creator]\nctr_localaaaaaaaaaaaa = "default"\n',
    'utf8',
  );
  return home;
}

/** Exclusive-create the lock out of band, exactly as a foreign process would. */
function acquireForeignLock(home) {
  writeFileSync(lockPathFor(home), `${process.pid} ${new Date().toISOString()}\n`, { flag: 'wx' });
}

/** Fail the test instead of hanging the runner when the writer never returns. */
function withDeadline(promise, ms, label) {
  return Promise.race([
    promise,
    sleep(ms).then(() => {
      throw new Error(`${label} did not return within ${ms} ms`);
    }),
  ]);
}

describe('config.toml advisory lock (I-000065)', () => {
  let writeAgentProfile;

  before(async () => {
    assert.ok(
      existsSync(join(serviceRoot, 'dist', 'agent-profile.js')),
      'dist/agent-profile.js is missing — run `pnpm --filter @42ch/nexus-service run test` (or `… run build`) first',
    );
    ({ writeAgentProfile } = await import(join(serviceRoot, 'dist/agent-profile.js')));
  });

  test('the service write waits while a foreign process holds the lock', async () => {
    const home = seededHome();
    acquireForeignLock(home);

    let finished = false;
    const pending = writeAgentProfile(home, { name: 'codex-cli' }).then(() => {
      finished = true;
    });

    await sleep(150);
    assert.equal(finished, false, 'the profile write must wait for config.toml.lock');

    rmSync(lockPathFor(home), { force: true });
    await withDeadline(pending, 5000, 'the profile write');
    assert.equal(finished, true);

    const stored = parse(readFileSync(configPathFor(home), 'utf8'));
    assert.deepEqual({ ...stored.agent_profile }, { name: 'codex-cli' });
    assert.equal(existsSync(lockPathFor(home)), false);
  });

  test('a stale lock (holder older than 10 s) is stolen instead of blocking', async () => {
    const home = seededHome();
    acquireForeignLock(home);
    const longAgo = new Date(Date.now() - 60_000);
    utimesSync(lockPathFor(home), longAgo, longAgo);

    await withDeadline(writeAgentProfile(home, { name: 'codex-cli' }), 5000, 'the profile write');

    const stored = parse(readFileSync(configPathFor(home), 'utf8'));
    assert.deepEqual({ ...stored.agent_profile }, { name: 'codex-cli' });
    assert.equal(stored.active_creator_id, 'ctr_localaaaaaaaaaaaa');
  });

  test('a failure after the lock file is created releases it instead of stranding it', async () => {
    const source = readFileSync(join(serviceRoot, 'dist/agent-profile.js'), 'utf8');
    const marker = 'await lock.writeFile(`${token}\\n`);';
    assert.ok(
      source.includes(marker),
      'the diagnostics-write marker must exist in dist/agent-profile.js',
    );
    const copyPath = join(serviceRoot, 'dist', `__fault-agent-profile-${process.pid}.mjs`);
    writeFileSync(copyPath, source.replace(marker, `throw new Error('injected');`), 'utf8');
    try {
      const { writeAgentProfile: faultyWrite } = await import(pathToFileURL(copyPath).href);
      const home = seededHome();
      await assert.rejects(
        withDeadline(faultyWrite(home, { name: 'codex-cli' }), 5000, 'the faulty write'),
      );
      assert.equal(
        existsSync(lockPathFor(home)),
        false,
        'the failed acquisition must not strand the lock',
      );
    } finally {
      rmSync(copyPath, { force: true });
    }
  });

  test('a foreign read-modify-write interleaved in the old race window loses nothing', async () => {
    const home = seededHome();
    acquireForeignLock(home);
    const foreignDocument = parse(readFileSync(configPathFor(home), 'utf8'));
    const foreignWrite = (async () => {
      await sleep(120);
      foreignDocument.foreign_marker = 'kept';
      writeFileSync(configPathFor(home), stringify(foreignDocument), 'utf8');
      rmSync(lockPathFor(home), { force: true });
    })();

    await Promise.all([
      withDeadline(writeAgentProfile(home, { name: 'codex-cli' }), 5000, 'the profile write'),
      foreignWrite,
    ]);

    const stored = parse(readFileSync(configPathFor(home), 'utf8'));
    assert.deepEqual({ ...stored.agent_profile }, { name: 'codex-cli' });
    assert.equal(stored.foreign_marker, 'kept');
    assert.equal(stored.active_creator_id, 'ctr_localaaaaaaaaaaaa');
  });

  test('a stale lock that cannot be removed fails instead of spinning', async () => {
    const home = seededHome();
    mkdirSync(lockPathFor(home));
    const longAgo = new Date(Date.now() - 60_000);
    utimesSync(lockPathFor(home), longAgo, longAgo);

    await assert.rejects(
      withDeadline(writeAgentProfile(home, { name: 'codex-cli' }), 90_000, 'the blocked write'),
      (error) => error.status === 500 && /stale and cannot be removed/.test(error.message),
    );
  });

  test('a stolen holder does not unlink the new owner\'s lock', async () => {
    const source = readFileSync(join(serviceRoot, 'dist/agent-profile.js'), 'utf8');
    const marker = 'await writeConfigDocument(home, document);';
    assert.ok(source.includes(marker), 'the write marker must exist in dist/agent-profile.js');
    const copyPath = join(serviceRoot, 'dist', `__slow-agent-profile-${process.pid}.mjs`);
    writeFileSync(
      copyPath,
      source.replace(
        marker,
        `await new Promise((resolve) => setTimeout(resolve, 250)); ${marker}`,
      ),
      'utf8',
    );
    try {
      const { writeAgentProfile: slowWrite } = await import(pathToFileURL(copyPath).href);
      const home = seededHome();
      const pending = slowWrite(home, { name: 'codex-cli' });
      const lockDeadline = Date.now() + 2000;
      while (!existsSync(lockPathFor(home))) {
        assert.ok(
          Date.now() < lockDeadline,
          'the slow writer must acquire config.toml.lock within 2000 ms',
        );
        await sleep(5);
      }

      const longAgo = new Date(Date.now() - 60_000);
      utimesSync(lockPathFor(home), longAgo, longAgo);
      rmSync(lockPathFor(home), { force: true });
      acquireForeignLock(home);

      await withDeadline(pending, 5000, 'the slow write');
      assert.equal(
        existsSync(lockPathFor(home)),
        true,
        "a stolen holder must not unlink the new owner's lock",
      );
    } finally {
      rmSync(copyPath, { force: true });
    }
  });

  /**
   * QC1-002 (I-000105, contract §2): the whole line is the token. A foreign
   * lock that merely ends with our token line — a prefix was prepended — must
   * never be unlinked; only byte equality proves this holder still owns it.
   */
  test('a release never unlinks a lock that only ends with our token', async () => {
    const source = readFileSync(join(serviceRoot, 'dist/agent-profile.js'), 'utf8');
    const marker = 'await writeConfigDocument(home, document);';
    assert.ok(source.includes(marker), 'the write marker must exist in dist/agent-profile.js');
    const copyPath = join(serviceRoot, 'dist', `__rewritten-agent-profile-${process.pid}.mjs`);
    writeFileSync(
      copyPath,
      source.replace(
        marker,
        `await new Promise((resolve) => setTimeout(resolve, 250)); ${marker}`,
      ),
      'utf8',
    );
    try {
      const { writeAgentProfile: slowWrite } = await import(pathToFileURL(copyPath).href);
      const home = seededHome();
      const pending = slowWrite(home, { name: 'codex-cli' });

      const lockDeadline = Date.now() + 2000;
      let held = '';
      while (!held.endsWith('\n')) {
        assert.ok(
          Date.now() < lockDeadline,
          'the slow writer must write config.toml.lock within 2000 ms',
        );
        held = existsSync(lockPathFor(home)) ? readFileSync(lockPathFor(home), 'utf8') : '';
        if (!held.endsWith('\n')) await sleep(5);
      }
      const rewritten = `foreign-prefix ${held}`;
      writeFileSync(lockPathFor(home), rewritten, 'utf8');

      await withDeadline(pending, 5000, 'the slow write');
      assert.equal(existsSync(lockPathFor(home)), true, 'a foreign lock must never be unlinked');
      assert.equal(readFileSync(lockPathFor(home), 'utf8'), rewritten);
    } finally {
      rmSync(copyPath, { force: true });
    }
  });
});

describe('config publication policy (native publisher)', () => {
  let writeAgentProfile;

  before(async () => {
    assert.ok(
      existsSync(join(serviceRoot, 'dist', 'agent-profile.js')),
      'dist/agent-profile.js is missing — run `pnpm --filter @42ch/nexus-service run build` first',
    );
    ({ writeAgentProfile } = await import(join(serviceRoot, 'dist', 'agent-profile.js')));
  });

  test('a write-denied config is refused as 403 forbidden and keeps its bytes', async (t) => {
    if (process.getuid?.() === 0) {
      t.skip('EACCES cannot be simulated for the superuser');
      return;
    }
    const home = seededHome();
    const original = readFileSync(configPathFor(home), 'utf8');
    chmodSync(configPathFor(home), 0o444);
    let error;
    try {
      error = await withDeadline(
        writeAgentProfile(home, { name: 'codex-cli' }).then(
          () => null,
          (rejection) => rejection,
        ),
        5000,
        'the refused profile write',
      );
    } finally {
      chmodSync(configPathFor(home), 0o600);
    }

    assert.ok(error, 'a write-denied config must be refused');
    assert.equal(error.code, 'forbidden');
    assert.equal(error.status, 403);
    assert.equal(readFileSync(configPathFor(home), 'utf8'), original);
    assert.deepEqual(readdirSync(join(home, NEXUS_DIR)), [CONFIG_NAME]);
  });

  test('an existing 0640 config keeps its mode and group with unrelated keys intact', async (t) => {
    const group = foreignGroup();
    if (group === null) {
      t.skip(
        'no supplementary group distinct from the process egid: a captured-group assertion cannot fail here',
      );
      return;
    }
    const home = seededHome();
    const path = configPathFor(home);
    chmodSync(path, 0o640);
    chownSync(path, -1, group);
    assert.equal(statSync(path).gid, group, 'the fixture group differs from the process egid');

    await withDeadline(writeAgentProfile(home, { name: 'codex-cli' }), 5000, 'the profile write');

    const fd = openSync(path, 'r');
    const after = fstatSync(fd);
    const stored = parse(readFileSync(fd, 'utf8'));
    closeSync(fd);
    assert.equal(after.mode & 0o777, 0o640, 'the captured mode is reapplied');
    assert.equal(after.gid, group, 'the captured group is reapplied');
    assert.deepEqual({ ...stored.agent_profile }, { name: 'codex-cli' });
    assert.equal(stored.active_creator_id, 'ctr_localaaaaaaaaaaaa');
    assert.deepEqual(
      { ...stored.active_workspace_slug_by_creator },
      { ctr_localaaaaaaaaaaaa: 'default' },
    );
  });
});
