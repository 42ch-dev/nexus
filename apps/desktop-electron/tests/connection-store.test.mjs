#!/usr/bin/env node
/**
 * Connection store tests (v1.192 P0-T5).
 *
 * Defends D-18 against the Tauri-era behavior: the secret is never read
 * back to the renderer projection, there is NO plaintext write fallback,
 * failed encryption is non-destructive, clear removes material and never
 * reimports, and the one-time legacy import encrypts before switching
 * stores. `safeStorage` is stubbed at the store boundary (headless Node
 * has no Electron keychain); the stub preserves the safeStorage contract
 * (isEncryptionAvailable / encryptString / decryptString).
 *
 *   node --test apps/desktop-electron/tests/connection-store.test.mjs
 */
import assert from 'node:assert/strict';
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { ConnectionStore } from '../dist/connection-store.js';
import { errorCode, errorMessage } from '../dist/desktop-contract.js';

const ENDPOINT = 'https://daemon.example.com:8443';

function xorCipher(key) {
  return {
    isEncryptionAvailable: () => true,
    encryptString(plain) {
      const buf = Buffer.from(plain, 'utf8');
      const out = new Uint8Array(buf.length + 1);
      out[0] = key;
      for (let i = 0; i < buf.length; i++) out[i + 1] = buf[i] ^ key;
      return out;
    },
    decryptString(data) {
      if (data[0] !== key) throw new Error('wrong key');
      const out = Buffer.alloc(data.length - 1);
      for (let i = 1; i < data.length; i++) out[i - 1] = data[i] ^ key;
      return out.toString('utf8');
    },
  };
}

const unavailableStorage = {
  isEncryptionAvailable: () => false,
  encryptString() {
    throw new Error('encryption unavailable');
  },
  decryptString() {
    throw new Error('encryption unavailable');
  },
};

/**
 * Encryption works, but the produced bytes cannot be read back — the state
 * `ConnectionStore.persist` refuses before publication.
 */
const encryptOnlyStorage = {
  isEncryptionAvailable: () => true,
  encryptString: (plain) => xorCipher(0x5a).encryptString(plain),
  decryptString() {
    throw new Error('ciphertext is not decryptable');
  },
};

function makeDeps(t, overrides = {}) {
  const dir = mkdtempSync(join(tmpdir(), 'nexus-conn-store-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  return {
    filePath: join(dir, 'connection-config.enc'),
    storage: xorCipher(0x5a),
    ...overrides,
  };
}

const BASE_CONFIG = { endpointUrl: ENDPOINT, hasApiKey: true, active: true };

// ---------------------------------------------------------------------------
// Secret isolation
// ---------------------------------------------------------------------------

test('saved config round-trips as a redacted projection; secret never read back', async (t) => {
  const deps = makeDeps(t);
  const store = await ConnectionStore.open(deps);
  const returned = await store.set({ ...BASE_CONFIG }, { action: 'replace', value: 'sk-live-secret' });
  assert.equal(returned.endpointUrl, ENDPOINT);
  assert.equal(returned.hasApiKey, true);
  assert.ok(!('apiKey' in returned), 'projection must not contain the key');
  const loaded = await store.get();
  assert.equal(loaded?.endpointUrl, ENDPOINT);
  assert.equal(loaded?.hasApiKey, true);
  assert.ok(!loaded || !('apiKey' in loaded));

  // On-disk bytes must be ciphertext, never the plaintext key.
  const onDisk = readFileSync(deps.filePath, 'utf8');
  assert.ok(!onDisk.includes('sk-live-secret'), 'plaintext key must never hit disk');
  assert.ok(onDisk.includes('credential'));

  // Main-side injection authority sees the real key (never over IPC).
  const auth = store.getAuth();
  assert.equal(auth?.endpointOrigin, 'https://daemon.example.com:8443');
  assert.equal(auth?.apiKey, 'sk-live-secret');
});

test('inactive config yields no main-side auth', async (t) => {
  const store = await ConnectionStore.open(makeDeps(t));
  await store.set({ ...BASE_CONFIG, active: false }, { action: 'replace', value: 'sk-1' });
  assert.equal(store.getAuth(), null);
});

test('credential keep retains the key only for the same endpoint', async (t) => {
  const store = await ConnectionStore.open(makeDeps(t));
  await store.set({ ...BASE_CONFIG }, { action: 'replace', value: 'sk-1' });
  const kept = await store.set(
    { endpointUrl: ENDPOINT, hasApiKey: true, active: true },
    { action: 'keep' },
  );
  assert.equal(kept.hasApiKey, true);
  assert.equal(store.getAuth()?.apiKey, 'sk-1');

  // Endpoint change with keep: the old endpoint's credential never carries.
  const moved = await store.set(
    { endpointUrl: 'https://other.example.com', hasApiKey: true },
    { action: 'keep' },
  );
  assert.equal(moved.hasApiKey, false);
  assert.equal(store.getAuth(), null);
});

test('empty replace explicitly clears the credential', async (t) => {
  const store = await ConnectionStore.open(makeDeps(t));
  await store.set({ ...BASE_CONFIG }, { action: 'replace', value: 'sk-1' });
  const cleared = await store.set({ ...BASE_CONFIG }, { action: 'replace', value: '' });
  assert.equal(cleared.hasApiKey, false);
  assert.equal(store.getAuth(), null);
});

// ---------------------------------------------------------------------------
// D-18: no plaintext write fallback; failed encryption non-destructive
// ---------------------------------------------------------------------------

test('encryption unavailable: set fails with structured error and writes nothing', async (t) => {
  const deps = makeDeps(t, { storage: unavailableStorage });
  const store = await ConnectionStore.open(deps);
  await assert.rejects(
    () => store.set({ ...BASE_CONFIG }, { action: 'replace', value: 'sk-1' }),
    (err) => errorCode(err) === 'secure_storage_unavailable',
  );
  assert.equal(existsSync(deps.filePath), false, 'no file may be written without encryption');
  assert.equal(await store.get(), null);
});

test('failed encryption is non-destructive: previous state survives', async (t) => {
  const deps = makeDeps(t);
  const store = await ConnectionStore.open(deps);
  await store.set({ ...BASE_CONFIG }, { action: 'replace', value: 'sk-before' });

  const broken = await ConnectionStore.open({ ...deps, storage: unavailableStorage });
  await assert.rejects(
    () => broken.set({ ...BASE_CONFIG }, { action: 'replace', value: 'sk-after' }),
    (err) => errorCode(err) === 'secure_storage_unavailable',
  );
  // Reopen healthy: the pre-failure credential must be fully intact.
  const healthy = await ConnectionStore.open(deps);
  assert.equal(healthy.getAuth()?.apiKey, 'sk-before');
});

// ---------------------------------------------------------------------------
// Clear + one-time legacy import
// ---------------------------------------------------------------------------

test('clear removes stored material; reopen never reimports legacy (durable tombstone)', async (t) => {
  const deps = makeDeps(t);
  let legacyReads = 0;
  let legacyCleanups = 0;
  const withLegacy = {
    ...deps,
    readLegacy: async () => {
      legacyReads += 1;
      return JSON.stringify({ endpointUrl: ENDPOINT, apiKey: 'sk-legacy', active: true });
    },
    cleanupLegacy: async () => {
      legacyCleanups += 1;
    },
  };
  const store = await ConnectionStore.open(withLegacy);
  assert.equal(legacyReads, 1, 'fresh install (no marker, no store): one-time import runs');
  assert.equal(legacyCleanups, 1, 'a successful migration removes the legacy plaintext sources');
  assert.equal(store.getAuth()?.apiKey, 'sk-legacy');
  assert.equal(existsSync(deps.filePath), true);

  await store.delete();
  assert.equal(existsSync(deps.filePath), false);
  assert.equal(await store.get(), null);
  const markerPath = join(deps.filePath, '..', 'connection-config.cleared');
  assert.equal(existsSync(markerPath), true, 'clear writes a durable tombstone');
  assert.ok(!readFileSync(markerPath, 'utf8').includes('sk-legacy'), 'tombstone is non-secret');

  // Reopen after clear: the untouched legacy key must NOT be re-imported.
  const reopened = await ConnectionStore.open(withLegacy);
  assert.equal(legacyReads, 1, 'cleared tombstone honored: no reimport on reopen');
  assert.equal(legacyCleanups, 1, 'a cleared store owns no material: nothing to clean up');
  assert.equal(await reopened.get(), null);
  assert.equal(reopened.getAuth(), null);
  assert.equal(existsSync(deps.filePath), false, 'no store recreated from legacy');
  assert.equal(existsSync(markerPath), true, 'tombstone survives the reopen');
});

test('cleared tombstone survives a failed encryption attempt non-destructively', async (t) => {
  const deps = makeDeps(t);
  const store = await ConnectionStore.open(deps);
  await store.set({ ...BASE_CONFIG }, { action: 'replace', value: 'sk-live-secret' });
  await store.delete();
  const markerPath = join(deps.filePath, '..', 'connection-config.cleared');
  assert.equal(existsSync(markerPath), true);

  // Open with encryption unavailable and legacy present: the tombstone must
  // keep the import path closed and survive untouched — nothing encrypted,
  // nothing written, marker bytes intact, no legacy source removed.
  let cleanups = 0;
  const reopened = await ConnectionStore.open({
    ...deps,
    storage: unavailableStorage,
    readLegacy: async () => JSON.stringify({ endpointUrl: ENDPOINT, apiKey: 'sk-legacy' }),
    cleanupLegacy: async () => {
      cleanups += 1;
    },
  });
  assert.equal(await reopened.get(), null);
  assert.equal(existsSync(deps.filePath), false, 'no store file created');
  assert.equal(existsSync(markerPath), true, 'tombstone survives the failed-encryption open');
  assert.equal(cleanups, 0, 'a cleared install removes no legacy source');
});

test('delete failure surfaces as structured error; bytes and prior state survive', async (t) => {
  const deps = makeDeps(t);
  const store = await ConnectionStore.open(deps);
  await store.set({ ...BASE_CONFIG }, { action: 'replace', value: 'sk-live-secret' });
  assert.equal(existsSync(deps.filePath), true);

  const failing = await ConnectionStore.open({
    ...deps,
    removeFile() {
      const err = new Error('EPERM: operation not permitted');
      err.code = 'EPERM';
      throw err;
    },
  });
  await assert.rejects(
    () => failing.delete(),
    (err) => errorCode(err) === 'secure_store_delete_failed',
  );
  assert.equal(existsSync(deps.filePath), true, 'failed clear must leave the file in place');
  assert.equal(failing.getAuth()?.apiKey, 'sk-live-secret', 'no false clear: state stays intact');
});

test('legacy migration removes both sources only after the encrypted store is persisted and readable', async (t) => {
  const deps = makeDeps(t);
  const legacyPath = join(deps.filePath, '..', 'connection_config.json');
  writeFileSync(legacyPath, JSON.stringify({ endpointUrl: ENDPOINT, apiKey: 'sk-legacy', active: true }));

  const observed = [];
  const store = await ConnectionStore.open({
    ...deps,
    readLegacy: async () => readFileSync(legacyPath, 'utf8'),
    cleanupLegacy: async () => {
      // Observed from inside the cleanup call: by the time either legacy
      // source is removed the encrypted store must exist on disk, must not
      // contain the plaintext, and must decrypt again.
      const onDisk = JSON.parse(readFileSync(deps.filePath, 'utf8'));
      observed.push({
        plaintextOnDisk: readFileSync(deps.filePath, 'utf8').includes('sk-legacy'),
        decrypted: deps.storage.decryptString(new Uint8Array(Buffer.from(onDisk.credential, 'base64'))),
        legacyStillPresent: existsSync(legacyPath),
      });
      rmSync(legacyPath, { force: true }); // the JSON target the adapter removes
    },
  });

  assert.deepEqual(observed, [
    { plaintextOnDisk: false, decrypted: 'sk-legacy', legacyStillPresent: true },
  ]);
  assert.equal(existsSync(legacyPath), false, 'the legacy plaintext source is gone after migration');
  assert.equal(store.legacyCleanupFailure, null, 'a clean migration claims no cleanup failure');
  const projection = await store.get();
  assert.equal(projection?.hasApiKey, true);
  assert.ok(!projection || !('apiKey' in projection), 'the projection stays redacted');
  assert.equal(store.getAuth()?.apiKey, 'sk-legacy', 'the migrated key is usable from the encrypted store');
});

test('legacy migration removes nothing when encryption or the encrypted write is refused', async (t) => {
  // (1) Encryption unavailable: the plaintext original must survive.
  const denied = makeDeps(t, { storage: unavailableStorage });
  const deniedLegacy = join(denied.filePath, '..', 'connection_config.json');
  writeFileSync(deniedLegacy, JSON.stringify({ endpointUrl: ENDPOINT, apiKey: 'sk-legacy' }));
  let deniedCleanups = 0;
  await assert.rejects(
    () =>
      ConnectionStore.open({
        ...denied,
        readLegacy: async () => readFileSync(deniedLegacy, 'utf8'),
        cleanupLegacy: async () => {
          deniedCleanups += 1;
        },
      }),
    (err) => errorCode(err) === 'secure_storage_unavailable',
  );
  assert.equal(deniedCleanups, 0, 'no cleanup before the encrypted bytes exist');
  assert.ok(readFileSync(deniedLegacy, 'utf8').includes('sk-legacy'), 'the legacy source is preserved');
  assert.equal(existsSync(denied.filePath), false, 'nothing was written in the clear');

  // (2) The encrypted bytes cannot be read back: persist() refuses before
  // publication, so the legacy source must survive that too.
  const unreadable = makeDeps(t, { storage: encryptOnlyStorage });
  const unreadableLegacy = join(unreadable.filePath, '..', 'connection_config.json');
  writeFileSync(unreadableLegacy, JSON.stringify({ endpointUrl: ENDPOINT, apiKey: 'sk-legacy' }));
  let unreadableCleanups = 0;
  await assert.rejects(() =>
    ConnectionStore.open({
      ...unreadable,
      readLegacy: async () => readFileSync(unreadableLegacy, 'utf8'),
      cleanupLegacy: async () => {
        unreadableCleanups += 1;
      },
    }),
  );
  assert.equal(unreadableCleanups, 0, 'no cleanup after a refused persist');
  assert.ok(readFileSync(unreadableLegacy, 'utf8').includes('sk-legacy'), 'the legacy source is preserved');
  assert.equal(existsSync(unreadable.filePath), false, 'no unreadable store was published');
});

test('a refused legacy cleanup still activates the encrypted store and keeps the sanitized failure observable', async (t) => {
  const deps = makeDeps(t);
  const legacyPath = join(deps.filePath, '..', 'connection_config.json');
  writeFileSync(legacyPath, JSON.stringify({ endpointUrl: ENDPOINT, apiKey: 'sk-legacy', active: true }));

  // A refusal carrying both a secret and command output, exactly like a real
  // /usr/bin/security failure — neither may surface.
  const refusal = Object.assign(new Error('security: sk-legacy could not be removed'), {
    code: 1,
    stderr: 'security: SecKeychainItemDelete: sk-legacy\n',
  });
  const store = await ConnectionStore.open({
    ...deps,
    readLegacy: async () => readFileSync(legacyPath, 'utf8'),
    cleanupLegacy: async () => {
      throw refusal;
    },
  });

  // A refusal never blocks activation: the encrypted store is authoritative.
  assert.equal(store.getAuth()?.apiKey, 'sk-legacy', 'the working encrypted session survives the refusal');
  assert.deepEqual(await store.get(), { endpointUrl: ENDPOINT, hasApiKey: true, active: true });

  // ...but the migration is never reported as clean while the plaintext stays.
  const failure = store.legacyCleanupFailure;
  assert.ok(failure !== null, 'the refused cleanup is observable, not swallowed');
  assert.equal(errorCode(failure), 'legacy_credential_cleanup_failed');
  for (const surfaced of [errorMessage(failure), String(failure.stack ?? '')]) {
    assert.ok(!surfaced.includes('sk-legacy'), 'no secret may surface');
    assert.ok(!surfaced.includes('SecKeychainItemDelete'), 'no command output may surface');
  }
  assert.ok(
    errorMessage(failure).includes('(1)'),
    'the numeric exit status stays available for diagnosis',
  );

  assert.equal(existsSync(legacyPath), true, 'the plaintext source survives a refused cleanup');
  assert.equal(existsSync(deps.filePath), true, 'the encrypted store is preserved');
  assert.ok(!readFileSync(deps.filePath, 'utf8').includes('sk-legacy'), 'the ciphertext stays plaintext-free');
});

test('a normal reopen finishes an interrupted legacy cleanup idempotently', async (t) => {
  const deps = makeDeps(t);
  const legacyPath = join(deps.filePath, '..', 'connection_config.json');
  writeFileSync(legacyPath, JSON.stringify({ endpointUrl: ENDPOINT, apiKey: 'sk-legacy', active: true }));

  // Interrupted migration: the authorized store lands, the removal is refused.
  const refused = await ConnectionStore.open({
    ...deps,
    readLegacy: async () => readFileSync(legacyPath, 'utf8'),
    cleanupLegacy: async () => {
      throw Object.assign(new Error('cleanup refused'), { code: 'EPERM' });
    },
  });
  assert.equal(refused.getAuth()?.apiKey, 'sk-legacy');
  assert.equal(errorCode(refused.legacyCleanupFailure), 'legacy_credential_cleanup_failed');

  // The ordinary reopen never reads the legacy source: the encrypted store is
  // authoritative — and it completes the cleanup the first open could not.
  const cleanups = [];
  const reopenDeps = {
    ...deps,
    readLegacy: async () => {
      throw new Error('an authoritative encrypted store must never re-read the legacy source');
    },
    cleanupLegacy: async () => {
      cleanups.push('cleanup');
      rmSync(legacyPath, { force: true }); // already-absent targets are success
    },
  };
  const first = await ConnectionStore.open(reopenDeps);
  assert.equal(first.getAuth()?.apiKey, 'sk-legacy', 'encrypted auth on the recovery open');
  assert.equal(existsSync(legacyPath), false, 'the interrupted cleanup is finished');
  assert.equal(first.legacyCleanupFailure, null, 'a completed cleanup leaves no failure behind');

  // Reopening again re-runs the now-no-op cleanup: idempotent, same store.
  const second = await ConnectionStore.open(reopenDeps);
  assert.deepEqual(cleanups, ['cleanup', 'cleanup'], 'every open of the store re-runs the cleanup');
  assert.equal(second.legacyCleanupFailure, null);
  assert.equal(second.getAuth()?.apiKey, 'sk-legacy');
  assert.deepEqual(await second.get(), await first.get());
});

test('a legacy source is never imported without the paired cleanup capability', async (t) => {
  const deps = makeDeps(t);
  const legacyPath = join(deps.filePath, '..', 'connection_config.json');
  writeFileSync(legacyPath, JSON.stringify({ endpointUrl: ENDPOINT, apiKey: 'sk-legacy', active: true }));

  let legacyReads = 0;
  const store = await ConnectionStore.open({
    ...deps,
    readLegacy: async () => {
      legacyReads += 1;
      return readFileSync(legacyPath, 'utf8');
    },
  });

  assert.equal(legacyReads, 0, 'no import without a removal capability: the plaintext would survive');
  assert.equal(await store.get(), null);
  assert.equal(store.getAuth(), null);
  assert.equal(existsSync(deps.filePath), false, 'nothing was migrated');
  assert.ok(readFileSync(legacyPath, 'utf8').includes('sk-legacy'), 'the legacy source stays recoverable');
});

test('invalid legacy JSON is skipped without creating a store', async (t) => {
  const deps = makeDeps(t);
  let cleanups = 0;
  const store = await ConnectionStore.open({
    ...deps,
    readLegacy: async () => 'not json',
    cleanupLegacy: async () => {
      cleanups += 1;
    },
  });
  assert.equal(await store.get(), null);
  assert.equal(existsSync(deps.filePath), false);
  assert.equal(cleanups, 0, 'an unimportable legacy record removes nothing');
});

test('encryption unavailable aborts legacy import with structured error and no plaintext write', async (t) => {
  const deps = makeDeps(t);
  let cleanups = 0;
  await assert.rejects(
    () =>
      ConnectionStore.open({
        ...deps,
        storage: unavailableStorage,
        readLegacy: async () => JSON.stringify({ endpointUrl: ENDPOINT, apiKey: 'sk-legacy' }),
        cleanupLegacy: async () => {
          cleanups += 1;
        },
      }),
    (err) => errorCode(err) === 'secure_storage_unavailable',
  );
  assert.equal(existsSync(deps.filePath), false, 'legacy secret must never be written in the clear');
  assert.equal(cleanups, 0, 'a refused import removes no legacy source');
});

// ---------------------------------------------------------------------------
// Endpoint admission (v1.194 P1-T2): ONE strict root-service grammar at the
// direct-set, persisted and legacy boundaries
// ---------------------------------------------------------------------------

test('rejected endpoint values never write or replace the stored connection config', async (t) => {
  const deps = makeDeps(t);
  const store = await ConnectionStore.open(deps);
  await store.set({ ...BASE_CONFIG }, { action: 'replace', value: 'sk-keep' });
  const before = readFileSync(deps.filePath, 'utf8');

  for (const endpointUrl of [
    'null',
    'about:blank',
    'file:///etc/passwd',
    'https://*.example.com',
    'http://user:pass@daemon.example.com:8443',
    // Empty delimiters / dot-segment paths that WHATWG normalizes away.
    'https://daemon.example.com?',
    'https://daemon.example.com#',
    'http://@daemon.example.com',
    'http://:@daemon.example.com',
    'https://daemon.example.com/.',
    'https://daemon.example.com/..',
    'https://daemon.example.com:',
    'https://daemon.example.com:8443/v1/daemon',
    'https://daemon.example.com:8443?token=1',
  ]) {
    await assert.rejects(
      () => store.set({ endpointUrl, hasApiKey: true, active: true }, { action: 'replace', value: 'sk-bad' }),
      (err) => errorCode(err) === 'invalid_input',
      `endpoint must be refused: ${endpointUrl}`,
    );
  }

  assert.equal(readFileSync(deps.filePath, 'utf8'), before, 'a rejected endpoint must not rewrite the store');
  assert.deepEqual(await store.get(), { endpointUrl: ENDPOINT, hasApiKey: true, active: true });
  assert.equal(store.getAuth()?.apiKey, 'sk-keep', 'the previous credential survives');
});

test('raw-form endpoints are refused on the persisted load path and never re-imported', async (t) => {
  for (const endpointUrl of [
    'https://daemon.example.com?',
    'https://daemon.example.com#',
    'http://@daemon.example.com',
    'http://:@daemon.example.com',
    'https://daemon.example.com/.',
    'https://daemon.example.com/..',
    'https://daemon.example.com:',
  ]) {
    const deps = makeDeps(t);
    const contents = JSON.stringify({
      version: 1,
      config: { endpointUrl, active: true, hasApiKey: true },
    });
    writeFileSync(deps.filePath, contents);
    let legacyReads = 0;
    const store = await ConnectionStore.open({
      ...deps,
      readLegacy: async () => {
        legacyReads += 1;
        return JSON.stringify({ endpointUrl: ENDPOINT, apiKey: 'sk-legacy', active: true });
      },
    });

    assert.equal(await store.get(), null, `not activated: ${endpointUrl}`);
    assert.equal(store.getAuth(), null, `no auth authority: ${endpointUrl}`);
    assert.equal(legacyReads, 0, `never replaced by legacy material: ${endpointUrl}`);
    assert.equal(readFileSync(deps.filePath, 'utf8'), contents, `bytes untouched: ${endpointUrl}`);
  }
});

test('a persisted remote endpoint with a trailing slash reloads verbatim with its exact auth origin', async (t) => {
  const deps = makeDeps(t);
  const stored = `${ENDPOINT}/`;
  const store = await ConnectionStore.open(deps);
  await store.set(
    { endpointUrl: stored, hasApiKey: true, active: true },
    { action: 'replace', value: 'sk-remote' },
  );

  // Reopen through readFile: the success path keeps the saved string and
  // projects the exact origin that desktop auth pins.
  const reopened = await ConnectionStore.open(deps);
  assert.deepEqual(await reopened.get(), { endpointUrl: stored, hasApiKey: true, active: true });
  assert.deepEqual(reopened.getAuth(), { endpointOrigin: ENDPOINT, apiKey: 'sk-remote' });
  assert.ok(
    readFileSync(deps.filePath, 'utf8').includes(stored),
    'stored bytes keep the verbatim endpoint identity',
  );
});

test('an invalid stored endpoint is not activated and never triggers the legacy import', async (t) => {
  const deps = makeDeps(t);
  writeFileSync(
    deps.filePath,
    JSON.stringify({ version: 1, config: { endpointUrl: 'null', hasApiKey: false } }),
  );
  let legacyReads = 0;
  let legacyCleanups = 0;
  const store = await ConnectionStore.open({
    ...deps,
    readLegacy: async () => {
      legacyReads += 1;
      return JSON.stringify({ endpointUrl: ENDPOINT, apiKey: 'sk-legacy' });
    },
    cleanupLegacy: async () => {
      legacyCleanups += 1;
    },
  });

  assert.equal(legacyReads, 0, 'an existing-but-invalid store must not be replaced by legacy material');
  assert.equal(legacyCleanups, 0, 'an invalid store removes no legacy source');
  assert.equal(await store.get(), null);
  assert.equal(store.getAuth(), null);
  assert.ok(
    readFileSync(deps.filePath, 'utf8').includes('"null"'),
    'the invalid bytes stay on disk for recovery',
  );

  // Recovery is the user saving a valid config over the invalid one.
  await store.set({ ...BASE_CONFIG }, { action: 'replace', value: 'sk-new' });
  assert.equal((await ConnectionStore.open(deps)).getAuth()?.apiKey, 'sk-new');
});

test('an unreadable existing store differs from an absent one: no legacy import, bytes preserved', async (t) => {
  const deps = makeDeps(t);
  mkdirSync(deps.filePath); // reading a directory fails with a non-ENOENT error
  let legacyReads = 0;
  let legacyCleanups = 0;
  const unreadable = await ConnectionStore.open({
    ...deps,
    readLegacy: async () => {
      legacyReads += 1;
      return JSON.stringify({ endpointUrl: ENDPOINT, apiKey: 'sk-legacy' });
    },
    cleanupLegacy: async () => {
      legacyCleanups += 1;
    },
  });
  assert.equal(legacyReads, 0, 'only ENOENT counts as absent: an unreadable store never imports legacy');
  assert.equal(legacyCleanups, 0, 'an unreadable store removes no legacy source');
  assert.equal(await unreadable.get(), null);
  assert.equal(statSync(deps.filePath).isDirectory(), true, 'the unreadable bytes are preserved');

  // Counterpart: the same deps with no file at all (ENOENT) DO import once —
  // and only then remove the legacy sources.
  const absentDeps = makeDeps(t);
  let absentReads = 0;
  let absentCleanups = 0;
  const imported = await ConnectionStore.open({
    ...absentDeps,
    readLegacy: async () => {
      absentReads += 1;
      return JSON.stringify({ endpointUrl: ENDPOINT, apiKey: 'sk-legacy', active: true });
    },
    cleanupLegacy: async () => {
      absentCleanups += 1;
    },
  });
  assert.equal(absentReads, 1, 'an absent store is the one-time legacy import trigger');
  assert.equal(absentCleanups, 1, 'the successful migration removes the legacy sources');
  assert.equal(imported.getAuth()?.apiKey, 'sk-legacy');
});

test('a legacy import with an unsupported endpoint persists nothing and keeps the original bytes', async (t) => {
  const deps = makeDeps(t);
  const legacyPath = join(deps.filePath, '..', 'connection_config.json');
  writeFileSync(legacyPath, JSON.stringify({ endpointUrl: 'nexus://app', apiKey: 'sk-legacy' }));
  let cleanups = 0;
  const store = await ConnectionStore.open({
    ...deps,
    readLegacy: async () => readFileSync(legacyPath, 'utf8'),
    cleanupLegacy: async () => {
      cleanups += 1;
    },
  });

  assert.equal(await store.get(), null);
  assert.equal(store.getAuth(), null);
  assert.equal(
    existsSync(deps.filePath),
    false,
    'no store may be created from an unsupported legacy endpoint',
  );
  assert.equal(cleanups, 0, 'a refused legacy import removes no legacy source');
  assert.ok(readFileSync(legacyPath, 'utf8').includes('sk-legacy'), 'legacy recovery bytes stay untouched');
});

// ---------------------------------------------------------------------------
// File permissions
// ---------------------------------------------------------------------------

test('store directory and file use 0700/0600', async (t) => {
  const deps = makeDeps(t);
  const store = await ConnectionStore.open(deps);
  await store.set({ ...BASE_CONFIG }, { action: 'replace', value: 'sk-1' });
  const dirMode = statSync(join(deps.filePath, '..')).mode & 0o777;
  const fileMode = statSync(deps.filePath).mode & 0o777;
  assert.equal(dirMode, 0o700);
  assert.equal(fileMode, 0o600);
});
