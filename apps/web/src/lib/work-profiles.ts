/**
 * Work-profile SSOT (R-V167P1-QC1-S2).
 *
 * The canonical set of work profiles the Create-Work dialog admits. The
 * `value` is the wire identifier sent to the daemon and MUST match the
 * backend canonical set — the daemon HTTP handler stores `work_profile`
 * verbatim (no normalization; see
 * `crates/nexus-daemon-runtime/src/api/handlers/works.rs` create_work). The
 * authoritative set is the DB CHECK constraint in
 * `crates/nexus-local-db/migrations/202606230001_work_profile_script.sql`
 * and the Rust helpers in `crates/nexus-local-db/src/works.rs` —
 * `game_bible` uses an underscore (not a hyphen).
 *
 * Consumed by the Create-Work dialog (`pages/dialogs/create-work-dialog.tsx`)
 * and the future V1.70 canvas Strategy surface. Keep this the single source —
 * do not duplicate the profile list elsewhere in `apps/web/src/`.
 */

/** Literal union of admitted work-profile wire values. */
export type WorkProfile = 'novel' | 'essay' | 'game_bible' | 'script';

/**
 * Canonical ordered values — matches the backend CHECK constraint order. The
 * first entry is the dialog's display default (the field is still omitted on
 * an untouched form so the daemon stores NULL; see the dialog).
 */
export const WORK_PROFILE_VALUES = [
  'novel',
  'essay',
  'game_bible',
  'script',
] as const satisfies readonly WorkProfile[];

/** Human-readable labels keyed by wire value. */
export const WORK_PROFILE_LABELS: Record<WorkProfile, string> = {
  novel: 'Novel',
  essay: 'Essay',
  game_bible: 'Game Bible',
  script: 'Script',
};

/**
 * Selector options (`{ value, label }`) derived from the values + labels so
 * the two can never drift. Consumed by the Create-Work dialog's `<Select>`.
 */
export const WORK_PROFILES: readonly { value: WorkProfile; label: string }[] =
  WORK_PROFILE_VALUES.map((value) => ({ value, label: WORK_PROFILE_LABELS[value] }));

/**
 * Type guard: narrows an arbitrary string to a {@link WorkProfile}. Used at
 * the dialog's `<Select>` boundary to reject invalid values before they enter
 * the typed selector state (R-V167P1-QC1-S1).
 */
export function isWorkProfile(value: string): value is WorkProfile {
  return (WORK_PROFILE_VALUES as readonly string[]).includes(value);
}
