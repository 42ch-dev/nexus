/**
 * Nexus Local API client adapter — the single transport boundary for the SPA.
 *
 * Import the {@link NexusClient} interface and pick an implementation based on
 * the host (browser now, Tauri webview in V1.65). Screens must depend only on
 * the interface, never on a concrete client.
 *
 * @see {@link ./types.ts} for the interface contract and pending-contracts notes.
 */
export { BrowserClient, type BrowserClientOptions } from './browser-client';
export { NexusClientError, type NexusErrorBody } from './errors';
export { TauriClient } from './tauri-client';
export type { DaemonHealth, NexusClient } from './types';
