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
import { errorCode } from '../dist/desktop-contract.js';

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

test('clear removes stored material; a subsequent import is possible exactly once', async (t) => {
  const deps = makeDeps(t);
  let legacyReads = 0;
  const withLegacy = {
    ...deps,
    readLegacy: async () => {
      legacyReads += 1;
      return JSON.stringify({ endpointUrl: ENDPOINT, apiKey: 'sk-legacy', active: true });
    },
  };
  const store = await ConnectionStore.open(withLegacy);
  assert.equal(legacyReads, 1, 'import runs at open when store absent');
  assert.equal(store.getAuth()?.apiKey, 'sk-legacy');
  assert.equal(existsSync(deps.filePath), true);

  await store.delete();
  assert.equal(existsSync(deps.filePath), false);
  assert.equal(await store.get(), null);

  // Clear itself never reimports; the next open performs the one allowed import.
  const reopened = await ConnectionStore.open(withLegacy);
  assert.equal(legacyReads, 2, 'exactly one more import on next open after clear');
  assert.equal(reopened.getAuth()?.apiKey, 'sk-legacy');
  const again = await ConnectionStore.open(withLegacy);
  assert.equal(legacyReads, 2, 'store now authoritative: no periodic dual-store fallback');
  assert.equal(again.getAuth()?.apiKey, 'sk-legacy');
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

test('legacy import leaves the original untouched and encrypts before switching', async (t) => {
  const deps = makeDeps(t);
  const legacyPath = join(deps.filePath, '..', 'connection_config.json');
  writeFileSync(legacyPath, JSON.stringify({ endpointUrl: ENDPOINT, apiKey: 'sk-legacy' }));
  const store = await ConnectionStore.open({
    ...deps,
    readLegacy: async () => readFileSync(legacyPath, 'utf8'),
  });
  const projection = await store.get();
  assert.equal(projection?.hasApiKey, true);
  assert.ok(!projection || !('apiKey' in projection));
  assert.ok(!readFileSync(deps.filePath, 'utf8').includes('sk-legacy'));
  // Original file still present, unmodified.
  assert.ok(readFileSync(legacyPath, 'utf8').includes('sk-legacy'));
});

test('invalid legacy JSON is skipped without creating a store', async (t) => {
  const deps = makeDeps(t);
  const store = await ConnectionStore.open({ ...deps, readLegacy: async () => 'not json' });
  assert.equal(await store.get(), null);
  assert.equal(existsSync(deps.filePath), false);
});

test('encryption unavailable aborts legacy import with structured error and no plaintext write', async (t) => {
  const deps = makeDeps(t);
  await assert.rejects(
    () =>
      ConnectionStore.open({
        ...deps,
        storage: unavailableStorage,
        readLegacy: async () => JSON.stringify({ endpointUrl: ENDPOINT, apiKey: 'sk-legacy' }),
      }),
    (err) => errorCode(err) === 'secure_storage_unavailable',
  );
  assert.equal(existsSync(deps.filePath), false, 'legacy secret must never be written in the clear');
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
