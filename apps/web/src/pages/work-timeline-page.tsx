/**
 * Work Timeline page — route entry for the Work Timeline peer surface
 * (V1.123 P2 Task 5).
 *
 * Reads the work id from the URL and renders {@link WorkTimelineCanvas}. The
 * page is thin so the canvas can be tested in isolation. Peer surfaces
 * (Outline, Strategy via `go.strategy`, World KB) are reachable from the
 * command palette (`CanvasNavCommands`) and the Work Timeline header.
 *
 * Work entry stays Outline (V1.118 regression gate) — `/works/:workId` does
 * NOT redirect here. The Work Timeline is reachable as a peer at
 * `/works/:workId/timeline` (sibling route, NOT the index). The index
 * redirect at `/works/:workId` still points to `outline` (see `App.tsx`).
 *
 * Route-split: this page (and therefore `@xyflow/react`) is lazy-loaded by
 * `App.tsx` so React Flow never enters the Control Room bootstrap chunk.
 */
import { useParams } from 'react-router-dom';

import { WorkTimelineCanvas } from '@/components/canvas/work-timeline-canvas/work-timeline-canvas';
import { NotFoundPage } from '@/pages/not-found-page';

export function WorkTimelinePage() {
  const { workId } = useParams<{ workId: string }>();
  if (!workId) return <NotFoundPage />;
  return <WorkTimelineCanvas workId={workId} />;
}
