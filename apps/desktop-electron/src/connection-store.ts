/**
 * Connection credential store (v1.192 P0-T5).
 *
 * Encrypted main-process store for the remote connection config, replacing
 * the Tauri-era OS-keychain-with-plaintext-fallback (D-18):
 *
 * - Secrets are encrypted with Electron `safeStorage` before touching disk;
 *   the renderer only ever receives the redacted public projection.
 * - NO plaintext write fallback: when encryption is unavailable the
 *   operation fails with a structured error and nothing is written.
 * - One-time legacy import reads the old keychain/app-data config and
 *   encrypts it before the new store becomes authoritative.
 * - Clear writes a durable tombstone marker: a later open sees the marker
 *   and never re-imports the legacy material (D-18).
 * - Writes are atomic (temp file + rename); a failed encrypt leaves the
 *   previous state fully intact.
 *
 * Host-agnostic: Electron's `safeStorage` is injected as an adapter so the
 * module builds and tests headless under plain Node.
 */

import { mkdirSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import type { ConnectionCredentialUpdate, PublicConnectionConfig } from './desktop-contract.js';
import { connectionEndpointOrigin, desktopError } from './desktop-contract.js';

/** Subset of Electron `safeStorage` this store relies on. */
export interface SecureStorageAdapter {
  isEncryptionAvailable(): boolean;
  encryptString(plain: string): Uint8Array;
  decryptString(encrypted: Uint8Array): string;
}

export interface ConnectionStoreDeps {
  /** Absolute path of the encrypted store file (…/connection-config.enc). */
  filePath: string;
  /** safeStorage adapter (inject a stub in headless tests). */
  storage: SecureStorageAdapter;
  /**
   * One-time legacy import source: reads the old macOS keychain entry
   * (`nexus42` / `connection_config`) or the old app-data
   * `connection_config.json` and returns the raw legacy JSON string. Must
   * never log or return the secret anywhere else. Absent ⇒ no import.
   */
  readLegacy?: () => Promise<string | null>;
  /**
   * Filesystem removal primitive (defaults to `rmSync`). Injectable so a
   * removal failure is observable in tests; any failure other than a
   * confirmed-absent file makes {@link ConnectionStore.delete} throw.
   */
  removeFile?: (filePath: string) => void;
}

interface StoredFile {
  version: 1;
  config: PublicConnectionConfig;
  /** base64(safeStorage(apiKey)) — never plaintext. */
  credential?: string;
}

function encodeCredential(ciphertext: Uint8Array): string {
  return Buffer.from(ciphertext).toString('base64');
}

function decodeCredential(encoded: string): Uint8Array {
  return new Uint8Array(Buffer.from(encoded, 'base64'));
}

function atomicWrite(filePath: string, contents: string): void {
  const dir = dirname(filePath);
  mkdirSync(dir, { recursive: true, mode: 0o700 });
  const tmp = join(dir, `.connection-config.${process.pid}.tmp`);
  writeFileSync(tmp, contents, { mode: 0o600 });
  renameSync(tmp, filePath);
}

function sameEndpoint(a: PublicConnectionConfig, b: PublicConnectionConfig): boolean {
  return a.endpointUrl === b.endpointUrl;
}

/** Non-secret tombstone written next to the store when the user clears it. */
const CLEARED_MARKER_NAME = 'connection-config.cleared';

/**
 * Public projection + main-only credential access. The renderer sees only
 * {@link ConnectionStore.get}; {@link ConnectionStore.getAuth} is main-side
 * injection authority for the network policy and never crosses IPC.
 */
export class ConnectionStore {
  private constructor(
    private readonly deps: ConnectionStoreDeps,
    private state: StoredFile | null,
  ) {}

  /**
   * Open (or create) the store. When no store file exists, no cleared
   * tombstone is present, and a legacy source is configured, performs the
   * one-time import: legacy JSON is validated and encrypted BEFORE the new
   * store is written; originals are left untouched. A failed/unavailable
   * encryption aborts the import without writing anything in the clear.
   *
   * An EXISTING store that is invalid or unreadable is never activated and —
   * unlike an absent file (ENOENT) — never falls back to the legacy import:
   * its bytes stay on disk for the user to recover by saving over them.
   */
  static async open(deps: ConnectionStoreDeps): Promise<ConnectionStore> {
    let state: StoredFile | null = null;
    try {
      state = ConnectionStore.readFile(deps);
    } catch {
      // Invalid/corrupt/unreadable existing store: not activated, not
      // rewritten, and never replaced by the legacy material.
      return new ConnectionStore(deps, null);
    }
    let cleared = false;
    try {
      readFileSync(join(dirname(deps.filePath), CLEARED_MARKER_NAME), 'utf8');
      cleared = true;
    } catch {
      cleared = false;
    }
    if (state === null && deps.readLegacy && !cleared) {
      const legacy = await deps.readLegacy();
      if (legacy !== null) {
        const imported = ConnectionStore.importLegacy(deps, legacy);
        if (imported !== null) {
          ConnectionStore.persist(deps, imported);
          state = imported;
        }
      }
    }
    return new ConnectionStore(deps, state);
  }

  /**
   * Read + validate the store file. ENOENT is the ONLY "absent store" signal;
   * every other read failure is an existing-but-unreadable store and is kept
   * distinct so it can never be mistaken for a fresh install. A stored
   * endpoint must satisfy the shared root-service grammar before it is
   * activated.
   */
  private static readFile(deps: ConnectionStoreDeps): StoredFile | null {
    let raw: string;
    try {
      raw = readFileSync(deps.filePath, 'utf8');
    } catch (err) {
      if ((err as NodeJS.ErrnoException | null)?.code === 'ENOENT') return null; // absent store
      const code = (err as NodeJS.ErrnoException | null)?.code ?? 'unknown error';
      throw desktopError('secure_store_unreadable', `connection store file could not be read (${code})`);
    }
    let parsed: StoredFile;
    try {
      parsed = JSON.parse(raw) as StoredFile;
    } catch {
      throw desktopError('secure_store_corrupt', 'connection store file is not valid JSON');
    }
    if (parsed?.version !== 1 || typeof parsed.config?.endpointUrl !== 'string') {
      throw desktopError('secure_store_corrupt', 'connection store file is not a valid v1 store');
    }
    try {
      connectionEndpointOrigin(parsed.config.endpointUrl);
    } catch {
      throw desktopError(
        'secure_store_corrupt',
        'connection store endpoint is not a supported root service URL',
      );
    }
    if (parsed.credential !== undefined) {
      // Fail fast (before any effect) when the stored credential cannot be
      // decrypted — the store stays recoverable rather than half-loaded.
      try {
        deps.storage.decryptString(decodeCredential(parsed.credential));
      } catch {
        throw desktopError('secure_store_corrupt', 'connection store credential is not decryptable');
      }
    }
    return { version: 1, config: parsed.config, credential: parsed.credential };
  }

  private static importLegacy(deps: ConnectionStoreDeps, legacy: string): StoredFile | null {
    let parsed: unknown;
    try {
      parsed = JSON.parse(legacy);
    } catch {
      return null; // invalid legacy JSON: nothing to import
    }
    if (!parsed || typeof parsed !== 'object') return null;
    const body = parsed as Record<string, unknown>;
    if (typeof body.endpointUrl !== 'string' || body.endpointUrl.length === 0) return null;
    // An unsupported legacy endpoint is never persisted (or encrypted).
    try {
      connectionEndpointOrigin(body.endpointUrl);
    } catch {
      return null;
    }
    const config: PublicConnectionConfig = {
      endpointUrl: body.endpointUrl,
      hasApiKey: typeof body.apiKey === 'string' && body.apiKey.length > 0,
    };
    if (typeof body.label === 'string' && body.label.length > 0) config.label = body.label;
    if (typeof body.active === 'boolean') config.active = body.active;
    if (typeof body.pinnedFingerprint === 'string' && body.pinnedFingerprint.length > 0) {
      config.pinnedFingerprint = body.pinnedFingerprint;
    }
    const credential =
      typeof body.apiKey === 'string' && body.apiKey.length > 0
        ? encodeCredential(ConnectionStore.encrypt(deps, body.apiKey))
        : undefined;
    return credential !== undefined
      ? { version: 1, config, credential }
      : { version: 1, config };
  }

  private static encrypt(deps: ConnectionStoreDeps, plaintext: string): Uint8Array {
    if (!deps.storage.isEncryptionAvailable()) {
      throw desktopError(
        'secure_storage_unavailable',
        'credential encryption is unavailable; refusing to write secrets unencrypted',
      );
    }
    return deps.storage.encryptString(plaintext);
  }

  /** Serialize + atomically replace. Caller passes the complete next state. */
  private static persist(deps: ConnectionStoreDeps, state: StoredFile): void {
    const serialized = JSON.stringify(state);
    // Encrypt BEFORE any disk write so a failure is fully non-destructive.
    if (state.credential !== undefined) {
      deps.storage.decryptString(decodeCredential(state.credential));
    }
    atomicWrite(deps.filePath, serialized);
  }

  /** Redacted public projection; the API key is NEVER included. */
  async get(): Promise<PublicConnectionConfig | null> {
    return this.state ? { ...this.state.config } : null;
  }

  /**
   * Main-side credential injection authority. Returns the active endpoint's
   * exact origin plus plaintext key ONLY for main-process network policy
   * use; never invoke this on a renderer path. Null when the config is
   * inactive or has no credential.
   */
  getAuth(): { endpointOrigin: string; apiKey: string } | null {
    const state = this.state;
    if (state?.config.active !== true || !state?.credential) {
      return null;
    }
    let apiKey: string;
    try {
      apiKey = this.deps.storage.decryptString(decodeCredential(state.credential));
    } catch {
      return null;
    }
    let origin: string;
    try {
      origin = connectionEndpointOrigin(state.config.endpointUrl);
    } catch {
      return null; // fail closed: an unsupported stored endpoint carries no auth
    }
    return { endpointOrigin: origin, apiKey };
  }

  /**
   * Apply a public config update plus the explicit credential update.
   *
   * - an unsupported endpoint is refused BEFORE any effect: nothing is
   *   encrypted, nothing is written and the previous state survives;
   * - `replace` sets the key (empty string explicitly clears it);
   * - `keep` retains the stored key ONLY when the endpoint is unchanged —
   *   an endpoint change never carries the old endpoint's credential;
   * - if encryption is unavailable the operation throws and the previous
   *   state survives untouched (no partial writes, no plaintext fallback).
   */
  async set(
    config: PublicConnectionConfig,
    credential: ConnectionCredentialUpdate,
  ): Promise<PublicConnectionConfig> {
    connectionEndpointOrigin(config.endpointUrl);
    const previous = this.state;
    let nextCredential: string | undefined;
    if (credential.action === 'replace') {
      nextCredential =
        credential.value.length > 0
          ? encodeCredential(ConnectionStore.encrypt(this.deps, credential.value))
          : undefined;
    } else {
      // keep
      if (!previous?.credential) {
        // No stored key and none supplied: valid, simply keyless.
        nextCredential = undefined;
      } else if (sameEndpoint(previous.config, config)) {
        nextCredential = previous.credential;
      } else {
        // Endpoint changed: the old endpoint's credential never carries over.
        nextCredential = undefined;
      }
    }
    const next: StoredFile = {
      version: 1,
      config: { ...config, hasApiKey: nextCredential !== undefined },
      credential: nextCredential,
    };
    ConnectionStore.persist(this.deps, next);
    this.state = next;
    return { ...next.config };
  }

  /**
   * Remove all stored material. Writes a durable cleared tombstone next to
   * the store so a later {@link ConnectionStore.open} sees the marker and
   * never re-imports the legacy material (D-18): clear is a deliberate user
   * action and must not be undone by the still-present legacy key. The
   * marker is non-secret and is created only after the store file is
   * confirmed gone.
   *
   * Resolves only when the material is actually gone: a removal failure
   * (permissions, I/O, …) throws `secure_store_delete_failed` and the
   * in-memory state is left intact so no false "cleared" is reported while
   * bytes remain on disk. An already-absent file counts as removed. A
   * tombstone write failure surfaces the same way.
   */
  async delete(): Promise<void> {
    const remove = this.deps.removeFile ?? rmSync;
    try {
      remove(this.deps.filePath);
    } catch (err) {
      const code = (err as NodeJS.ErrnoException | null)?.code;
      if (code !== 'ENOENT') {
        throw desktopError(
          'secure_store_delete_failed',
          `connection store file could not be removed (${code ?? 'unknown error'})`,
        );
      }
    }
    try {
      atomicWrite(join(dirname(this.deps.filePath), CLEARED_MARKER_NAME), '{"cleared":true}\n');
    } catch (err) {
      const code = (err as NodeJS.ErrnoException | null)?.code;
      throw desktopError(
        'secure_store_delete_failed',
        `cleared tombstone could not be written (${code ?? 'unknown error'})`,
      );
    }
    this.state = null;
  }
}
