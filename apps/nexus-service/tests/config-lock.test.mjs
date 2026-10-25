import assert from 'node:assert/strict';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, utimesSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';
import { before, describe, test } from 'node:test';
import { parse, stringify } from 'smol-toml';

const serviceRoot = join(dirname(fileURLToPath(import.meta.url)), '..');

/**
 * I-000065: the daemon-home `config.toml` advisory lock. The service writer is
 * one of three cross-process writers (core CLI + desktop main are the others),
 * so the lock is the file `<home>/.nexus42/config.toml.lock`, exclusive-create.
 * These tests drive the exported service writer against a simulated foreign
 * writer holding the same lock — the cross-process exclusion the in-process
 * gate cannot provide.
 */

const NEXUS_DIR = '.nexus42';
const CONFIG_NAME = 'config.toml';
const LOCK_NAME = 'config.toml.lock';

const configPathFor = (home) => join(home, NEXUS_DIR, CONFIG_NAME);
const lockPathFor = (home) => join(home, NEXUS_DIR, LOCK_NAME);

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

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
    const build = spawnSync('npx', ['tsc', '-p', 'tsconfig.json'], {
      cwd: serviceRoot,
      stdio: 'inherit',
    });
    assert.equal(build.status, 0);
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
    // The writer released the lock it took.
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
    // Fault injection: the diagnostics write is the only reachable failure
    // after a successful exclusive create, and no portable filesystem call
    // forces it. Patch that one line in a copy of the built module (same
    // directory, so its relative imports resolve) and assert the invariant
    // that matters: no lock file left behind for the next writer to wait out.
    const source = readFileSync(join(serviceRoot, 'dist/agent-profile.js'), 'utf8');
    const marker = 'await lock.writeFile(`${process.pid} ${new Date().toISOString()} ${token}\\n`);';
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
    // The simulated core/desktop writer read the document and now pauses
    // inside its read-modify-write window — the exact lost-update window the
    // lock removes. Its write is the non-atomic `std::fs::write` shape.
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
    // Without the lock the service's write lands inside the foreign window and
    // the foreign write replaces the document from its stale snapshot.
    assert.deepEqual({ ...stored.agent_profile }, { name: 'codex-cli' });
    assert.equal(stored.foreign_marker, 'kept');
    assert.equal(stored.active_creator_id, 'ctr_localaaaaaaaaaaaa');
  });

  /**
   * MF-2 (I-000091): a stale lock that cannot be removed (root-owned or
   * immutable file → EACCES/EPERM) must fail the write instead of hot-looping
   * create/stat forever. A directory at the lock path is the portable stand-in:
   * exclusive-create reports EEXIST and removal never succeeds.
   */
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

  /**
   * SF-1 (I-000092): a holder stalled past the stale window is stolen; its
   * release on resume must not unlink the new owner's lock and so admit a third
   * writer into the second writer's read-modify-write.
   */
  test('a stolen holder does not unlink the new owner\'s lock', async () => {
    const source = readFileSync(join(serviceRoot, 'dist/agent-profile.js'), 'utf8');
    // Fault injection (same idiom as the stranded-lock test): stall the holder
    // inside its read-modify-write, so it can be stolen from in the open.
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
      await sleep(80); // the writer holds the lock and is inside its RMW

      // A second writer finds the held lock stale and steals it.
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
});
