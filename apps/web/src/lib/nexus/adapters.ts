/**
 * Client-side adapters for the F-P3 and F-F1 deferred residuals.
 *
 * Spec: compass §5 item #2 + §6 residual targets; web-ui.md §10.
 *
 * - **F-P3 (list-array naming)**: the daemon's list endpoints use heterogeneous
 *   array keys (`works`, `sessions`, `schedules`, `capabilities`) until the
 *   structural rename to `items` lands in V1.66+. The query layer normalizes
 *   them here so screen components consume one shape `{ items, pagination? }`.
 *   Findings already uses `items` (new endpoint, convention §4).
 *
 * - **F-F1 (sort)**: no `sort_by`/`sort_order` support exists server-side; the
 *   UI sorts small un-paginated lists client-side. Cursor-paginated lists
 *   (Works, Findings) are NOT sorted client-side — server order is
 *   authoritative and a client re-sort would break pagination consistency.
 *
 * These adapters are deliberately thin and side-effect-free so the V1.66+
 * structural closure removes them without touching screen code.
 */
import type { PaginationInfo } from '@42ch/nexus-contracts';

/** Common shape all list screens consume after normalization. */
export interface NormalizedList<T = unknown> {
  items: T[];
  /** Present for cursor-paginated endpoints (Works, Findings); absent otherwise. */
  pagination?: PaginationInfo;
}

/** Known array keys the daemon emits today (F-P3 closure target). */
export type ListArrayKey = 'works' | 'sessions' | 'schedules' | 'capabilities' | 'items';

/**
 * Extract `{ items, pagination? }` from a list response keyed by `arrayKey`.
 *
 * Returns `{ items: [] }` when the key is missing so screens always render a
 * stable empty state rather than `undefined`.
 */
export function normalizeList<T = unknown>(
  response: Record<string, unknown>,
  arrayKey: ListArrayKey,
): NormalizedList<T> {
  const rawItems = (response[arrayKey] as T[] | undefined) ?? [];
  const pagination = response.pagination as PaginationInfo | undefined;
  return { items: rawItems, pagination };
}

/**
 * Sort a small list newest-first by an ISO-date accessor (F-F1 client-side).
 *
 * Rows whose accessor returns `undefined`/empty are pushed to the end and kept
 * in stable relative order. The input array is not mutated. Use only on lists
 * the daemon returns un-paginated (sessions, schedules, capabilities, presets);
 * never on cursor-paginated lists (Works, Findings).
 */
export function sortByDate<T>(rows: readonly T[], dateOf: (row: T) => string | undefined): T[] {
  const indexed = rows.map((row, idx) => ({ row, idx, t: dateOf(row) }));
  indexed.sort((a, b) => {
    const at = a.t ? Date.parse(a.t) : NaN;
    const bt = b.t ? Date.parse(b.t) : NaN;
    const aMissing = Number.isNaN(at);
    const bMissing = Number.isNaN(bt);
    if (aMissing && bMissing) return a.idx - b.idx; // stable for both undated
    if (aMissing) return 1; // undated sinks to the end
    if (bMissing) return -1;
    return bt - at; // newest first
  });
  return indexed.map((entry) => entry.row);
}
