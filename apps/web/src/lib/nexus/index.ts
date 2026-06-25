/**
 * Nexus Local API client adapter — the single transport boundary for the SPA.
 *
 * Import the {@link NexusClient} interface and the factory picks an
 * implementation based on the host (browser-tab vs Tauri webview). Screens must
 * depend only on the interface, never on a concrete client. Desktop-only
 * capabilities come from `DesktopCapabilities` (null in the browser build).
 *
 * @see {@link ./types.ts} for the interface contract and pending-contracts notes.
 * @see {@link ./desktop-capabilities.ts} for the V1.66 desktop-only surface.
 */
export { BrowserClient, type BrowserClientOptions } from './browser-client';
export { NexusClientError, type NexusErrorBody } from './errors';
export { TauriClient, resolveDesktopPort, type TauriClientOptions } from './tauri-client';
export {
  TauriDesktopCapabilities,
  DESKTOP_CAPABILITIES_UNAVAILABLE,
  type DesktopCapabilities,
  type DesktopCapabilityError,
  type DaemonStatus,
} from './desktop-capabilities';
export { isDesktopBuild } from './detect';
export { normalizeList, sortByDate, type ListArrayKey, type NormalizedList } from './adapters';
export type { DaemonHealth, NexusClient } from './types';
