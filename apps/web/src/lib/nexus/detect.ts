/**
 * Desktop capability detection (compass §5 #7 LOCKED; parity row 27).
 *
 * `apps/web` is a single bundle served both as a browser tab (daemon-served)
 * and inside the Electron shell (packaged `nexus://app` or the Vite dev
 * origin). The two modes are distinguished at **runtime**, not build time, so
 * detection is a runtime signal.
 *
 * Resolution (checked **once** at the client factory, never scattered across
 * screens):
 *   1. Explicit `NEXUS_DESKTOP` override (build flag via Vite `define`, or a
 *      runtime global the shell can inject). Primary signal per §5 #7.
 *   2. Valid typed preload bridge — `window.nexusDesktop.version === 1`
 *      (see {@link ./desktop-bridge.ts}). No Tauri runtime marker remains:
 *      the Tauri baseline is rollback source until P2, not a supported
 *      concurrent runtime.
 *
 * Browser build → `false` → `BrowserClient`. Desktop build → `true` →
 * `DesktopClient` + `ElectronDesktopCapabilities`.
 */
import { getDesktopBridge } from './desktop-bridge';

declare global {
  interface Window {
    NEXUS_DESKTOP?: boolean;
  }
}

/** Vite exposes `import.meta.env.*`; declare the optional desktop flag. */
interface NexusImportMetaEnv {
  NEXUS_DESKTOP?: boolean;
}

/**
 * `true` only when running inside the Electron desktop shell (or when the
 * `NEXUS_DESKTOP` flag is explicitly set). Use at the client factory, not in
 * screen components — screens consume the `DesktopCapabilities` context which
 * is `null` in browser mode.
 */
export function isDesktopBuild(): boolean {
  // 1. Explicit flag (build-time via Vite `define` or runtime global).
  const env = (import.meta as unknown as { env?: NexusImportMetaEnv }).env;
  const flag =
    env?.NEXUS_DESKTOP ??
    (typeof window !== 'undefined' ? window.NEXUS_DESKTOP : undefined);
  if (flag === true) return true;

  // 2. Typed preload bridge (the real runtime signal for a shared bundle).
  return getDesktopBridge() !== null;
}
