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
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import type { ChapterSummary } from '@42ch/nexus-contracts';

interface ChapterNavProps {
  workId: string;
  prev: ChapterSummary | null;
  next: ChapterSummary | null;
  volumes: number[];
  currentVolume?: number;
  loading?: boolean;
}

function chapterHref(workId: string, row: ChapterSummary): string {
  return `/works/${encodeURIComponent(workId)}/chapters/${row.chapter}?volume=${row.volume}`;
}

function chapterLabel(row: ChapterSummary, t: (key: string, options?: Record<string, unknown>) => string): string {
  return row.title?.trim() ? row.title : t('chapter.title', { chapter: row.chapter });
}

export function ChapterNav({ workId, prev, next, volumes, currentVolume, loading = false }: ChapterNavProps) {
  const { t } = useTranslation('reading');
  const multiVolume = volumes.length > 1;
  return (
    <nav
      aria-label={t('nav.ariaLabel')}
      className="flex flex-wrap items-center justify-between gap-3 rounded-card border border-gray-alpha-400 bg-background-200 px-4 py-3"
    >
      <div className="flex min-w-0 items-center gap-2">
        {prev ? (
          <Button asChild variant="secondary" size="small">
            <Link to={chapterHref(workId, prev)} aria-label={t('nav.previousAria', { label: chapterLabel(prev, t) })}>
              <ChevronLeft className="h-4 w-4" aria-hidden />
              <span className="truncate">{chapterLabel(prev, t)}</span>
            </Link>
          </Button>
        ) : loading ? (
          <span
            className="inline-flex h-8 items-center gap-1 rounded-control border border-gray-alpha-300 px-3 text-copy-13 text-gray-700"
            aria-label={t('nav.loadingChapters')}
          >
            <ChevronLeft className="h-4 w-4" aria-hidden />
            {t('nav.loadingChapters')}
          </span>
        ) : (
          <span
            className="inline-flex h-8 items-center gap-1 rounded-control border border-gray-alpha-300 px-3 text-copy-13 text-gray-700"
            aria-label={t('nav.noPrevious')}
          >
            <ChevronLeft className="h-4 w-4" aria-hidden />
            {t('nav.firstChapter')}
          </span>
        )}
      </div>

      <div className="flex items-center gap-2 text-copy-13 text-gray-700">
        {multiVolume && (
          <span
            className="rounded-pill border border-gray-alpha-300 bg-background-300 px-2 py-0.5 text-label-12"
            aria-label={t('nav.volume', { volume: currentVolume ?? 1 })}
          >
            {t('nav.volume', { volume: currentVolume ?? 1 })}
          </span>
        )}
        <span aria-hidden className="hidden sm:inline">
          {t('nav.keyboardHint')}
        </span>
      </div>

      <div className="flex min-w-0 items-center gap-2">
        {next ? (
          <Button asChild variant="secondary" size="small">
            <Link to={chapterHref(workId, next)} aria-label={t('nav.nextAria', { label: chapterLabel(next, t) })}>
              <span className="truncate">{chapterLabel(next, t)}</span>
              <ChevronRight className="h-4 w-4" aria-hidden />
            </Link>
          </Button>
        ) : loading ? (
          <span
            className="inline-flex h-8 items-center gap-1 rounded-control border border-gray-alpha-300 px-3 text-copy-13 text-gray-700"
            aria-label={t('nav.loadingChapters')}
          >
            {t('nav.loadingChapters')}
            <ChevronRight className="h-4 w-4" aria-hidden />
          </span>
        ) : (
          <span
            className="inline-flex h-8 items-center gap-1 rounded-control border border-gray-alpha-300 px-3 text-copy-13 text-gray-700"
            aria-label={t('nav.noNext')}
          >
            {t('nav.lastChapter')}
            <ChevronRight className="h-4 w-4" aria-hidden />
          </span>
        )}
      </div>
    </nav>
  );
}
