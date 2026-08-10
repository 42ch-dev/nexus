/**
 * World KB page — route entry for the Canvas World KB surface (V1.73 β).
 *
 * Reads the world id from the URL and renders {@link WorldKbCanvas}. The page
 * is thin so the canvas can be tested in isolation. Reached from the Work
 * detail page (a Work is bound to a World via `world_id`).
 *
 * V1.152 P1 (DF-77): the Pack panel (export + import of Narrative Knowledge
 * Packs) mounts below the canvas as a World-KB-home section — the product
 * home locked by product-manager §5.1 ("under the World-KB area"), with the
 * canvas as the hero and the pack section as the transport surface.
 *
 * Route-split: this page (and therefore `@xyflow/react`) is lazy-loaded by
 * `App.tsx` so React Flow never enters the Control Room bootstrap chunk.
 */
import { useParams } from 'react-router';

import { WorldKbCanvas } from '@/components/canvas/world-kb/world-kb-canvas';
import { PackPanel } from '@/components/pack/pack-panel';
import { NotFoundPage } from '@/pages/not-found-page';

export function WorldKbPage() {
  const { worldId } = useParams<{ worldId: string }>();
  if (!worldId) return <NotFoundPage />;
  return (
    <div className="flex flex-col gap-4" data-testid="world-kb-page">
      <WorldKbCanvas worldId={worldId} />
      <PackPanel worldId={worldId} />
    </div>
  );
}
