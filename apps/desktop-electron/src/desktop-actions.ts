/**
 * Guarded desktop OS actions (v1.192 P0-T3): open-with, reveal-in-Finder,
 * directory picker, external-URL opener and the user-confirmed local-state
 * reset.
 *
 * Parity rows 1/2/5/19/20/25 of the frozen plan. Every path-bearing action
 * re-resolves the active workspace root per call (P0-T2's
 * `resolveWorkspaceRoot`), realpaths both root and candidate and denies
 * symlink escapes, sibling-prefix collisions, unreadable root/candidate and
 * NUL **before any OS effect** — with the frozen structured error codes
 * `workspace_root_unknown`, `path_unresolvable`, `path_outside_workspace`.
 * The canonical (realpathed) path is what reaches the OS call.
 *
 * The reset is user-triggered and user-confirmed: cancellation resolves
 * `{status:'cancelled'}` — discriminated from success — and touches nothing;
 * a confirmed reset invokes the bounded real recovery on the P0-T4
 * controller (service close → native `resetLocalState(home)` →
 * readiness/restart) and resolves `{status:'confirmed'}` only after it
 * completes; and any failure propagates — a failed reset never surfaces as
 * success.
 *
 * Electron runtime objects (`shell`, `dialog`) are injected structurally, so
 * this module has no Electron import and the handlers are directly testable
 * in plain Node against the compiled output.
 */

import { realpath } from 'node:fs/promises';
import { isAbsolute, join, sep } from 'node:path';

import {
  desktopError,
  errorMessage,
  isAllowedDesktopExternalUrl,
} from './desktop-contract.js';
import type { DesktopHandlers } from './desktop-ipc.js';

/** Operations P0-T3 owns in the frozen host-contract table. */
export type DesktopActionOperation =
  | 'open_with'
  | 'reveal_in_finder'
  | 'open_external_url'
  | 'pick_directory'
  | 'reset_local_database';

export type DesktopActionHandlers = Pick<DesktopHandlers, DesktopActionOperation>;

/**
 * Structural view of the Electron `shell` module this module needs. Main
 * passes the real `shell`; tests pass fakes. No other shell surface is
 * reachable from the desktop actions.
 */
export interface DesktopShellAdapters {
  openPath(path: string): Promise<string>;
  showItemInFolder(path: string): void;
  openExternal(url: string): Promise<void>;
}

/**
 * Main-side OS dialogs, narrowed to the two P0-T3 needs. `pickDirectory` is
 * directory-only with `defaultPath` support and resolves null on cancel;
 * `confirmReset` is the explicit native confirmation gate for the
 * local-state reset (row 19 + D18/D20).
 */
export interface DesktopDialogAdapters {
  pickDirectory(options: { defaultPath?: string }): Promise<string | null>;
  confirmReset(): Promise<boolean>;
}

/**
 * Narrowest structural view of the P0-T4 `DesktopServiceController` this
 * module consumes. The controller itself owns the serialization, the
 * confirmed service close, the bounded native reset (utility
 * `reset-local-state` with the trusted home) and the post-reset
 * readiness/restart — this handler only gates it behind user confirmation.
 */
export interface DesktopResetController {
  resetLocalState(): Promise<void>;
}

export interface DesktopActionsOptions {
  /** Re-resolves the active workspace root on every guarded call (P0-T2). */
  resolveWorkspaceRoot(): Promise<string>;
  shell: DesktopShellAdapters;
  dialog: DesktopDialogAdapters;
  controller: DesktopResetController;
}

// ---------------------------------------------------------------------------
// Path guard (parity row 5 + D10)
// ---------------------------------------------------------------------------

/**
 * Canonicalize one workspace-relative or absolute path for an OS action.
 * Resolves the root fresh, realpaths both sides and accepts only the root
 * itself or paths strictly inside it (candidate == root or starts with
 * root + separator — so a sibling sharing the root's name prefix is denied).
 * Never performs an OS effect on failure.
 */
async function guardWorkspacePath(
  resolveWorkspaceRoot: () => Promise<string>,
  rawPath: string,
): Promise<string> {
  if (rawPath.includes('\0')) {
    throw desktopError('path_unresolvable', 'path contains a NUL byte');
  }
  let rootRaw: string;
  try {
    rootRaw = await resolveWorkspaceRoot();
  } catch (err) {
    throw desktopError(
      'workspace_root_unknown',
      `active workspace root could not be resolved: ${errorMessage(err)}`,
    );
  }
  if (typeof rootRaw !== 'string' || rootRaw.length === 0) {
    throw desktopError('workspace_root_unknown', 'active workspace root is empty');
  }
  let root: string;
  try {
    root = await realpath(rootRaw);
  } catch {
    throw desktopError('workspace_root_unknown', `workspace root does not exist or is unreadable: ${rootRaw}`);
  }
  const candidateJoined = isAbsolute(rawPath) ? rawPath : join(root, rawPath);
  let candidate: string;
  try {
    candidate = await realpath(candidateJoined);
  } catch {
    throw desktopError('path_unresolvable', `path does not exist or is unreadable: ${rawPath}`);
  }
  if (candidate !== root && !candidate.startsWith(root + sep)) {
    throw desktopError('path_outside_workspace', `path escapes the active workspace: ${rawPath}`);
  }
  return candidate;
}

// ---------------------------------------------------------------------------
// External URL policy (parity row 25)
// ---------------------------------------------------------------------------

// The single main-owned predicate `isAllowedDesktopExternalUrl` lives in the
// shared host contract (desktop-contract.ts) and rejects C0 controls / DEL
// on the raw string BEFORE WHATWG parsing (which would otherwise strip or
// remap some controls and let them pass). Both this action and the
// protocol/navigation layer consume that one implementation.

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

/**
 * Build the guarded action handler map. Main (P0-T7) composes this into the
 * full `DesktopHandlers` map; tests drive it with fake OS adapters.
 */
export function createDesktopActions(options: DesktopActionsOptions): DesktopActionHandlers {
  const { resolveWorkspaceRoot, shell, dialog, controller } = options;

  return {
    /** Parity row 1: guarded open with the default app. */
    async open_with(payload) {
      if (typeof payload !== 'object' || payload === null || !('path' in payload)) {
        throw desktopError('invalid_input', 'a path payload is required');
      }
      const { path } = payload;
      if (typeof path !== 'string') {
        throw desktopError('invalid_input', 'path must be a string');
      }
      const canonical = await guardWorkspacePath(resolveWorkspaceRoot, path);
      const failure = await shell.openPath(canonical);
      // shell.openPath resolves an empty string on success, an error string
      // on failure; both must reach the caller (never a success envelope).
      if (failure !== '') {
        throw desktopError('open_failed', failure);
      }
      return null;
    },

    /** Parity row 2: guarded reveal in Finder. */
    async reveal_in_finder(payload) {
      if (typeof payload !== 'object' || payload === null || !('path' in payload)) {
        throw desktopError('invalid_input', 'a path payload is required');
      }
      const { path } = payload;
      if (typeof path !== 'string') {
        throw desktopError('invalid_input', 'path must be a string');
      }
      const canonical = await guardWorkspacePath(resolveWorkspaceRoot, path);
      shell.showItemInFolder(canonical);
      return null;
    },

    /** Parity row 25: external URLs pass the single main-owned predicate. */
    async open_external_url(payload) {
      const url = typeof payload === 'object' && payload !== null && 'url' in payload ? payload.url : undefined;
      if (typeof url !== 'string' || !isAllowedDesktopExternalUrl(url)) {
        throw desktopError('url_not_allowed', 'external URL is not an allowed http/https destination');
      }
      await shell.openExternal(url);
      return null;
    },

    /** Parity row 20: main directory-only picker, defaultPath, cancel = null. */
    async pick_directory(payload) {
      const defaultPath =
        typeof payload === 'object' && payload !== null && 'defaultPath' in payload ? payload.defaultPath : undefined;
      if (defaultPath !== undefined && typeof defaultPath !== 'string') {
        throw desktopError('invalid_input', 'defaultPath must be a string when present');
      }
      // Deliberate root-selection dialog: intentionally NOT guarded to the
      // current workspace — it may pick a new workspace outside the root;
      // the committed root governs subsequent guarded actions.
      return dialog.pickDirectory(defaultPath === undefined || defaultPath === '' ? {} : { defaultPath });
    },

    /**
     * Parity row 19 + D18/D20: reset only after explicit native confirmation.
     * Cancellation resolves `{status:'cancelled'}` — discriminated from
     * success so renderer consumers never treat a declined dialog as a
     * completed reset. A confirmed reset invokes the controller's bounded
     * real recovery (confirmed service close → native `resetLocalState(home)`
     * → readiness/restart) and resolves `{status:'confirmed'}` only after it
     * completes; any failure propagates as a typed error — never a success
     * envelope.
     */
    async reset_local_database() {
      let confirmed: boolean;
      try {
        confirmed = await dialog.confirmReset();
      } catch (err) {
        // Coded dialog errors (e.g. user-cancelled native dialog) keep their
        // code; a codeless dialog failure is a confirmation failure.
        const code =
          typeof err === 'object' && err !== null && 'code' in err && typeof err.code === 'string'
            ? err.code
            : 'reset_confirmation_failed';
        throw desktopError(code, `reset confirmation failed: ${errorMessage(err)}`);
      }
      if (!confirmed) {
        // User declined: nothing was closed, deleted or restarted.
        return { status: 'cancelled' };
      }
      await controller.resetLocalState();
      return { status: 'confirmed' };
    },
  };
}
