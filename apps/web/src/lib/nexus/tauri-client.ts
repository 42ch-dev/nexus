/**
 * `TauriClient` — V1.66 desktop shell `NexusClient` implementation.
 *
 * Spec: [desktop-shell.md](../../../../../.mstar/knowledge/specs/desktop-shell.md)
 * §5; compass §5 #1 LOCKED. Architecture: **thin desktop-augmentation over
 * `BrowserClient`** — the 21 `NexusClient` data methods reuse the identical HTTP
 * transport to the localhost daemon (`http://127.0.0.1:<resolvedPort>/v1/local/*`),
 * exactly as `BrowserClient` does in the browser-tab flow. The Tauri webview can
 * `fetch` loopback directly (compass §5 #4 — no `http` plugin), so no Tauri
 * `invoke` is needed for data access.
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

/**
 * Resolve the desktop daemon port (compass §5 #3 LOCKED).
 *
 * Order: explicit `port` argument → `NEXUS_DAEMON_PORT` (if a valid u16) →
 * `8420` (the daemon default, `crates/nexus42/src/config.rs` /
 * `nexus-daemon-runtime/src/boot.rs`). The Tauri app passes `--port <resolved>`
 * to the sidecar in P1 so CLI args + env cannot diverge.
 */
export function resolveDesktopPort(explicit?: number | string): number {
  if (explicit !== undefined && explicit !== '') {
    const n = Number(explicit);
    if (Number.isInteger(n) && n > 0 && n < 65536) return n;
  }
  const fromEnv =
    typeof process !== 'undefined' ? process.env?.NEXUS_DAEMON_PORT : undefined;
  if (fromEnv) {
    const n = Number(fromEnv);
    if (Number.isInteger(n) && n > 0 && n < 65536) return n;
  }
  return 8420;
}

export interface TauriClientOptions {
  /** Override the daemon port (defaults to resolved port per §5 #3). */
  port?: number;
  /** Optional fetch implementation (testing injection, mirroring BrowserClient). */
  fetchImpl?: typeof fetch;
}

/**
 * Desktop `NexusClient`. Inherits all 21 data methods from `BrowserClient`
 * unchanged; only the constructor fixes the transport origin to the resolved
 * desktop loopback port. This is the thinnest possible impl — zero method
 * duplication, the entire V1.64/V1.65 HTTP surface reused wholesale.
 */
export class TauriClient extends BrowserClient {
  readonly port: number;

  constructor(options: TauriClientOptions = {}) {
    const port = resolveDesktopPort(options.port);
    const browserOptions: BrowserClientOptions = {
      baseUrl: `http://127.0.0.1:${port}`,
    };
    if (options.fetchImpl) browserOptions.fetchImpl = options.fetchImpl;
    super(browserOptions);
    this.port = port;
  }
}
