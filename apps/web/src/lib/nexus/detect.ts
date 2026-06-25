/**
 * Desktop capability detection (compass §5 #7 LOCKED).
 *
 * `apps/web` is a single bundle served both as a browser tab (daemon-served via
 * rust-embed) and inside the Tauri webview (`build.frontendDist`). The two modes
 * are distinguished at **runtime**, not build time, so detection must be a
 * runtime signal — not a compile-time flag that would require two builds.
 *
 * Resolution (checked **once** at the client factory, never scattered across
 * screens):
 *   1. Explicit `NEXUS_DESKTOP` override (build flag via Vite `define`, or a
 *      runtime global Tauri can inject). Primary signal per §5 #7.
 *   2. `window.__TAURI_INTERNALS__` presence — the authoritative runtime marker
 *      that `@tauri-apps/api/core`'s `isTauri()` also checks. `app.withGlobalTauri`
 *      is set `true` in `tauri.conf.json` so the full `window.__TAURI__` namespace
 *      (incl. `core.invoke`) is available; this internal key is present in every
 *      Tauri v2 webview regardless of that flag.
 *
 * Browser build → `false` → `BrowserClient`. Desktop build → `true` →
 * `TauriClient` + desktop capability object.
 */

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
    NEXUS_DESKTOP?: boolean;
  }
}

/** Vite exposes `import.meta.env.*`; declare the optional desktop flag. */
interface NexusImportMetaEnv {
  NEXUS_DESKTOP?: boolean;
}

/**
 * `true` only when running inside the Tauri desktop webview (or when the
 * `NEXUS_DESKTOP` flag is explicitly set). Use at the client factory, not in
 * screen components — screens consume the `DesktopCapabilities` context which is
 * `null` in browser mode.
 */
export function isDesktopBuild(): boolean {
  // 1. Explicit flag (build-time via Vite `define` or runtime global).
  const env = (import.meta as unknown as { env?: NexusImportMetaEnv }).env;
  const flag = env?.NEXUS_DESKTOP ?? (typeof window !== 'undefined' ? window.NEXUS_DESKTOP : undefined);
  if (flag === true) return true;

  // 2. Tauri runtime presence (sanity check + the real signal for a shared bundle).
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}
