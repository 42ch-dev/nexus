/**
 * Timeline page — route entry for the Canvas Timeline hero surface
 * (V1.122 P1 T3). Default World entry: `/worlds/:worldId` redirects here.
 *
 * Reads the world id from the URL and renders {@link TimelineCanvas}. The
 * page is thin so the canvas can be tested in isolation. Peer surfaces
 * (World KB, Strategy) are reachable from the Timeline header + the canvas
 * shell nav.
 *
 * Work entry stays Outline (V1.118 regression gate) — `/works/:workId` does
 * NOT redirect here.
 *
 * Route-split: this page (and therefore `@xyflow/react`) is lazy-loaded by
 * `App.tsx` so React Flow never enters the Control Room bootstrap chunk.
 */
import { useParams } from 'react-router';

import { TimelineCanvas } from '@/components/canvas/timeline-canvas/timeline-canvas';
import { NotFoundPage } from '@/pages/not-found-page';

export function TimelinePage() {
  const { worldId } = useParams<{ worldId: string }>();
  if (!worldId) return <NotFoundPage />;
  return <TimelineCanvas worldId={worldId} />;
}
