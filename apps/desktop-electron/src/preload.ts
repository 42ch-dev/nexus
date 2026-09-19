/**
 * Product preload bridge (v1.192 P0-T1) — the ONLY renderer surface:
 * `window.nexusDesktop` version 1.
 *
 * - No raw ipcRenderer exposure, no Node/process/env handles, no arbitrary
 *   channel invocation — invoke is typed to the frozen operation union.
 * - `runtime` is immutable nonsecret metadata fetched synchronously from main
 *   (trusted additional argument) BEFORE the bridge is exposed; main must
 *   answer `nexus:desktop:runtime` before creating the window (P0-T7).
 * - `onStatusChanged` never forwards Electron event objects to the renderer
 *   and re-bounds status frames before delivery.
 *
 * Why this file does not import `desktop-contract` (not even type-only):
 * the sandboxed preload compiles CommonJS, and the package build runs
 * `tsc -p tsconfig.json && tsc -p tsconfig.preload.json` with both projects
 * emitting into `dist/`. Any reference to the contract source pulls it into
 * the preload program and re-emits it as CommonJS, overwriting the ESM
 * artifact the main process loads at runtime (invalid under `type: module`).
 * The bridge's canonical TYPE (`DesktopBridge`, `Window.nexusDesktop`) lives
 * in `desktop-contract.ts` and governs the SPA side; the mirrors below are
 * the mechanical preload-side implementation values, kept in lockstep by the
 * channel-parity test in tests/desktop-security.test.mjs.
 */
/// <reference lib="dom" />
import { contextBridge, ipcRenderer } from 'electron';

// --- mirrors of desktop-contract.ts (canonical definitions live there) ---
const DESKTOP_INVOKE_CHANNEL = 'nexus:desktop:invoke' as const;
const DESKTOP_STATUS_CHANNEL = 'nexus:desktop:status-changed' as const;
const DESKTOP_RUNTIME_CHANNEL = 'nexus:desktop:runtime' as const;
const MAX_STATUS_BYTES = 4 * 1024;
const MAX_DIAGNOSTIC_BYTES = 2 * 1024;
const MAX_URL_BYTES = 8192;

const DESKTOP_OPERATIONS = [
  'open_with',
  'reveal_in_finder',
  'open_external_url',
  'pick_directory',
  'get_workspace_root',
  'set_workspace_path',
  'switch_active_creator',
  'ensure_setup_bootstrap',
  'get_entrance',
  'set_entrance',
  'get_setup_completed',
  'set_setup_completed',
  'get_agent_profile',
  'set_agent_profile',
  'get_connection_config',
  'set_connection_config',
  'delete_connection_config',
  'get_daemon_status',
  'start_daemon',
  'stop_daemon',
  'restart_daemon',
  'reset_local_database',
  'toggle_maximize_window',
] as const;
type DesktopOperation = (typeof DESKTOP_OPERATIONS)[number];

type DaemonStatus = {
  state: 'starting' | 'running' | 'degraded' | 'stopped' | 'error';
  version?: string;
  port: number;
  detail?: string;
};

type DesktopBridge = {
  version: 1;
  runtime: { localEndpoint: string };
  invoke(operation: DesktopOperation, payload?: unknown): Promise<unknown>;
  onStatusChanged(listener: (status: DaemonStatus) => void): () => void;
};
// --- end mirrors ---

const encoder = new TextEncoder();

/** NUL, C0 controls and DEL are rejected (mirrors desktop-contract.ts). */
function hasControlChars(value: string): boolean {
  for (let i = 0; i < value.length; i += 1) {
    const code = value.charCodeAt(i);
    if (code < 0x20 || code === 0x7f) return true;
  }
  return false;
}

function typedError(code: string, message: string): Error {
  const err = new Error(message) as Error & { code: string };
  err.code = code;
  return err;
}

/** Trusted runtime metadata, supplied by main before the window is created. */
function readTrustedRuntime(): { localEndpoint: string } {
  let raw: unknown;
  try {
    raw = ipcRenderer.sendSync(DESKTOP_RUNTIME_CHANNEL);
  } catch (err) {
    throw typedError('runtime_unavailable', `desktop runtime metadata unavailable: ${String(err)}`);
  }
  if (!raw || typeof raw !== 'object') {
    throw typedError('runtime_unavailable', 'desktop runtime metadata unavailable');
  }
  const { localEndpoint } = raw as { localEndpoint?: unknown };
  if (typeof localEndpoint !== 'string' || localEndpoint.length === 0) {
    throw typedError('runtime_unavailable', 'desktop runtime metadata malformed');
  }
  if (encoder.encode(localEndpoint).length > MAX_URL_BYTES || hasControlChars(localEndpoint)) {
    throw typedError('runtime_unavailable', 'desktop runtime metadata malformed');
  }
  let parsed: URL;
  try {
    parsed = new URL(localEndpoint);
  } catch {
    throw typedError('runtime_unavailable', 'desktop runtime metadata malformed');
  }
  if ((parsed.protocol !== 'http:' && parsed.protocol !== 'https:') || !parsed.host) {
    throw typedError('runtime_unavailable', 'desktop runtime metadata malformed');
  }
  return { localEndpoint };
}

function isDesktopResponse(
  value: unknown,
): value is {
  request_id: string;
  ok: boolean;
  result?: unknown;
  error?: { code: string; message: string };
} {
  if (!value || typeof value !== 'object') return false;
  const body = value as { request_id?: unknown; ok?: unknown };
  return typeof body.request_id === 'string' && typeof body.ok === 'boolean';
}

/** Preload-side bound re-check; main already validated before sending. */
function isBoundedStatusFrame(raw: unknown): raw is DaemonStatus {
  if (!raw || typeof raw !== 'object') return false;
  const body = raw as { state?: unknown; version?: unknown; port?: unknown; detail?: unknown };
  if (typeof body.state !== 'string' || typeof body.port !== 'number') return false;
  if (body.detail !== undefined && typeof body.detail !== 'string') return false;
  if (typeof body.detail === 'string' && encoder.encode(body.detail).length > MAX_DIAGNOSTIC_BYTES) {
    return false;
  }
  return encoder.encode(JSON.stringify(raw)).length <= MAX_STATUS_BYTES;
}

function invoke(operation: DesktopOperation, payload?: unknown): Promise<unknown> {
  if (!DESKTOP_OPERATIONS.includes(operation)) {
    return Promise.reject(typedError('invalid_input', `unsupported operation: ${String(operation)}`));
  }
  const request_id = crypto.randomUUID();
  const envelope =
    payload === undefined ? { request_id, operation } : { request_id, operation, payload };
  return ipcRenderer.invoke(DESKTOP_INVOKE_CHANNEL, envelope).then((raw: unknown) => {
    if (!isDesktopResponse(raw)) {
      throw typedError('internal', 'malformed desktop response envelope');
    }
    if (raw.request_id !== request_id) {
      throw typedError('internal', 'desktop response id mismatch');
    }
    if (raw.ok) return raw.result;
    throw typedError(raw.error?.code ?? 'internal', raw.error?.message ?? 'desktop request failed');
  });
}

const bridge: DesktopBridge = {
  version: 1,
  runtime: readTrustedRuntime(),
  invoke,
  onStatusChanged(listener) {
    const handler = (_event: unknown, raw: unknown): void => {
      // Invalid/oversized frames are dropped, never delivered to the SPA, and
      // Electron event objects never cross this boundary.
      if (isBoundedStatusFrame(raw)) listener(raw);
    };
    ipcRenderer.on(DESKTOP_STATUS_CHANNEL, handler);
    return () => {
      ipcRenderer.removeListener(DESKTOP_STATUS_CHANNEL, handler);
    };
  },
};

contextBridge.exposeInMainWorld('nexusDesktop', bridge);
