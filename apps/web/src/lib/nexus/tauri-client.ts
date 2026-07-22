/**
 * `TauriClient` — V1.66 desktop shell `NexusClient` implementation.
 *
 * Spec: [desktop-shell.md](../../../../../.mstar/specs/desktop-shell.md)
 * §5; compass §5 #1 LOCKED. Architecture: **thin desktop-augmentation over
 * `BrowserClient`** — the `NexusClient` data methods reuse the identical HTTP
 * transport to the localhost daemon (`http://localhost:<resolvedPort>/v1/daemon/*`,
 * or same-origin via the Vite proxy when the SPA is on `:5173`), exactly as
 * `BrowserClient` does in the browser-tab flow. The Tauri webview can `fetch`
 * loopback directly (compass §5 #4 — no `http` plugin), so no Tauri `invoke` is
 * needed for data access.
 *
 * Method count: see the `NexusClient` interface (`types.ts`) for the canonical
 * count — it grows as daemon surfaces are promoted, so a literal number here
 * would drift (it was previously stale at 24).
 *
 * Selection: the client factory ({@link ../client-context.tsx}) picks
 * `TauriClient` when {@link isDesktopBuild} is `true`, else `BrowserClient`.
 * `TauriClient` itself does not self-guard against browser instantiation — it is
 * a real HTTP client that works wherever `fetch` does; the factory is the single
 * selection point (§5 #7).
 *
 * Desktop-only capability extensions (`openWith`, `revealInFinder`, daemon
 * lifecycle) live on a separate `DesktopCapabilities` object
 * (`desktop-capabilities.ts`), not on this class — per the spec's "equivalent
 * capability object" wording (keeps `NexusClient` pure transport).
 */
import { BrowserClient, type BrowserClientOptions } from './browser-client';

declare global {
  interface Window {
    /** Injected by the Tauri desktop shell before the SPA loads (authoritative). */
    __NEXUS_DAEMON_PORT__?: number;
  }
}

/**
 * Resolve the desktop daemon port (compass §5 #3 LOCKED).
 *
 * Order: explicit `port` argument → `window.__NEXUS_DAEMON_PORT__` (injected by
 * the Tauri shell from the Rust-resolved value) → `NEXUS_DAEMON_PORT` env var
 * (dev/browser fallback) → `8420`. The injected global is authoritative because
 * `process.env` is unavailable inside the Tauri webview; the Rust launcher and
 * SPA must agree on the same port when `NEXUS_DAEMON_PORT` is overridden
 * (daemon-runtime.md §12.3).
 */
export function resolveDesktopPort(explicit?: number | string): number {
  if (explicit !== undefined && explicit !== '') {
    const n = Number(explicit);
    if (Number.isInteger(n) && n > 0 && n < 65536) return n;
  }
  if (typeof window !== 'undefined' && typeof window.__NEXUS_DAEMON_PORT__ === 'number') {
    return window.__NEXUS_DAEMON_PORT__;
  }
  const fromEnv =
    typeof process !== 'undefined' ? process.env?.NEXUS_DAEMON_PORT : undefined;
  if (fromEnv) {
    const n = Number(fromEnv);
    if (Number.isInteger(n) && n > 0 && n < 65536) return n;
  }
  return 8420;
}

/**
 * True when the SPA is served from the Vite dev/preview origin (`:5173`).
 *
 * `pnpm run dev:desktop` loads the built SPA via `vite preview` on
 * `http://localhost:5173`. Relative `/v1/daemon/*` must stay same-origin so the
 * preview proxy (see `vite.config.ts`) can forward to the daemon — direct
 * `fetch` to `http://127.0.0.1:<port>` is a cross-origin call that WebKit can
 * fail even when `curl` against the daemon succeeds.
 */
export function isViteDevOrigin(): boolean {
  if (typeof window === 'undefined') return false;
  const { protocol, hostname, port } = window.location;
  if (protocol !== 'http:' && protocol !== 'https:') return false;
  if (hostname !== 'localhost' && hostname !== '127.0.0.1') return false;
  return port === '5173';
}

/**
 * Default desktop transport origin.
 *
 * - Vite `:5173` → empty string (same-origin + preview/dev proxy).
 * - Packaged / embedded SPA → `http://localhost:<port>` (prefer `localhost`
 *   over `127.0.0.1` so the host matches the daemon allowlist family and avoids
 *   localhost↔127.0.0.1 cross-host quirks in WKWebView).
 */
export function resolveDesktopBaseUrl(port: number): string {
  if (isViteDevOrigin()) return '';
  return `http://localhost:${port}`;
}

export interface TauriClientOptions {
  /**
   * Override the daemon origin. When omitted the client targets the resolved
   * local loopback port (`http://localhost:<port>`), or same-origin when the
   * SPA is on the Vite `:5173` origin. Set this to connect a desktop build to
   * a remote daemon (V1.92 P1).
   */
  baseUrl?: string;
  /**
   * API key for remote daemon access. Ignored for loopback connections unless
   * an explicit `baseUrl` is provided.
   */
  apiKey?: string;
  /** Override the daemon port (defaults to resolved port per §5 #3). */
  port?: number;
  /** Optional fetch implementation (testing injection, mirroring BrowserClient). */
  fetchImpl?: typeof fetch;
}

/**
 * Desktop `NexusClient`. Inherits all `NexusClient` data methods from
 * `BrowserClient` unchanged; only the constructor fixes the transport origin to
 * the resolved desktop loopback port, or to an explicit remote `baseUrl` for
 * the P1 connection model. This is the thinnest possible impl — zero method
 * duplication, the entire V1.64/V1.65 HTTP surface reused wholesale.
 */
export class TauriClient extends BrowserClient {
  readonly port: number | undefined;

  constructor(options: TauriClientOptions = {}) {
    const port = options.baseUrl ? undefined : resolveDesktopPort(options.port);
    const browserOptions: BrowserClientOptions = {
      baseUrl:
        options.baseUrl ?? (port === undefined ? undefined : resolveDesktopBaseUrl(port)),
      apiKey: options.apiKey,
    };
    if (options.fetchImpl) browserOptions.fetchImpl = options.fetchImpl;
    super(browserOptions);
    this.port = port;
  }
}
