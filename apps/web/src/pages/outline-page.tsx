/**
 * Outline page — route entry for the Canvas Outline+Timeline surface (V1.72 β).
 *
 * Reads the work id from the URL and renders {@link OutlineCanvas}. The page
 * itself is thin so the canvas can be tested in isolation.
 */
import { useParams } from 'react-router-dom';

import { OutlineCanvas } from '@/components/canvas/outline-canvas';
import { NotFoundPage } from '@/pages/not-found-page';

export function OutlinePage() {
  const { workId } = useParams<{ workId: string }>();
  if (!workId) return <NotFoundPage />;
  return <OutlineCanvas workId={workId} />;
}
