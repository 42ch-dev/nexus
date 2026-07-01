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
import { useEffect, useMemo } from 'react';

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
  /**
   * True while the chapter list is still being walked (page-1 load or
   * cursor-walking additional pages). While true, `prev`/`next` may be null
   * simply because the relevant page has not loaded yet — callers must NOT
   * degrade to "first/last chapter" placeholders until this is false.
   */
  loading: boolean;
}

function matchCurrent(
  row: ChapterSummary,
  chapter: number,
  volume: number | undefined,
): boolean {
  // Chapter number is the primary key; volume disambiguates only when the
  // caller is reading a specific volume context (the route's ?volume= query).
  // `row.volume` is contract-guaranteed (ChapterSummary.volume: number, >= 1).
  if (row.chapter !== chapter) return false;
  if (volume === undefined) return true;
  return row.volume === volume;
}

/**
 * Resolve the prev/next chapters for navigation within a Work. Reads the
 * chapter list and locates the current chapter's neighbors.
 *
 * The daemon clamps the chapter-list `limit` to `[1, 100]`
 * (`crates/nexus-daemon-runtime/src/api/handlers/chapters.rs`), so a broad
 * request is silently served as a 100-row page. When the server signals more
 * pages (`has_more`), this hook cursor-walks the full list so neighbor
 * resolution sees the complete ordered set — without it, chapters past the
 * first server page silently lose prev/next nav (qc3 W-QC3-001). For
 * normal-sized Works the first page returns `has_more: false` and the effect
 * never fires, so there is no over-fetch.
 *
 * Returns `prev: null, next: null` (with `loading: true`) while pages are
 * still loading, so callers can avoid rendering misleading "first/last
 * chapter" placeholders during the walk.
 */
export function useChapterNeighbors(
  workId: string | undefined,
  chapter: number | undefined,
  volume: number | undefined,
): ChapterNeighbors {
  const chapters = useChapters(workId || undefined, { limit: NEIGHBOR_PAGE_LIMIT });
  const rows = useMemo(() => flattenPages(chapters.data), [chapters.data]);

  // Cursor-walk every page when the server paginates. Guarded by `hasNextPage`
  // so normal-sized Works (first page returns `has_more: false`) never fetch a
  // second page. Long Works walk 2-3 pages; chapters are rarely in the 100s.
  useEffect(() => {
    if (chapters.hasNextPage && !chapters.isFetchingNextPage) {
      void chapters.fetchNextPage();
    }
  }, [chapters.hasNextPage, chapters.isFetchingNextPage, chapters.fetchNextPage]);

  // Still resolving: page-1 fetch in flight, more pages to walk, or a walk
  // fetch in flight. Once all pages land, all three are false.
  const loading = chapters.isLoading || chapters.hasNextPage || chapters.isFetchingNextPage;

  return useMemo<ChapterNeighbors>(() => {
    if (chapter === undefined) {
      return { chapters: rows, prev: null, next: null, volumes: deriveVolumes(rows), loading };
    }
    const idx = rows.findIndex((r) => matchCurrent(r, chapter, volume));
    if (idx === -1) {
      // Not in the loaded rows yet. If `loading`, the chapter may live on a
      // not-yet-fetched page — keep neighbors null without implying first/last.
      return { chapters: rows, prev: null, next: null, volumes: deriveVolumes(rows), loading };
    }
    return {
      chapters: rows,
      prev: idx > 0 ? rows[idx - 1] : null,
      next: idx >= 0 && idx < rows.length - 1 ? rows[idx + 1] : null,
      volumes: deriveVolumes(rows),
      loading,
    };
  }, [rows, chapter, volume, loading]);
}

function deriveVolumes(rows: ChapterSummary[]): number[] {
  // `volume` is contract-guaranteed (ChapterSummary.volume: number, >= 1).
  const set = new Set<number>();
  for (const r of rows) set.add(r.volume);
  return [...set].sort((a, b) => a - b);
}

/**
 * Count non-terminal findings scoped to a chapter (the actionable count while
 * reading). Reuses the existing `useFindings` cache; the comma-separated status
 * filter is enforced server-side (`list_findings_handler`).
 *
 * The `PaginationInfo` envelope carries no `total` field, so an exact count
 * beyond one page would require cursor-walking every page (loading full
 * `FindingDetailResponse` rows just to count them). Instead, when the last
 * loaded page reports `has_more`, the count is a lower bound and `truncated`
 * is true — callers render an honest "N+" label rather than an exact-looking
 * but clipped integer (qc3 W-QC3-002).
 */
export function useOpenFindingsCount(
  workId: string | undefined,
  chapter: number | undefined,
): { count: number; isLoading: boolean; truncated: boolean } {
  const findings = useFindings(workId || undefined, {
    status: OPEN_FINDING_STATUSES,
    chapter,
    limit: NEIGHBOR_PAGE_LIMIT,
  });
  const rows = useMemo(() => flattenPages(findings.data), [findings.data]);
  const pages = findings.data?.pages;
  const lastPage = pages && pages.length > 0 ? pages[pages.length - 1] : undefined;
  const truncated = Boolean(lastPage?.pagination.has_more);
  return { count: rows.length, isLoading: findings.isLoading, truncated };
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
