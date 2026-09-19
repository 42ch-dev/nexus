/**
 * Lifecycle coordination for the app-managed service owner (v1.192 P0-T4).
 *
 * Shared by the service controller (main side) and the utility owner: the
 * crash-retry backoff schedule (parity row 10) and the frozen close-truth
 * predicates. The proof shell's renderer-reattach/open-policy helpers were
 * retired with the proof main that imported them (P0-T7); a renderer crash
 * now only replaces the renderer window — the service owner is untouched.
 */

/**
 * Unexpected owned-service exits retry with these delays (parity row 10:
 * 500ms/1s/2s/4s/8s, five attempts, then stopped with manual recovery).
 */
export const CRASH_BACKOFF_DELAYS_MS: readonly number[] = [500, 1_000, 2_000, 4_000, 8_000];

/** Delay for a 1-based recovery attempt, or null when the budget is exhausted. */
export function crashBackoffDelayMs(attempt: number): number | null {
  if (!Number.isInteger(attempt) || attempt < 1) return null;
  return CRASH_BACKOFF_DELAYS_MS[attempt - 1] ?? null;
}

/** A frame from a previous owner generation is ignored, never applied. */
export function isStaleGeneration(messageGeneration: number, currentGeneration: number): boolean {
  return messageGeneration !== currentGeneration;
}

/**
 * Row 8 / §9 close truth: only a generated report that says `closed` *and*
 * confirms cleanup is success. An interrupted or unconfirmed close (including
 * a lost owner) is never reported as a successful cleanup.
 */
export function isConfirmedCloseReport(report: unknown): boolean {
  if (!report || typeof report !== 'object') return false;
  if (!('state' in report) || report.state !== 'closed') return false;
  return 'cleanup_confirmed' in report && report.cleanup_confirmed === true;
}
