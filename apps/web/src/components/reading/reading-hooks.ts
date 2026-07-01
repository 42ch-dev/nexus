/**
 * Reading-surface data hooks — V1.79 Author Reflection (Track A / P0).
 *
 * Read-only composition of EXISTING query hooks for the manuscript reading
 * surface. No new write route, no new query key namespace — these hooks only
 * project data from `useChapters`, `useFindings`, `useWork`, and
 * `useWorldKbGraph` into the shapes the reading surface consumes. The
 * body-ownership invariant (canvas = sole authoring surface) is preserved:
 * nothing here mutates.
 */
import { useMemo } from 'react';

import { flattenPages, useChapters, useFindings, useWork } from '@/api/queries';
import { useWorldKbGraph } from '@/lib/canvas/use-world-kb-data';
import type { ChapterSummary } from '@42ch/nexus-contracts';

/** Non-terminal finding statuses (open + triaged + in_review). */
export const OPEN_FINDING_STATUSES = 'open,triaged,in_review';

/** Broad page so neighbor resolution covers most manuscripts in one page. */
const NEIGHBOR_PAGE_LIMIT = 200;

export interface ChapterNeighbors {
  /** All loaded chapters for the Work, in server order. */
  chapters: ChapterSummary[];
  /** Immediately preceding chapter relative to the current one, or null. */
  prev: ChapterSummary | null;
  /** Immediately following chapter relative to the current one, or null. */
  next: ChapterSummary | null;
  /** Distinct volume numbers present in the Work (sorted ascending). */
  volumes: number[];
}

function matchCurrent(
  row: ChapterSummary,
  chapter: number,
  volume: number | undefined,
): boolean {
  // Chapter number is the primary key; volume disambiguates only when the
  // caller is reading a specific volume context (the route's ?volume= query).
  if (row.chapter !== chapter) return false;
  if (volume === undefined) return true;
  return (row.volume ?? 1) === volume;
}

/**
 * Resolve the prev/next chapters for navigation within a Work. Reads the
 * chapter list (one broad page) and locates the current chapter's neighbors.
 * Returns `prev: null, next: null` while the list is still loading.
 */
export function useChapterNeighbors(
  workId: string | undefined,
  chapter: number | undefined,
  volume: number | undefined,
): ChapterNeighbors {
  const chapters = useChapters(workId || undefined, { limit: NEIGHBOR_PAGE_LIMIT });
  const rows = useMemo(() => flattenPages(chapters.data), [chapters.data]);

  return useMemo<ChapterNeighbors>(() => {
    if (chapter === undefined) {
      return { chapters: rows, prev: null, next: null, volumes: deriveVolumes(rows) };
    }
    const idx = rows.findIndex((r) => matchCurrent(r, chapter, volume));
    if (idx === -1) {
      return { chapters: rows, prev: null, next: null, volumes: deriveVolumes(rows) };
    }
    return {
      chapters: rows,
      prev: idx > 0 ? rows[idx - 1] : null,
      next: idx >= 0 && idx < rows.length - 1 ? rows[idx + 1] : null,
      volumes: deriveVolumes(rows),
    };
  }, [rows, chapter, volume]);
}

function deriveVolumes(rows: ChapterSummary[]): number[] {
  const set = new Set<number>();
  for (const r of rows) set.add(r.volume ?? 1);
  return [...set].sort((a, b) => a - b);
}

/**
 * Count non-terminal findings scoped to a chapter (the actionable count while
 * reading). Reuses the existing `useFindings` cache; the comma-separated status
 * filter is enforced server-side (`list_findings_handler`).
 */
export function useOpenFindingsCount(
  workId: string | undefined,
  chapter: number | undefined,
): { count: number; isLoading: boolean } {
  const findings = useFindings(workId || undefined, {
    status: OPEN_FINDING_STATUSES,
    chapter,
    limit: NEIGHBOR_PAGE_LIMIT,
  });
  const rows = useMemo(() => flattenPages(findings.data), [findings.data]);
  return { count: rows.length, isLoading: findings.isLoading };
}

/**
 * Count World KB key blocks for the Work's World. Resolves the World via
 * `useWork`, then reads the entity graph (excludes deleted entities
 * server-side). Returns `count: null` when the Work has no World bound.
 */
export function useWorldKbDensity(
  workId: string | undefined,
): { count: number | null; isLoading: boolean } {
  const work = useWork(workId);
  const worldId = work.data?.world_id;
  const graph = useWorldKbGraph(worldId);
  const enabled = Boolean(worldId);
  if (!enabled) return { count: null, isLoading: work.isLoading };
  return { count: graph.data?.entities.length ?? 0, isLoading: graph.isLoading };
}
