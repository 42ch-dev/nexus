/**
 * Findings lifecycle — client-side mirror of the server-enforced 6-state status
 * machine.
 *
 * Spec: `.mstar/knowledge/specs/findings-lifecycle.md` §2. The DAO enforces
 * transition adjacency on every PATCH
 * (`crates/nexus-local-db/src/findings.rs:172` — `is_valid_transition()`); an
 * illegal transition is rejected with HTTP 422 `INVALID_TRANSITION`. The UI
 * disables illegal transitions as defense-in-depth + UX (D1a LOCKED), but the
 * server is the authority — these constants exist only to render the right
 * enabled/disabled affordances, not to gate writes.
 *
 * Keep this table in sync with the DAO adjacency table; it is pinned by
 * `findings-lifecycle.test.ts`.
 */

/** The 6 finding statuses (DAO `VALID_STATUSES`). */
export const FINDING_STATUSES = [
  'open',
  'triaged',
  'in_review',
  'resolved',
  'wont_fix',
  'duplicate',
] as const;
export type FindingStatus = (typeof FINDING_STATUSES)[number];

/** Terminal statuses — no outbound transitions (DAO terminal set). */
export const TERMINAL_FINDING_STATUSES: ReadonlySet<FindingStatus> = new Set([
  'resolved',
  'wont_fix',
  'duplicate',
]);

/**
 * Transition adjacency (DAO `is_valid_transition()`). Each non-terminal status
 * maps to the set of statuses it may advance to. Self-loops are NOT permitted
 * (a `status: "<current>"` patch is rejected as `INVALID_TRANSITION`); callers
 * that only want to refresh `updated_at` must omit `status` from the patch.
 */
const TRANSITIONS: Readonly<Record<string, readonly FindingStatus[]>> = {
  open: ['triaged', 'in_review', 'resolved', 'wont_fix', 'duplicate'],
  triaged: ['in_review', 'resolved', 'wont_fix', 'duplicate'],
  in_review: ['resolved', 'wont_fix', 'duplicate'],
  resolved: [],
  wont_fix: [],
  duplicate: [],
};

/** Is `to` a valid next status from `from`? (Self-transitions are invalid.) */
export function isValidTransition(from: string | undefined, to: string | undefined): boolean {
  if (!from || !to) return false;
  if (from === to) return false;
  return TRANSITIONS[from]?.includes(to as FindingStatus) ?? false;
}

/** The statuses a finding in `from` may advance to (empty for terminal). */
export function nextStatuses(from: string | undefined): FindingStatus[] {
  if (!from) return [];
  return [...(TRANSITIONS[from] ?? [])];
}

/** Is `status` terminal (no outbound transitions)? */
export function isTerminalStatus(status: string | undefined): boolean {
  return !!status && TERMINAL_FINDING_STATUSES.has(status as FindingStatus);
}
