/**
 * World KB inspector panel (V1.74 A10 split).
 *
 * Routes the current selection to the appropriate inspector:
 * entity → EntityInspector, candidate → PromotionInspector, none → placeholder.
 */
import type { WorldKbEntityProjection } from '@42ch/nexus-contracts';

import { EntityInspector, type EntityEditForm } from './entity-inspector';
import { PromotionInspector } from './promotion-inspector';
import type { Selection } from './world-kb-canvas-types';

interface InspectorPanelProps {
  selection: Selection;
  worldId: string;
  confirmedEntities: WorldKbEntityProjection[];
  reseedSignal: number;
  onEntityConflict: (payload: {
    currentVersion: number;
    entityId: string;
    conflictingPath: string;
    draft: EntityEditForm;
    dirtyFields: ('title' | 'body' | 'aliases' | 'block_type')[];
  }) => void;
  onPromoteConflict: (payload: {
    currentVersion: number;
    candidateName: string;
    newStatus: 'adopted' | 'rejected' | 'merged';
    action: 'adopt' | 'reject' | 'merge';
    mergeTargetId?: string;
    mergeTargetLabel?: string;
  }) => void;
}

export function InspectorPanel({
  selection,
  worldId,
  confirmedEntities,
  reseedSignal,
  onEntityConflict,
  onPromoteConflict,
}: InspectorPanelProps) {
  if (!selection) {
    return (
      <aside
        aria-label="World KB inspector"
        className="rounded-card border border-gray-alpha-400 bg-background-100 p-4 text-copy-13 text-gray-700 shadow-card"
      >
        Select an entity or candidate to inspect it.
      </aside>
    );
  }
  if (selection.kind === 'entity') {
    return (
      <aside
        aria-label={`Entity inspector: ${selection.node.name}`}
        className="rounded-card border border-gray-alpha-400 bg-background-100 p-4 shadow-card"
      >
        <EntityInspector
          worldId={worldId}
          node={selection.node}
          entity={selection.entity}
          onConflict={onEntityConflict}
          reseedSignal={reseedSignal}
        />
      </aside>
    );
  }
  return (
    <aside
      aria-label={`Promotion inspector: ${selection.node.name}`}
      className="rounded-card border border-gray-alpha-400 bg-background-100 p-4 shadow-card"
    >
      <PromotionInspector
        worldId={worldId}
        node={selection.node}
        candidate={selection.candidate}
        confirmedEntities={confirmedEntities}
        onConflict={onPromoteConflict}
        reseedSignal={reseedSignal}
      />
    </aside>
  );
}
