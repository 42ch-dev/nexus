/**
 * `DesktopClient` — Electron desktop shell `NexusClient` implementation
 * (v1.192 P0-T8; renamed from `TauriClient`, no aliases).
 *
 * Architecture: **thin desktop-augmentation over `BrowserClient`** — the
 * `NexusClient` data methods reuse the identical HTTP transport to the
 * localhost daemon (`http://localhost:<resolvedPort>/v1/daemon/*`, or
 * same-origin via the Vite proxy when the SPA is on `:5173`), exactly as
 * `BrowserClient` does in the browser-tab flow. The Electron renderer can
 * `fetch` loopback directly under the CSP `connect-src` allowance, so no
 * desktop invoke is needed for data access. Remote (V1.92 P1) connections use
 * an explicit `baseUrl`; the main-owned session hook injects the persisted
 * `X-API-Key` for the saved active endpoint (the renderer never holds it —
 * D-18 redacted load).
 *
 * Method count: see the `NexusClient` interface (`types.ts`) for the canonical
 * count — it grows as daemon surfaces are promoted, so a literal number here
 * would drift.
 *
 * Selection: the client factory ({@link ../client-context.tsx}) picks
 * `DesktopClient` when {@link isDesktopBuild} is `true`, else `BrowserClient`.
 * `DesktopClient` itself does not self-guard against browser instantiation —
 * it is a real HTTP client that works wherever `fetch` does; the factory is
 * the single selection point (§5 #7).
 *
 * Desktop-only capability extensions (`openWith`, `revealInFinder`, daemon
 * lifecycle) live on a separate `DesktopCapabilities` object
 * (`desktop-capabilities.ts`), not on this class — per the spec's "equivalent
 * capability object" wording (keeps `NexusClient` pure transport).
 */
import { BrowserClient, type BrowserClientOptions } from './browser-client';
import { getDesktopBridge } from './desktop-bridge';

/**
 * Resolve the desktop daemon port (compass §5 #3 LOCKED; consumed
 * synchronously from the bridge's trusted runtime metadata).
 *
 * Order: explicit `port` argument → `window.nexusDesktop.runtime.localEndpoint`
 * (populated by main before preload — authoritative, including nondefault
 * ports) → `NEXUS_DAEMON_PORT` env var (dev/browser fallback) → `8420`.
 * The bridge metadata is synchronous, so the factory never races an async
 * status read (parity row 27).
 */
export function resolveDesktopPort(explicit?: number | string): number {
  if (explicit !== undefined && explicit !== '') {
    const n = Number(explicit);
    if (Number.isInteger(n) && n > 0 && n < 65536) return n;
  }
  const localEndpoint = getDesktopBridge()?.runtime.localEndpoint;
  if (localEndpoint) {
    try {
      const n = Number(new URL(localEndpoint).port);
      if (Number.isInteger(n) && n > 0 && n < 65536) return n;
    } catch {
      // Malformed trusted metadata falls through to the env/default chain.
    }
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
 * `fetch` to `http://127.0.0.1:<port>` is a cross-origin call the renderer
 * can fail even when `curl` against the daemon succeeds.
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
 *   localhost↔127.0.0.1 cross-host quirks).
 */
export function resolveDesktopBaseUrl(port: number): string {
  if (isViteDevOrigin()) return '';
  return `http://localhost:${port}`;
}

export interface DesktopClientOptions {
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
 * duplication, the entire HTTP surface reused wholesale.
 */
export class DesktopClient extends BrowserClient {
  readonly port: number | undefined;

  constructor(options: DesktopClientOptions = {}) {
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
