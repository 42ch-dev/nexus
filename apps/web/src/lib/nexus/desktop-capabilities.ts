/**
 * Desktop-only `NexusClient` extensions (compass §5 #1; desktop-shell.md §5).
 *
 * These are the capabilities the browser sandbox cannot perform:
 *   - {@link DesktopCapabilities.openWith} — open a path in the system default editor.
 *   - {@link DesktopCapabilities.revealInFinder} — reveal a path in Finder.
 *   - {@link DesktopCapabilities.getDaemonStatus} / `startDaemon` / `stopDaemon`
 *     — daemon lifecycle via Tauri custom commands backed by the bundled
 *       `nexus42` sidecar (P1).
 *
 * Transport = Tauri custom commands (Tauri IPC), **not** Local API HTTP — so
 * `wire_contracts_changed` stays `false` (compass §5 #5). The commands live in
 * `apps/desktop/src-tauri/src/lib.rs` and enforce the authoritative runtime path
 * guard (§5 #8) for file actions.
 *
 * Browser build: the {@link DesktopCapabilities} context value is `null`, so
 * screens hide the desktop-only affordances (Copy Path stays — it is plain
 * clipboard write, browser + desktop).
 */
import type { DaemonHealth } from './types';

/** Structured error thrown by desktop capability methods. Mirrors the Rust
 * `PathGuardError` shape (`{ code, message }`) so the toast layer can read it
 * uniformly. */
export interface DesktopCapabilityError {
  code:
    | 'workspace_root_unknown'
    | 'path_outside_workspace'
    | 'path_unresolvable'
    | 'invoke_failed'
    | 'not_in_desktop_build';
  message: string;
}

/** Daemon lifecycle state surfaced by `getDaemonStatus` (drives the indicator). */
export interface DaemonStatus {
  /** Coarse lifecycle state (DESIGN.md "Daemon Status Indicator" table). */
  state: 'starting' | 'running' | 'degraded' | 'stopped' | 'error';
  /** Daemon package version when known (carried from the health probe). */
  version?: string;
  /** Resolved loopback port. */
  port: number;
  /** Plain-language recovery/next-step copy (never just a code). */
  detail?: string;
}

/**
 * Desktop-only capability surface. Provided via React context; `null` in browser
 * mode. Screens must depend on this interface (via `useDesktopCapabilities`),
 * never on `window.__TAURI__` directly — that keeps a clean boundary for tests.
 */
export interface DesktopCapabilities {
  /** Open `path` in the system default editor (path-guarded). */
  openWith(path: string): Promise<void>;
  /** Reveal `path` in Finder (path-guarded). */
  revealInFinder(path: string): Promise<void>;
  /**
   * Current daemon lifecycle state. P1 returns the sidecar-controlled state
   * from the Tauri sidecar manager.
   */
  getDaemonStatus(): Promise<DaemonStatus>;
  /** Subscribe to daemon status changes emitted by the Rust sidecar manager. */
  onDaemonStatusChanged(callback: (status: DaemonStatus) => void): Promise<() => void>;
  /** Start/restart the owned sidecar. */
  startDaemon(): Promise<void>;
  /** Stop the owned sidecar. */
  stopDaemon(): Promise<void>;
}

/**
 * The global Tauri namespace shape this adapter calls into. Only `core.invoke`
 * and `event.listen` are used (custom commands + lifecycle events); the opener
 * plugin's JS API is not called directly because the custom commands own the
 * path guard (§5 #8).
 */
interface TauriGlobal {
  core: {
    invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T>;
  };
  event: {
    listen<T>(event: string, handler: (event: { payload: T }) => void): Promise<() => void>;
  };
}

const DAEMON_STATUS_EVENT = 'nexus://daemon-status-changed';

function tauriInvoke(): TauriGlobal {
  // `withGlobalTauri: true` (tauri.conf.json) guarantees `window.__TAURI__` in
  // the desktop webview. Resolved lazily so the browser build never touches it.
  const w = window as unknown as { __TAURI__?: TauriGlobal };
  const tauri = w.__TAURI__;
  if (!tauri?.core?.invoke) {
    throw {
      code: 'invoke_failed',
      message: 'Desktop commands are not available. Is the Nexus shell running?',
    } satisfies DesktopCapabilityError;
  }
  return tauri;
}

function asDesktopError(err: unknown): DesktopCapabilityError {
  // Rust PathGuardError / string errors serialize as `{ code, message }`.
  // Anything else (incl. Tauri invoke transport failures) collapses to
  // `invoke_failed`.
  if (err && typeof err === 'object' && 'code' in err && 'message' in err) {
    const e = err as { code: string; message: string };
    return { code: e.code as DesktopCapabilityError['code'], message: e.message };
  }
  const message = err instanceof Error ? err.message : String(err);
  return { code: 'invoke_failed', message: message || 'Desktop command failed.' };
}

/**
 * Real `DesktopCapabilities` backed by Tauri custom commands. Constructed only
 * when {@link isDesktopBuild} is `true` (the client factory).
 */
export class TauriDesktopCapabilities implements DesktopCapabilities {
  async openWith(path: string): Promise<void> {
    try {
      await tauriInvoke().core.invoke<void>('open_with', { path });
    } catch (err) {
      throw asDesktopError(err);
    }
  }

  async revealInFinder(path: string): Promise<void> {
    try {
      await tauriInvoke().core.invoke<void>('reveal_in_finder', { path });
    } catch (err) {
      throw asDesktopError(err);
    }
  }

  async getDaemonStatus(): Promise<DaemonStatus> {
    try {
      return await tauriInvoke().core.invoke<DaemonStatus>('get_daemon_status');
    } catch (err) {
      throw asDesktopError(err);
    }
  }

  async onDaemonStatusChanged(
    callback: (status: DaemonStatus) => void,
  ): Promise<() => void> {
    try {
      return await tauriInvoke().event.listen<DaemonStatus>(DAEMON_STATUS_EVENT, (event) => {
        callback(event.payload);
      });
    } catch (err) {
      throw asDesktopError(err);
    }
  }

  async startDaemon(): Promise<void> {
    try {
      await tauriInvoke().core.invoke<void>('start_daemon', undefined);
    } catch (err) {
      throw asDesktopError(err);
    }
  }

  async stopDaemon(): Promise<void> {
    try {
      await tauriInvoke().core.invoke<void>('stop_daemon', undefined);
    } catch (err) {
      throw asDesktopError(err);
    }
  }
}

/** Sentinel for the browser build — every capability method reports it is not
 * available. Returned by `useDesktopCapabilities` when not in desktop mode, so
 * screens can branch on `null` rather than catching. */
export const DESKTOP_CAPABILITIES_UNAVAILABLE: DesktopCapabilityError = {
  code: 'not_in_desktop_build',
  message: 'This action is only available in the Nexus desktop app.',
};

/** Re-export for consumers that still need the HTTP health shape elsewhere. */
export type { DaemonHealth };
