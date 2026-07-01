/**
 * ChapterNav — V1.79 Author Reflection (Track A / P0).
 *
 * Prev/next chapter navigation within a Work, with volume-grouping awareness
 * for multi-volume Works. The prev/next targets are derived from the Work's
 * chapter list (resolved by {@link useChapterNeighbors}); the keyboard
 * shortcuts (←/→) are wired by the parent page so the nav stays a pure
 * affordance surface here.
 *
 * DESIGN.md §reading-chapter-nav tokens document the chrome mapping; the
 * controls compose `button.secondary` and the chrome uses the standard
 * background/border primitives (see DESIGN.md component table).
 */
import { Link } from 'react-router-dom';
import { ChevronLeft, ChevronRight } from 'lucide-react';

import { Button } from '@/components/ui/button';
import type { ChapterSummary } from '@42ch/nexus-contracts';

interface ChapterNavProps {
  workId: string;
  /** Immediately preceding chapter, or null when reading the first chapter. */
  prev: ChapterSummary | null;
  /** Immediately following chapter, or null when reading the last chapter. */
  next: ChapterSummary | null;
  /** Distinct volume numbers in the Work (drives the volume-grouping chip). */
  volumes: number[];
  /** Volume the current chapter belongs to (for the "in Volume N" label). */
  currentVolume?: number;
  /**
   * True while the chapter list is still being walked. While true, a null
   * `prev`/`next` may simply mean the relevant page has not loaded yet, so the
   * nav renders a neutral loading placeholder instead of the misleading "First
   * chapter"/"Last chapter" labels (qc3 W-QC3-001).
   */
  loading?: boolean;
}

function chapterHref(workId: string, row: ChapterSummary): string {
  // `volume` is contract-guaranteed (ChapterSummary.volume: number, >= 1), so
  // no defensive fallback is needed (R-V179P0-QC1-002).
  return `/works/${encodeURIComponent(workId)}/chapters/${row.chapter}?volume=${row.volume}`;
}

function chapterLabel(row: ChapterSummary): string {
  return row.title?.trim() ? row.title : `Chapter ${row.chapter}`;
}

export function ChapterNav({ workId, prev, next, volumes, currentVolume, loading = false }: ChapterNavProps) {
  const multiVolume = volumes.length > 1;
  return (
    <nav
      aria-label="Chapter navigation"
      className="flex flex-wrap items-center justify-between gap-3 rounded-card border border-gray-alpha-400 bg-background-200 px-4 py-3"
    >
      <div className="flex min-w-0 items-center gap-2">
        {prev ? (
          <Button asChild variant="secondary" size="small">
            <Link to={chapterHref(workId, prev)} aria-label={`Previous chapter: ${chapterLabel(prev)}`}>
              <ChevronLeft className="h-4 w-4" aria-hidden />
              <span className="truncate">{chapterLabel(prev)}</span>
            </Link>
          </Button>
        ) : loading ? (
          <span
            className="inline-flex h-8 items-center gap-1 rounded-control border border-gray-alpha-300 px-3 text-copy-13 text-gray-700"
            aria-label="Loading chapters"
          >
            <ChevronLeft className="h-4 w-4" aria-hidden />
            Loading chapters…
          </span>
        ) : (
          <span
            className="inline-flex h-8 items-center gap-1 rounded-control border border-gray-alpha-300 px-3 text-copy-13 text-gray-700"
            aria-label="No previous chapter"
          >
            <ChevronLeft className="h-4 w-4" aria-hidden />
            First chapter
          </span>
        )}
      </div>

      <div className="flex items-center gap-2 text-copy-13 text-gray-700">
        {multiVolume && (
          <span
            className="rounded-pill border border-gray-alpha-300 bg-background-300 px-2 py-0.5 text-label-12"
            aria-label={`Volume ${currentVolume ?? 1}`}
          >
            Volume {currentVolume ?? 1}
          </span>
        )}
        <span aria-hidden className="hidden sm:inline">
          Use ← → to navigate
        </span>
      </div>

      <div className="flex min-w-0 items-center gap-2">
        {next ? (
          <Button asChild variant="secondary" size="small">
            <Link to={chapterHref(workId, next)} aria-label={`Next chapter: ${chapterLabel(next)}`}>
              <span className="truncate">{chapterLabel(next)}</span>
              <ChevronRight className="h-4 w-4" aria-hidden />
            </Link>
          </Button>
        ) : loading ? (
          <span
            className="inline-flex h-8 items-center gap-1 rounded-control border border-gray-alpha-300 px-3 text-copy-13 text-gray-700"
            aria-label="Loading chapters"
          >
            Loading chapters…
            <ChevronRight className="h-4 w-4" aria-hidden />
          </span>
        ) : (
          <span
            className="inline-flex h-8 items-center gap-1 rounded-control border border-gray-alpha-300 px-3 text-copy-13 text-gray-700"
            aria-label="No next chapter"
          >
            Last chapter
            <ChevronRight className="h-4 w-4" aria-hidden />
          </span>
        )}
      </div>
    </nav>
  );
}
