/**
 * World KB inspector panel (V1.74 A10 split).
 *
 * Routes the current selection to the appropriate inspector:
 * entity → EntityInspector, candidate → PromotionInspector, none → placeholder.
 */
import type {
  WorldKbEntityProjection,
  WorldKbSourceAnchorProjection,
} from '@42ch/nexus-contracts';

import { EntityInspector, type EntityEditForm } from './entity-inspector';
import { PromotionInspector } from './promotion-inspector';
import { RelationshipInspector, type RelationshipForm } from './relationship-inspector';
import type { Selection } from './world-kb-canvas-types';

interface InspectorPanelProps {
  selection: Selection;
  worldId: string;
  confirmedEntities: WorldKbEntityProjection[];
  anchors: WorldKbSourceAnchorProjection[];
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
  onRelationshipConflict: (payload: {
    currentVersion: number;
    relationshipId: string;
    draft: RelationshipForm;
  }) => void;
  onRelationshipSaved: () => void;
}

export function InspectorPanel({
  selection,
  worldId,
  confirmedEntities,
  anchors,
  reseedSignal,
  onEntityConflict,
  onPromoteConflict,
  onRelationshipConflict,
  onRelationshipSaved,
}: InspectorPanelProps) {
  if (!selection) {
    return (
      <aside
        aria-label="World KB inspector"
        className="rounded-card border border-gray-alpha-400 bg-background-100 p-4 text-copy-13 text-gray-700 shadow-card"
      >
        Select an entity, candidate, or relationship to inspect it.
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
  if (selection.kind === 'candidate') {
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
  if (selection.kind === 'new-relationship') {
    return (
      <aside
        aria-label="New relationship inspector"
        className="rounded-card border border-gray-alpha-400 bg-background-100 p-4 shadow-card"
      >
        <RelationshipInspector
          worldId={worldId}
          initialSourceEntityId={selection.initialSourceEntityId}
          initialTargetEntityId={selection.initialTargetEntityId}
          entities={confirmedEntities}
          anchors={anchors}
          onSaved={onRelationshipSaved}
        />
      </aside>
    );
  }
  return (
    <aside
      aria-label={`Relationship inspector: ${selection.relationship.relationship_id}`}
      className="rounded-card border border-gray-alpha-400 bg-background-100 p-4 shadow-card"
    >
      <RelationshipInspector
        worldId={worldId}
        relationship={selection.relationship}
        entities={confirmedEntities}
        anchors={anchors}
        onConflict={onRelationshipConflict}
        onSaved={onRelationshipSaved}
      />
    </aside>
  );
}
