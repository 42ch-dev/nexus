/**
 * Desktop-only `NexusClient` extensions (compass §5 #1; desktop-shell.md §5).
 *
 * These are the capabilities the browser sandbox cannot perform:
 *   - {@link DesktopCapabilities.openWith} — open a path in the system default editor.
 *   - {@link DesktopCapabilities.revealInFinder} — reveal a path in Finder.
 *   - {@link DesktopCapabilities.getDaemonStatus} / `startDaemon` / `stopDaemon`
 *     — daemon lifecycle owned by main's `DesktopServiceController`.
 *
 * Transport = the typed preload bridge (`window.nexusDesktop.invoke`, frozen
 * operation union from `apps/desktop-electron/src/desktop-contract.ts`),
 * **not** Daemon API HTTP — so `wire_contracts_changed` stays `false`
 * (compass §5 #5). Main enforces the authoritative runtime path guard (§5 #8)
 * for file actions.
 *
 * Browser build: the {@link DesktopCapabilities} context value is `null`, so
 * screens hide the desktop-only affordances (Copy Path stays — it is plain
 * clipboard write, browser + desktop).
 */
import { errorMessage } from '@/lib/error-message';
import { isEntranceId, type EntranceId } from '@/components/layout/entrance-registry';
import { getDesktopBridge, invokeDesktop } from './desktop-bridge';
import type { DaemonStatus, ResetLocalDatabaseResult } from '../../../../desktop-electron/src/desktop-contract';

export type { DaemonStatus, ResetLocalDatabaseResult };

/** Structured error thrown by desktop capability methods. Mirrors the main
 * action error shape (`{ code, message }`) so the toast layer can read it
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

/**
 * Desktop-only capability surface. Provided via React context; `null` in browser
 * mode. Screens must depend on this interface (via `useDesktopCapabilities`),
 * never on `window.nexusDesktop` directly — that keeps a clean boundary for
 * tests.
 */
export interface DesktopCapabilities {
  /** Open `path` in the system default editor (path-guarded). */
  openWith(path: string): Promise<void>;
  /** Reveal `path` in Finder (path-guarded). */
  revealInFinder(path: string): Promise<void>;
  /**
   * Open a URL in the system default browser. Only `http:` / `https:` URLs
   * are accepted (validated by main). Returns a structured error on invalid
   * scheme or open failure.
   */
  openExternalUrl(url: string): Promise<void>;
  /**
   * Current daemon lifecycle state, from the main-owned controller.
   */
  getDaemonStatus(): Promise<DaemonStatus>;
  /** Subscribe to daemon status changes emitted by the controller. */
  onDaemonStatusChanged(callback: (status: DaemonStatus) => void): Promise<() => void>;
  /** Start/restart the owned service. */
  startDaemon(): Promise<void>;
  /** Stop the owned service. */
  stopDaemon(): Promise<void>;
  /** Restart the service atomically (owned or explicitly authorized attached). */
  restartDaemon(): Promise<void>;
  /**
   * Wipe the daemon's local state database(s) under `~/.nexus42/` so the daemon
   * can boot fresh. Creative files in the workspace are untouched. Resolves
   * `{status:'confirmed'}` only after main's explicit native confirmation and a
   * completed bounded reset; `{status:'cancelled'}` when the user declines the
   * native dialog (nothing was closed, deleted or restarted — stay in
   * recovery); rejects with a coded error on failure.
   */
  resetLocalDatabase(): Promise<ResetLocalDatabaseResult>;
  /**
   * Open a native directory picker starting at `defaultPath` and return the
   * selected directory path, or `null` if the user cancelled.
   */
  pickDirectory(defaultPath: string): Promise<string | null>;
  /**
   * Persist the chosen workspace path to `~/.nexus42/config.toml` so the daemon
   * and CLI agree on the active workspace root.
   */
  setWorkspacePath(path: string): Promise<void>;
  /**
   * Whether the first-launch setup wizard has been completed.
   * Browser build defaults to `true`; desktop reads from the main-owned config.
   */
  getSetupCompleted(): Promise<boolean>;
  /** Mark setup as completed (desktop only). */
  setSetupCompleted(value: boolean): Promise<void>;
  /**
   * Read the persisted user-layer entrance (AR-16) from `~/.nexus42/config.toml`.
   * Missing/unparseable values resolve to `content-creator` (the default) —
   * never a third state. Invoke errors throw `DesktopCapabilityError` (the
   * provider fails open to the default).
   */
  getEntrance(): Promise<EntranceId>;
  /** Persist the user-layer entrance (AR-16) — desktop only. */
  setEntrance(value: EntranceId): Promise<void>;
  /** Persist the agent profile selected during setup (desktop only). */
  setAgentProfile(name: string, launchCommand?: string): Promise<void>;
  /**
   * Read the saved agent profile for Settings preselect (desktop only).
   * Returns `null` when missing, unreadable, or no `native_cli` provider —
   * invoke transport errors are also surfaced as `null` so preselect never crashes.
   */
  getAgentProfile(): Promise<{ name: string; launchCommand?: string } | null>;
  /**
   * Switch the active Profile in `~/.nexus42/config.toml`, updating the active
   * creator ID and mirroring the target Profile's workspace path to the legacy
   * `workspace_path` key. Returns the resolved workspace path for the switched-to
   * Profile (AC-P0-5).
   */
  switchActiveCreator(creatorId: string): Promise<string>;
  /** Resolve the default workspace root path (desktop only). */
  getWorkspaceRoot(): Promise<string>;
  /** Toggle the main window between maximized and restored (desktop titlebar double-click). */
  toggleMaximizeWindow(): Promise<void>;
  /**
   * Bootstrap local creator/workspace state before daemon start.
   * Idempotent: if a creator ID already exists, returns it without overwriting.
   * Browser build: no-op — wizard skips this step when {@link useDesktopCapabilities} is `null`.
   */
  ensureSetupBootstrap(): Promise<{ creator_id: string; already_bootstrapped: boolean }>;
}

function asDesktopError(err: unknown): DesktopCapabilityError {
  // Main action errors serialize as `{ code, message }` (the preload rethrows
  // them as `Error` with a `code` own-property). Anything else collapses to
  // `invoke_failed`.
  if (err && typeof err === 'object' && 'code' in err && 'message' in err) {
    const e = err as { code: string; message: string };
    return { code: e.code as DesktopCapabilityError['code'], message: e.message };
  }
  const message = errorMessage(err) || 'Desktop command failed.';
  return { code: 'invoke_failed', message };
}

/**
 * Real `DesktopCapabilities` backed by the typed preload bridge. Constructed
 * only when {@link isDesktopBuild} is `true` (the client factory); every
 * method resolves the bridge lazily so construction never throws.
 */
export class ElectronDesktopCapabilities implements DesktopCapabilities {
  async openWith(path: string): Promise<void> {
    try {
      await invokeDesktop('open_with', { path });
    } catch (err) {
      throw asDesktopError(err);
    }
  }

  async openExternalUrl(url: string): Promise<void> {
    try {
      await invokeDesktop('open_external_url', { url });
    } catch (err) {
      throw asDesktopError(err);
    }
  }

  async revealInFinder(path: string): Promise<void> {
    try {
      await invokeDesktop('reveal_in_finder', { path });
    } catch (err) {
      throw asDesktopError(err);
    }
  }

  async getDaemonStatus(): Promise<DaemonStatus> {
    try {
      return await invokeDesktop('get_daemon_status');
    } catch (err) {
      throw asDesktopError(err);
    }
  }

  async onDaemonStatusChanged(
    callback: (status: DaemonStatus) => void,
  ): Promise<() => void> {
    try {
      const bridge = getDesktopBridge();
      if (!bridge) throw new Error('Desktop bridge unavailable.');
      // The preload's subscription returns the unsubscribe synchronously and
      // never passes Electron event objects across the boundary.
      return bridge.onStatusChanged(callback);
    } catch (err) {
      throw asDesktopError(err);
    }
  }

  async startDaemon(): Promise<void> {
    try {
      await invokeDesktop('start_daemon');
    } catch (err) {
      throw asDesktopError(err);
    }
  }

  async stopDaemon(): Promise<void> {
    try {
      await invokeDesktop('stop_daemon');
    } catch (err) {
      throw asDesktopError(err);
    }
  }

  async restartDaemon(): Promise<void> {
    try {
      await invokeDesktop('restart_daemon');
    } catch (err) {
      throw asDesktopError(err);
    }
  }

  async resetLocalDatabase(): Promise<ResetLocalDatabaseResult> {
    try {
      return await invokeDesktop('reset_local_database');
    } catch (err) {
      throw asDesktopError(err);
    }
  }

  async pickDirectory(defaultPath: string): Promise<string | null> {
    try {
      return await invokeDesktop('pick_directory', { defaultPath });
    } catch (err) {
      throw asDesktopError(err);
    }
  }

  async setWorkspacePath(path: string): Promise<void> {
    try {
      await invokeDesktop('set_workspace_path', { path });
    } catch (err) {
      throw asDesktopError(err);
    }
  }

  async ensureSetupBootstrap(): Promise<{ creator_id: string; already_bootstrapped: boolean }> {
    try {
      return await invokeDesktop('ensure_setup_bootstrap');
    } catch (err) {
      throw asDesktopError(err);
    }
  }

  async getSetupCompleted(): Promise<boolean> {
    try {
      return await invokeDesktop('get_setup_completed');
    } catch (err) {
      throw asDesktopError(err);
    }
  }

  async setSetupCompleted(value: boolean): Promise<void> {
    try {
      await invokeDesktop('set_setup_completed', { value });
    } catch (err) {
      throw asDesktopError(err);
    }
  }

  async getEntrance(): Promise<EntranceId> {
    try {
      const value = await invokeDesktop('get_entrance');
      // Stored-but-unparseable resolves content-creator (AR-16) — main only
      // ever writes valid values, but a hand-edited config.toml must not
      // produce a third state.
      return isEntranceId(value) ? value : 'content-creator';
    } catch (err) {
      throw asDesktopError(err);
    }
  }

  async setEntrance(value: EntranceId): Promise<void> {
    try {
      await invokeDesktop('set_entrance', { value });
    } catch (err) {
      throw asDesktopError(err);
    }
  }

  async setAgentProfile(name: string, launchCommand?: string): Promise<void> {
    try {
      await invokeDesktop('set_agent_profile', { name, launchCommand });
    } catch (err) {
      throw asDesktopError(err);
    }
  }

  async getAgentProfile(): Promise<{ name: string; launchCommand?: string } | null> {
    try {
      return await invokeDesktop('get_agent_profile');
    } catch {
      // Preselect path: treat invoke/transport failures as "no saved profile".
      return null;
    }
  }

  async switchActiveCreator(creatorId: string): Promise<string> {
    try {
      return await invokeDesktop('switch_active_creator', { creatorId });
    } catch (err) {
      throw asDesktopError(err);
    }
  }

  async getWorkspaceRoot(): Promise<string> {
    try {
      return await invokeDesktop('get_workspace_root');
    } catch (err) {
      throw asDesktopError(err);
    }
  }

  async toggleMaximizeWindow(): Promise<void> {
    try {
      await invokeDesktop('toggle_maximize_window');
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
