/**
 * Desktop bridge seam (v1.192 P0-T8) — the ONLY module in the SPA that reads
 * `window.nexusDesktop`, the typed preload bridge the Electron shell exposes
 * (contract: `apps/desktop-electron/src/desktop-contract.ts`, imported here
 * **types-only** — no Electron runtime dependency in the web bundle).
 *
 * Selection (compass §5 #7 LOCKED, parity row 27): a valid
 * `window.nexusDesktop.version === 1` marks the desktop runtime. Everything
 * else — browser tab, missing/foreign bridge — runs browser mode. There is no
 * Tauri fallback: the Tauri baseline is rollback source only until P2, not a
 * supported concurrent runtime.
 *
 * The bridge is immutable nonsecret metadata + a typed invoke closed over the
 * frozen operation union; the SPA never sees raw IPC channels, event objects
 * or the persisted API key (D-18 redacted load).
 */
import type {
  ConnectionCredentialUpdate,
  DesktopBridge,
  DesktopOperation,
  DesktopOperationPayload,
  DesktopOperationResult,
  PublicConnectionConfig,
} from '../../../../desktop-electron/src/desktop-contract';

export type {
  ConnectionCredentialUpdate,
  DesktopBridge,
  PublicConnectionConfig,
};

/**
 * The desktop bridge when running inside the Electron shell, else `null`.
 * Cheap and synchronous — safe to call at the client factory (no async race).
 */
export function getDesktopBridge(): DesktopBridge | null {
  if (typeof window === 'undefined') return null;
  const bridge = window.nexusDesktop;
  if (bridge?.version !== 1) return null;
  return bridge;
}

/** True when the typed preload bridge (version 1) is present. */
export function hasDesktopBridge(): boolean {
  return getDesktopBridge() !== null;
}

/**
 * Invoke one frozen desktop operation through the typed bridge. Fails fast
 * with a plain `Error` when the bridge is absent (browser defensive path);
 * main-side failures arrive as `Error` objects carrying a machine-readable
 * `code` own-property (the preload's typed error envelope).
 */
export async function invokeDesktop<O extends DesktopOperation>(
  operation: O,
  ...args: DesktopOperationPayload[O] extends undefined
    ? []
    : [payload: DesktopOperationPayload[O]]
): Promise<DesktopOperationResult[O]> {
  const bridge = getDesktopBridge();
  if (!bridge) {
    throw new Error(
      'Desktop bridge unavailable: window.nexusDesktop (version 1) is not present.',
    );
  }
  return bridge.invoke(operation, ...args);
}
