/**
 * World KB page — route entry for the Canvas World KB surface (V1.73 β).
 *
 * Reads the world id from the URL and renders {@link WorldKbCanvas}. The page
 * is thin so the canvas can be tested in isolation. Reached from the Work
 * detail page (a Work is bound to a World via `world_id`).
 *
 * Route-split: this page (and therefore `@xyflow/react`) is lazy-loaded by
 * `App.tsx` so React Flow never enters the Control Room bootstrap chunk.
 */
import { useParams } from 'react-router-dom';

import { WorldKbCanvas } from '@/components/canvas/world-kb/world-kb-canvas';
import { NotFoundPage } from '@/pages/not-found-page';

export function WorldKbPage() {
  const { worldId } = useParams<{ worldId: string }>();
  if (!worldId) return <NotFoundPage />;
  return <WorldKbCanvas worldId={worldId} />;
}
