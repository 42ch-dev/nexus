import { errorMessage } from '@/lib/error-message';
import { NexusClientError } from '@/lib/nexus/errors';

/**
 * Class of a Setup Continue-path failure (spec §Architecture contract).
 *
 * - `soft_workspace_path` — `setWorkspacePath` failed (Tauri config/IO).
 * - `soft_bootstrap` — `ensureSetupBootstrap` failed for a non-migration reason.
 * - `soft_display_name` — `updateCreator(display_name)` failed after a
 *   successful bootstrap (e.g. the historical HTTP 409 `uninitialized`).
 * - `migration_db` — a migration / DB-class failure surfaced during the
 *   bootstrap phase (the only class that permits Reset).
 */
export type SetupContinueErrorClass =
  | 'soft_workspace_path'
  | 'soft_bootstrap'
  | 'soft_display_name'
  | 'migration_db';

export interface SetupContinueError {
  message: string;
  class: SetupContinueErrorClass;
}

/** Phase of the Continue path that produced an error. */
export type ContinueErrorPhase = 'workspace_path' | 'bootstrap' | 'display_name';

/**
 * Structured error codes that indicate a migration / DB failure.
 *
 * NOTE (T1 finding): these are effectively dead — the daemon emits the public
 * code `internal` for DB errors, never `migration_failed` /
 * `database_migration_failed` / `database_error`. They are kept for
 * completeness; the primary migration signal is the message regex below.
 */
const MIGRATION_CODES = new Set<string>([
  'migration_failed',
  'database_migration_failed',
  'database_error',
]);

/**
 * Classify a Continue-path error into a {@link SetupContinueErrorClass}.
 *
 * Migration detection relies on the message regex `/migration/i` (T1 finding:
 * structured codes are dead — the daemon emits public `internal` with no
 * `details` for DB failures, so a message like "Failed to run DB migrations:
 * ..." is the signal). The sidecar `detail` signal is NOT reachable from the
 * Continue path today and is intentionally not consulted.
 *
 * Classification rules (per spec §Architecture contract):
 * - `display_name` phase → always `soft_display_name`.
 * - `workspace_path` phase → always `soft_workspace_path`.
 * - `bootstrap` phase + migration signal → `migration_db`.
 * - `bootstrap` phase (otherwise) → `soft_bootstrap`.
 *
 * The returned `message` is the raw extracted message (possibly empty); callers
 * are responsible for applying a phase fallback i18n key when it is empty.
 */
export function classifySetupContinueError(
  phase: ContinueErrorPhase,
  error: unknown,
): SetupContinueError {
  const message = errorMessage(error);

  if (phase === 'display_name') {
    return { message, class: 'soft_display_name' };
  }
  if (phase === 'workspace_path') {
    return { message, class: 'soft_workspace_path' };
  }

  // bootstrap phase: detect migration/DB-class, else default soft_bootstrap.
  const code = error instanceof NexusClientError ? error.code : '';
  const isMigrationDb =
    (code !== '' && MIGRATION_CODES.has(code)) || /migration/i.test(message);
  return { message, class: isMigrationDb ? 'migration_db' : 'soft_bootstrap' };
}
