/**
 * Connection config persistence abstraction (V1.92 P1; v1.192 P0-T8 Electron
 * redacted store).
 *
 * The config shape is a client-side only data model — it is never sent to the
 * daemon as a wire payload. Storage backends differ by platform:
 *   - Web SPA: localStorage (trust boundary equals the SPA itself); the actual
 *     API key is stored and required.
 *   - Electron desktop: main-owned encrypted store behind the typed bridge.
 *     Load returns the **public projection only** (never the API key — D-18);
 *     save passes the ephemeral user-entered key once as a credential update
 *     (`keep` / `replace`, empty value explicitly clears).
 *
 * Spec: daemon-runtime.md §16.1, §16.5.
 */
import {
  getDesktopBridge,
  invokeDesktop,
  type ConnectionCredentialUpdate,
} from './desktop-bridge';

/** Active (or saved-but-inactive) remote connection configuration. */
export interface ConnectionConfig {
  /** Full daemon URL including protocol and port. */
  endpointUrl: string;
  /**
   * User-entered API key; sent as `X-API-Key` on protected requests. Optional:
   * the desktop store never returns the persisted key (redacted load), so a
   * desktop-saved config carries `hasApiKey` instead and an omitted `apiKey`
   * on save means "keep the stored credential". The browser backend requires
   * the actual key.
   */
  apiKey?: string;
  /** Whether a credential is persisted for this endpoint (desktop redacted load). */
  hasApiKey?: boolean;
  /** SHA-256 fingerprint pinned after TOFU confirmation, if any. */
  pinnedFingerprint?: string;
  /** User-visible connection name; defaults to hostname if blank. */
  label?: string;
  /** Whether this config is currently active (false = saved but local mode). */
  active?: boolean;
}

/** Platform-agnostic storage backend for {@link ConnectionConfig}. */
export interface ConnectionStorage {
  load(): Promise<ConnectionConfig | null>;
  save(config: ConnectionConfig): Promise<void>;
  clear(): Promise<void>;
}

const STORAGE_KEY = 'nexus-connection-config-v1';

/**
 * Web backend: persists the connection config in `localStorage`.
 *
 * Trust-boundary note (daemon-runtime.md §16.5):
 * - `localStorage` is readable by any script on the SPA origin, so an XSS
 *   attacker could read the stored `apiKey`.
 * - This is an accepted trade-off for the local-first web build: the API key
 *   is user-entered and the trust boundary is the SPA itself.
 * - The desktop build stores this value in the OS keychain / credential
 *   manager instead; see `apps/desktop/src-tauri/src/connection_config.rs`.
 */
class WebConnectionStorage implements ConnectionStorage {
  async load(): Promise<ConnectionConfig | null> {
    if (typeof window === 'undefined') return null;
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    try {
      const parsed = JSON.parse(raw) as unknown;
      if (!isValidConnectionConfig(parsed)) {
        await this.clear();
        return null;
      }
      return parsed;
    } catch {
      await this.clear();
      return null;
    }
  }

  async save(config: ConnectionConfig): Promise<void> {
    if (typeof window === 'undefined') return;
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(config));
  }

  async clear(): Promise<void> {
    if (typeof window === 'undefined') return;
    window.localStorage.removeItem(STORAGE_KEY);
  }
}

/**
 * Desktop backend: main-owned encrypted store behind the typed bridge (P0-T5).
 *
 * Load returns the **public projection only** — never the persisted API key
 * (D-18); the in-memory config carries `apiKey: ''` plus `hasApiKey`. Save
 * converts an omitted `apiKey` to a `keep` credential update and a present
 * (possibly empty) `apiKey` to `replace` — empty explicitly clears.
 */
class DesktopConnectionStorage implements ConnectionStorage {
  async load(): Promise<ConnectionConfig | null> {
    const pub = await invokeDesktop('get_connection_config');
    if (!pub) return null;
    return { ...pub, apiKey: '' };
  }

  async save(config: ConnectionConfig): Promise<void> {
    const credential: ConnectionCredentialUpdate =
      config.apiKey === undefined
        ? { action: 'keep' }
        : { action: 'replace', value: config.apiKey };
    await invokeDesktop('set_connection_config', {
      config: {
        endpointUrl: config.endpointUrl,
        label: config.label,
        active: config.active,
        pinnedFingerprint: config.pinnedFingerprint,
        hasApiKey: config.apiKey ? true : (config.hasApiKey ?? false),
      },
      credential,
    });
  }

  async clear(): Promise<void> {
    await invokeDesktop('delete_connection_config');
  }
}

/** Validate the raw parsed JSON has the minimal fields we require. */
function isValidConnectionConfig(value: unknown): value is ConnectionConfig {
  if (value === null || typeof value !== 'object') return false;
  const c = value as Record<string, unknown>;
  if (typeof c.endpointUrl !== 'string' || c.endpointUrl.length === 0) return false;
  if (c.apiKey !== undefined && typeof c.apiKey !== 'string') return false;
  if (c.hasApiKey !== undefined && typeof c.hasApiKey !== 'boolean') return false;
  if (c.pinnedFingerprint !== undefined && typeof c.pinnedFingerprint !== 'string') return false;
  if (c.label !== undefined && typeof c.label !== 'string') return false;
  if (c.active !== undefined && typeof c.active !== 'boolean') return false;
  return true;
}

/** Select the appropriate storage backend for the current runtime. */
export function createConnectionStorage(): ConnectionStorage {
  return getDesktopBridge() ? new DesktopConnectionStorage() : new WebConnectionStorage();
}

/** Normalise a user-entered endpoint URL. */
export function normalizeEndpointUrl(input: string): string {
  const trimmed = input.trim();
  if (!trimmed) return '';
  // Strip trailing slashes for consistent storage and display.
  return trimmed.replace(/\/+$/, '');
}

/** Extract a display label / hostname from an endpoint URL. */
export function endpointLabel(input: string, fallback = 'Remote daemon'): string {
  try {
    const url = new URL(input);
    return url.hostname || fallback;
  } catch {
    return fallback;
  }
}
