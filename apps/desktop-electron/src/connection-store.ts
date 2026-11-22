/**
 *
 * Encrypted main-process store for the remote connection config, replacing
 * the legacy OS-keychain-with-plaintext-fallback (D-18):
 *
 * - Secrets are encrypted with Electron `safeStorage` before touching disk;
 *   the renderer only ever receives the redacted public projection.
 * - NO plaintext write fallback: when encryption is unavailable the
 *   operation fails with a structured error and nothing is written.
 * - One-time legacy import reads the old keychain/app-data config, encrypts
 *   and persists it, and only then removes both plaintext originals; an
 *   interrupted cleanup is finished by the next open of the encrypted store.
 * - Cleanup is best-effort after that point: a refusal preserves the readable
 *   encrypted store and the remaining legacy source, never blocks activation,
 *   and is reported as the sanitized `legacy_credential_cleanup_failed`.
 * - Clear publishes the durable tombstone marker BEFORE it removes the store,
 *   so a refused marker leaves the credential in place and reports failure, and
 *   a later open sees the marker and never re-imports the legacy material
 *   (D-18).
 * - `set` and `delete` run one at a time through a per-store gate, so a save can
 *   never overwrite a completed clear and a later save always derives from what
 *   the previous one published.
 * - Writes are published atomically and policy-preserving through the native
 *   publisher; a failed encrypt or a refused target leaves the previous state
 *   fully intact. The plaintext key never reaches an error or a log.
 *
 * Host-agnostic: Electron's `safeStorage` is injected as an adapter so the
 * module builds and tests headless under plain Node.
 */

import { mkdirSync, readFileSync, rmSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { publishDocument } from '@42ch/nexus-native';
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
   * (`nexus42` / `connection_config`) or the old app-data
   * `connection_config.json` and returns the raw legacy JSON string. Must
   * never log or return the secret anywhere else.
   *
   * The import runs only when {@link cleanupLegacy} is supplied too: a
   * migration that cannot remove the plaintext originals is never performed.
   */
  readLegacy?: () => Promise<string | null>;
  /**
   * app-data JSON); an already-absent source counts as removed. Injected by
   * the host so this module keeps no keychain/platform knowledge.
   *
   * Best-effort: it is called only after the encrypted store is persisted and
   * readable, and again on every open of an authoritative encrypted store, so
   * an interrupted cleanup is finished by the ordinary reopen. A refusal is
   * reported through {@link ConnectionStore.legacyCleanupFailure} and never
   * blocks activation or rolls the store back.
   */
  cleanupLegacy?: () => Promise<void>;
  /**
   * removal failure is observable in tests; any failure other than a
   * confirmed-absent file makes {@link ConnectionStore.delete} throw.
   */
  removeFile?: (filePath: string) => void;
  /**
   * only so a regression can hold one publication open and overlap two
   * mutations; every production path uses the native publisher.
   */
  publish?: PublishDocument;
}

/** Publishes complete `contents` at the absolute `filePath`. */
type PublishDocument = (filePath: string, contents: string) => Promise<void>;

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

/**
 *
 * The publisher probes the target for write access, captures its access
 * policy, stages an owner-only copy in a sealed directory beside the target and
 * renames it after reapplying owner/group/ACL/mode — so a write-denied target
 * refuses and keeps its previous bytes, and an existing `0640` store keeps its
 * policy. A refusal is `secure_store_publish_refused` naming at most an errno
 * token: neither the plaintext key nor the ciphertext ever reaches the error.
 */
async function atomicWrite(
  publish: PublishDocument,
  filePath: string,
  contents: string,
): Promise<void> {
  try {
    mkdirSync(dirname(filePath), { recursive: true, mode: 0o700 });
    await publish(filePath, contents);
  } catch (err) {
    // the absolute target path and the whole CoreError envelope, and the store's
    // failures reach logs a user can read.
    const error = err as { code?: unknown; details?: { code?: unknown } } | null;
    const wire = error?.details?.code ?? error?.code;
    const detail = typeof wire === 'string' ? wire : 'target refused';
    throw desktopError(
      'secure_store_publish_refused',
      `connection store could not be published (${detail})`,
    );
  }
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
  /**
   * read → derive → publish → state-update sequence inside it, so the previous
   * state they derive from is the state the earlier mutation committed.
   */
  private tail: Promise<unknown> = Promise.resolve();

  private constructor(
    private readonly deps: ConnectionStoreDeps,
    private state: StoredFile | null,
    /**
     * cleanup attempt, or null when there was nothing to remove or the
     * removal succeeded — i.e. non-null means this open did NOT end with the
     * legacy plaintext sources removed.
     *
     * Cleanup is best-effort by contract: the encrypted store is already
     * authoritative, so a refusal is reported here instead of failing the
     * open, and the next open retries it.
     */
    readonly legacyCleanupFailure: Error | null = null,
    /**
     * last open, or null when the open ended healthy or found no store file
     * at all. Non-null means an existing store was present but could not be
     * activated; its bytes stay on disk for the user to recover.
     *
     * Tri-state: `openFailure === null && state === null` → fresh install
     * (no diagnostic); `openFailure !== null` → unreadable/corrupt
     * (diagnostic); otherwise the store is healthy. Reported by the host;
     * the store itself takes no further action.
     */
    readonly openFailure: Error | null = null,
  ) {}

  /** Run one mutation after the previous one settled; the tail never rejects. */
  private serialize<T>(run: () => Promise<T>): Promise<T> {
    const next = this.tail.catch(() => undefined).then(run);
    this.tail = next.then(
      () => undefined,
      () => undefined,
    );
    return next;
  }

  /**
   * tombstone is present, and the paired legacy source is configured,
   * performs the one-time import in one order: validate → encrypt → persist
   * → remove both legacy originals → publish. A parse/URL/encrypt/persist
   * failure removes nothing, so the plaintext sources stay recoverable.
   *
   * The removal itself is best-effort: a refusal never fails the open (the
   * encrypted store is already persisted and readable), preserves both the
   * ciphertext and the remaining legacy source, and is reported as
   * {@link ConnectionStore.legacyCleanupFailure}.
   *
   * Opening an authoritative encrypted store re-runs the same idempotent
   * cleanup, which finishes a previously interrupted one without a second
   * state format, retry service or pending marker.
   *
   * An EXISTING store that is invalid or unreadable is never activated and —
   * unlike an absent file (ENOENT) — never falls back to the legacy import
   * and never triggers legacy cleanup: its bytes stay on disk for the user to
   * recover by saving over them.
   */
  static async open(deps: ConnectionStoreDeps): Promise<ConnectionStore> {
    let state: StoredFile | null = null;
    try {
      state = ConnectionStore.readFile(deps);
    } catch (err) {
      // rewritten, and never replaced by the legacy material. The typed
      // classification is retained in `openFailure` so the host can surface
      // it; the inactive-store return contract and the on-disk bytes are
      // unchanged.
      return new ConnectionStore(deps, null, null, err as Error);
    }
    let cleared = false;
    try {
      readFileSync(join(dirname(deps.filePath), CLEARED_MARKER_NAME), 'utf8');
      cleared = true;
    } catch {
      cleared = false;
    }
    let cleanupFailure: Error | null = null;
    if (state === null && deps.readLegacy && deps.cleanupLegacy && !cleared) {
      const legacy = await deps.readLegacy();
      if (legacy !== null) {
        const imported = ConnectionStore.importLegacy(deps, legacy);
        if (imported !== null) {
          // readable; no failure above can have removed either original.
          await ConnectionStore.persist(deps, imported);
          cleanupFailure = await ConnectionStore.removeLegacy(deps);
          state = imported;
        }
      }
    } else if (state !== null) {
      cleanupFailure = await ConnectionStore.removeLegacy(deps);
    }
    return new ConnectionStore(deps, state, cleanupFailure);
  }

  /**
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

  /** Serialize + publish. Caller passes the complete next state. */
  private static async persist(deps: ConnectionStoreDeps, state: StoredFile): Promise<void> {
    const serialized = JSON.stringify(state);
    if (state.credential !== undefined) {
      deps.storage.decryptString(decodeCredential(state.credential));
    }
    await atomicWrite(deps.publish ?? publishDocument, deps.filePath, serialized);
  }

  /**
   * sanitized structured `legacy_credential_cleanup_failed` error. It is
   * reached ONLY once the encrypted store is persisted and readable (or was
   * already authoritative), so a refusal never rolls the store back, never
   * fails the open and never writes plaintext.
   *
   * The returned error is the caller-visible signal that this open did not
   * end with the plaintext originals removed — so a migration with a refused
   * cleanup is never reported as clean — and the next open retries.
   *
   * The adapter's own error is NOT propagated: it may carry command output or
   * the secret itself. Only an errno-shaped token or an exit status is kept.
   */
  private static async removeLegacy(deps: ConnectionStoreDeps): Promise<Error | null> {
    try {
      await deps.cleanupLegacy?.();
      return null;
    } catch (err) {
      // adapter's own message may carry command output or the secret.
      const code = (err as { code?: unknown } | null)?.code;
      const detail =
        typeof code === 'number' && Number.isInteger(code)
          ? String(code)
          : typeof code === 'string' && /^[A-Z][A-Z0-9_]{0,31}$/.test(code)
            ? code
            : null;
      return desktopError(
        'legacy_credential_cleanup_failed',
        `legacy plaintext credentials could not be removed${detail === null ? '' : ` (${detail})`}; ` +
          'the encrypted store is used and the cleanup is retried on the next open',
      );
    }
  }

  /** Redacted public projection; the API key is NEVER included. */
  async get(): Promise<PublicConnectionConfig | null> {
    return this.state ? { ...this.state.config } : null;
  }

  /**
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
   *
   * - an unsupported endpoint is refused BEFORE any effect: nothing is
   *   encrypted, nothing is written and the previous state survives;
   * - `replace` sets the key (empty string explicitly clears it);
   * - `keep` retains the stored key ONLY when the endpoint is unchanged —
   *   an endpoint change never carries the old endpoint's credential;
   * - if encryption is unavailable the operation throws and the previous
   *   state survives untouched (no partial writes, no plaintext fallback).
   *
   * Runs after every mutation already queued on this store, so `keep` sees the
   * key an earlier `replace` published instead of restoring the replaced one.
   */
  async set(
    config: PublicConnectionConfig,
    credential: ConnectionCredentialUpdate,
  ): Promise<PublicConnectionConfig> {
    return this.serialize(async () => {
      connectionEndpointOrigin(config.endpointUrl);
      const previous = this.state;
      let nextCredential: string | undefined;
      if (credential.action === 'replace') {
        nextCredential =
          credential.value.length > 0
            ? encodeCredential(ConnectionStore.encrypt(this.deps, credential.value))
            : undefined;
      } else {
        if (!previous?.credential) {
          nextCredential = undefined;
        } else if (sameEndpoint(previous.config, config)) {
          nextCredential = previous.credential;
        } else {
          nextCredential = undefined;
        }
      }
      const next: StoredFile = {
        version: 1,
        config: { ...config, hasApiKey: nextCredential !== undefined },
        credential: nextCredential,
      };
      await ConnectionStore.persist(this.deps, next);
      this.state = next;
      return { ...next.config };
    });
  }

  /**
   * the store so a later {@link ConnectionStore.open} sees the marker and
   * never re-imports the legacy material (D-18): clear is a deliberate user
   * action and must not be undone by the still-present legacy key. The
   * marker is non-secret.
   *
   * The marker is published BEFORE the store file is removed. The opposite
   * order leaves neither store nor tombstone when the marker publication is
   * refused, and the next open then re-imports the legacy material — a clear
   * the user was told had failed would be quietly revoked.
   *
   * Resolves only when the material is actually gone: a refused tombstone
   * publication (or a removal failure — permissions, I/O, …) throws and the
   * in-memory state is left intact, so no false "cleared" is reported while
   * bytes remain on disk. An already-absent file counts as removed. A refused
   * tombstone surfaces as `secure_store_publish_refused`, a refused removal as
   * `secure_store_delete_failed`.
   *
   * Runs after every mutation already queued on this store, so an in-flight
   * save cannot republish the store after this clear reported success.
   */
  async delete(): Promise<void> {
    return this.serialize(async () => {
      await atomicWrite(
        this.deps.publish ?? publishDocument,
        join(dirname(this.deps.filePath), CLEARED_MARKER_NAME),
        '{"cleared":true}\n',
      );
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
      this.state = null;
    });
  }
}
