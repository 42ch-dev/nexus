/**
 * Desktop capability detection (compass §5 #7 LOCKED; parity row 27).
 *
 * `apps/web` is a single bundle served both as a browser tab (daemon-served)
 * and inside the Electron shell (packaged `nexus://app` or the Vite dev
 * origin). The two modes are distinguished at **runtime**, not build time, so
 * detection is a runtime signal.
 *
 * Resolution (checked **once** at the client factory, never scattered across
 * screens): a valid typed preload bridge — `window.nexusDesktop.version === 1`
 * (see {@link ./desktop-bridge.ts}). Per desktop-shell.md §5 there is
 * deliberately NO build-env override: `NEXUS_DESKTOP` flags, Vite `define`
 * injection and runtime globals cannot select desktop mode — the bridge is
 * the only signal. No Tauri runtime marker remains: the Tauri baseline is
 * rollback source until P2, not a supported concurrent runtime.
 *
 * Browser build → `false` → `BrowserClient`. Desktop build → `true` →
 * `DesktopClient` + `ElectronDesktopCapabilities`.
 */
import { getDesktopBridge } from './desktop-bridge';

/**
 * `true` only when running inside the Electron desktop shell, detected via
 * the typed preload bridge. Use at the client factory, not in screen
 * components — screens consume the `DesktopCapabilities` context which is
 * `null` in browser mode.
 */
export function isDesktopBuild(): boolean {
  return getDesktopBridge() !== null;
}
