/**
 * Desktop-only `NexusClient` extensions (compass §5 #1; desktop-shell.md §5).
 *
 * These are the capabilities the browser sandbox cannot perform:
 *   - {@link DesktopCapabilities.openWith} — open a path in the system default editor.
 *   - {@link DesktopCapabilities.revealInFinder} — reveal a path in Finder.
 *   - {@link DesktopCapabilities.getDaemonStatus} / `startDaemon` / `stopDaemon`
 *     — daemon lifecycle (P1 wires the real sidecar control; P0 stubs return a
 *     structured "not managed here" state because P0 runs against an externally
 *     started daemon).
 *
 * Transport = Tauri custom commands (Tauri IPC), **not** Local API HTTP — so
 * `wire_contracts_changed` stays `false` (compass §5 #5). The commands live in
 * `apps/desktop/src-tauri/src/lib.rs` and enforce the authoritative runtime path
 * guard (§5 #8).
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
  port?: number;
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
   * Current daemon lifecycle state. P0 returns a passive probe (the daemon is
   * externally managed); P1 returns the sidecar-controlled state.
   */
  getDaemonStatus(): Promise<DaemonStatus>;
  /** Restart the daemon. P0 stub — P1 drives the sidecar. */
  startDaemon(): Promise<void>;
  /** Stop the daemon. P0 stub — P1 drives the sidecar. */
  stopDaemon(): Promise<void>;
}

/**
 * The global Tauri namespace shape this adapter calls into. Only `core.invoke`
 * is used (custom commands); the opener plugin's JS API is not called directly
 * because the custom commands own the path guard (§5 #8).
 */
interface TauriGlobal {
  core: {
    invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T>;
  };
}

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
  // Rust PathGuardError serializes as `{ code, message }`. Anything else (incl.
  // Tauri invoke transport failures) collapses to `invoke_failed`.
  if (err && typeof err === 'object' && 'code' in err && 'message' in err) {
    const e = err as { code: string; message: string };
    return { code: e.code as DesktopCapabilityError['code'], message: e.message };
  }
  const message = err instanceof Error ? err.message : String(err);
  return { code: 'invoke_failed', message: message || 'Desktop command failed.' };
}

/**
 * Real `DesktopCapabilities` backed by Tauri custom commands. Constructed only
 * when {@link isDesktopBuild} is `true` (the client factory). The optional
 * `daemonHealth` callback lets P0 reuse the HTTP health probe (same transport as
 * `NexusClient.health()`) to report daemon status without sidecar control.
 */
export class TauriDesktopCapabilities implements DesktopCapabilities {
  private readonly daemonHealth: () => Promise<DaemonHealth>;
  private readonly resolvedPort: number;

  constructor(opts: { daemonHealth: () => Promise<DaemonHealth>; port: number }) {
    this.daemonHealth = opts.daemonHealth;
    this.resolvedPort = opts.port;
  }

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
    // P0: the daemon is externally managed (user ran `nexus42 daemon start`).
    // Probe via the shared HTTP health endpoint; map to the indicator states.
    try {
      const h = await this.daemonHealth();
      return {
        state: h.status === 'ok' ? 'running' : 'degraded',
        version: h.version,
        port: this.resolvedPort,
      };
    } catch {
      return {
        state: 'stopped',
        port: this.resolvedPort,
        detail: 'Restart the daemon to use local workspace features.',
      };
    }
  }

  async startDaemon(): Promise<void> {
    // P0 stub: sidecar control lands in P1. Calling this in V1.66 means the
    // user started the daemon externally — surface the recovery copy.
    throw {
      code: 'invoke_failed',
      message: 'Start the daemon with `nexus42 daemon start` (desktop sidecar arrives in V1.67).',
    } satisfies DesktopCapabilityError;
  }

  async stopDaemon(): Promise<void> {
    throw {
      code: 'invoke_failed',
      message: 'Stop the daemon with Ctrl-C in its terminal (desktop sidecar arrives in V1.67).',
    } satisfies DesktopCapabilityError;
  }
}

/** Sentinel for the browser build — every capability method reports it is not
 * available. Returned by `useDesktopCapabilities` when not in desktop mode, so
 * screens can branch on `null` rather than catching. */
export const DESKTOP_CAPABILITIES_UNAVAILABLE: DesktopCapabilityError = {
  code: 'not_in_desktop_build',
  message: 'This action is only available in the Nexus desktop app.',
};
