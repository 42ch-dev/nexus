/**
 * ChapterPage — V1.79 Author Reflection (Track A / P0) manuscript reading surface.
 *
 * Promotes the post-V1.75-pivot residual (bare read-only body render) into a
 * designed reading experience: legible prose typography, chapter/volume
 * navigation (←/→ keyboard + prev/next controls), session-only reading
 * progress, and in-context lightweight maturation indicators. Read-only — no
 * write route; the only edit affordance routes back to the canvas (the sole
 * authoring surface per the V1.75 pivot).
 *
 * V1.75 residuals preserved verbatim: body prose render + frontmatter strip,
 * Copy Path, the body right-click context menu, and the "Edit outline →
 * Canvas" redirect CTA. See {@link ReadingProse} for the prose surface and
 * {@link MaturationIndicators} / {@link ChapterNav} for the V1.79 additions.
 */
import { useEffect, useMemo } from 'react';
import { Link, useNavigate, useParams, useSearchParams } from 'react-router-dom';
import { ArrowLeft, ArrowUpRight } from 'lucide-react';

import { ChapterNav } from '@/components/reading/chapter-nav';
import { MaturationIndicators } from '@/components/reading/maturation-indicators';
import { ReadingProgress } from '@/components/reading/reading-progress';
import { ReadingProse } from '@/components/reading/reading-prose';
import { useChapterNeighbors } from '@/components/reading/reading-hooks';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { ErrorState, LoadingState } from '@/components/ui/states';
import { useChapter, useChapterBody } from '@/api/queries';
import { formatRelative } from '@/lib/format';

export function ChapterPage() {
  const { workId = '', chapter: chapterParam = '' } = useParams();
  const chapterNumber = Number(chapterParam);
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();

  const volumeQuery = useMemo(() => {
    const raw = searchParams.get('volume');
    const n = raw === null ? undefined : Number(raw);
    return n !== undefined && n > 0 ? { volume: n } : undefined;
  }, [searchParams]);
  const currentVolume = volumeQuery?.volume;

  const chapter = useChapter(workId || undefined, chapterNumber || undefined, volumeQuery);
  const body = useChapterBody(workId || undefined, chapterNumber || undefined, volumeQuery);
  const neighbors = useChapterNeighbors(workId || undefined, chapterNumber || undefined, currentVolume);

  // Keyboard navigation (←/→) — wired here so the nav component stays a pure
  // affordance. Guarded against input/textarea/contenteditable focus and open
  // menus/dialogs so typing in a field or using the context menu never
  // hijacks chapter navigation.
  useChapterKeyboardNav(workId, neighbors, navigate);

  if (chapter.isLoading) return <LoadingState label="Loading chapter…" />;
  if (chapter.isError || !chapter.data) {
    return (
      <ErrorState
        description="Could not load this chapter. It may not exist or the daemon could not return it."
        onRetry={() => chapter.refetch()}
      />
    );
  }

  const ch = chapter.data;
  const canvasHref = `/works/${encodeURIComponent(workId)}/outline?chapter=${ch.chapter}`;
  // Key the progress bar on chapter so it resets when the reader navigates.
  const progressKey = `${workId}:${ch.chapter}:${ch.volume ?? 1}`;

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex flex-wrap items-center gap-2">
          <Button asChild variant="tertiary" size="small">
            <Link to={`/works/${encodeURIComponent(workId)}/chapters`}>
              <ArrowLeft className="h-4 w-4" aria-hidden />Back to Chapters
            </Link>
          </Button>
          <span className="text-heading-20 font-heading tracking-tight text-gray-1000">
            Chapter {ch.chapter}
          </span>
          <MaturationIndicators workId={workId} chapter={ch.chapter} status={ch.status} />
        </div>
        <div className="text-copy-13 text-gray-700">
          Updated {formatRelative(ch.updated_at)}
        </div>
      </div>

      <ReadingProgress key={progressKey} />

      <ChapterNav
        workId={workId}
        prev={neighbors.prev}
        next={neighbors.next}
        volumes={neighbors.volumes}
        currentVolume={ch.volume ?? currentVolume}
        loading={neighbors.loading}
      />

      <Card className="shadow-card">
        <CardHeader className="pb-3">
          <CardTitle>Outline editing moved to Canvas</CardTitle>
          <CardDescription>
            The whole-document outline editor was retired in V1.75. Edit this chapter&rsquo;s outline
            on the outline canvas.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <Button asChild variant="primary" size="small">
            <Link
              to={canvasHref}
              aria-label={`Edit outline for Chapter ${ch.chapter} on the outline canvas`}
            >
              Edit outline → Canvas <ArrowUpRight className="h-4 w-4" aria-hidden />
            </Link>
          </Button>
        </CardContent>
      </Card>

      <ReadingProse
        body={body.data}
        isLoading={body.isLoading}
        isError={body.isError}
        onRetry={() => body.refetch()}
      />
    </div>
  );
}

/**
 * Wire ←/→ keyboard navigation between chapters. The effect is a no-op when the
 * reader is focused in an editable element or a menu/dialog is open, so it
 * never competes with form input or the body context menu.
 */
function useChapterKeyboardNav(
  workId: string,
  neighbors: { prev: { chapter: number; volume: number } | null; next: { chapter: number; volume: number } | null },
  navigate: ReturnType<typeof useNavigate>,
) {
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.defaultPrevented) return;
      if (e.metaKey || e.ctrlKey || e.altKey) return;
      const el = document.activeElement;
      if (el && (isEditable(el) || hasOpenOverlay())) return;
      const target = e.key === 'ArrowLeft' ? neighbors.prev : e.key === 'ArrowRight' ? neighbors.next : null;
      if (!target) return;
      e.preventDefault();
      const v = target.volume ?? 1;
      navigate(`/works/${encodeURIComponent(workId)}/chapters/${target.chapter}?volume=${v}`);
    }
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [workId, neighbors, navigate]);
}

function isEditable(el: Element): boolean {
  const tag = el.tagName;
  return (
    tag === 'INPUT' ||
    tag === 'TEXTAREA' ||
    tag === 'SELECT' ||
    (el as HTMLElement).isContentEditable === true
  );
}

function hasOpenOverlay(): boolean {
  // A visible menu or dialog captures keyboard intent; do not navigate.
  return Boolean(document.querySelector('[role="menu"]:not([hidden]), [role="dialog"]:not([hidden])'));
}
