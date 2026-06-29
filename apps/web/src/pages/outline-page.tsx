/**
 * Outline page — route entry for the Canvas Outline+Timeline surface (V1.72 β).
 *
 * Reads the work id from the URL and renders {@link OutlineCanvas}. The page
 * itself is thin so the canvas can be tested in isolation.
 *
 * V1.75 (F-QC3-001): consumes an optional `?chapter=N` query param so the
 * chapter-page "Edit outline → Canvas" CTA can preselect the chapter node and
 * open its inspector on mount. The param is read once and threaded in as the
 * canvas's initial selection; it does not override later user clicks.
 */
import { useParams, useSearchParams } from 'react-router-dom';

import { OutlineCanvas } from '@/components/canvas/outline-canvas';
import { NotFoundPage } from '@/pages/not-found-page';

export function OutlinePage() {
  const { workId } = useParams<{ workId: string }>();
  const [searchParams] = useSearchParams();

  const chapterParam = searchParams.get('chapter');
  const parsed = chapterParam === null ? NaN : Number(chapterParam);
  const initialSelectedChapterId =
    Number.isFinite(parsed) && parsed > 0 ? parsed : null;

  if (!workId) return <NotFoundPage />;
  return (
    <OutlineCanvas
      workId={workId}
      initialSelectedChapterId={initialSelectedChapterId}
    />
  );
}
