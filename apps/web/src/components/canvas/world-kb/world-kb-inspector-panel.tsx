/**
 * World KB inspector panel (V1.74 A10 split).
 *
 * Routes the current selection to the appropriate inspector:
 * entity → EntityInspector, candidate → PromotionInspector, none → placeholder.
 */
import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';

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
  /** Optional adapter-driven inspector for node-based selections (graph mode). */
  nodeInspector?: ReactNode;
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
  nodeInspector,
}: InspectorPanelProps) {
  const { t } = useTranslation('canvas');
  if (!selection) {
    return (
      <aside
        aria-label={t('worldKb.inspector.ariaLabel')}
        className="rounded-card border border-gray-alpha-400 bg-background-100 p-4 text-copy-13 text-gray-700 shadow-card"
      >
        {t('worldKb.inspector.empty')}
      </aside>
    );
  }
  if (selection.kind === 'entity') {
    return (
      <aside
        aria-label={t('worldKb.inspector.entityAria', { name: selection.node.name })}
        className="rounded-card border border-gray-alpha-400 bg-background-100 p-4 shadow-card"
      >
        {nodeInspector ?? (
          <EntityInspector
            worldId={worldId}
            node={selection.node}
            entity={selection.entity}
            onConflict={onEntityConflict}
            reseedSignal={reseedSignal}
          />
        )}
      </aside>
    );
  }
  if (selection.kind === 'candidate') {
    return (
      <aside
        aria-label={t('worldKb.inspector.candidateAria', { name: selection.node.name })}
        className="rounded-card border border-gray-alpha-400 bg-background-100 p-4 shadow-card"
      >
        {nodeInspector ?? (
          <PromotionInspector
            worldId={worldId}
            node={selection.node}
            candidate={selection.candidate}
            confirmedEntities={confirmedEntities}
            onConflict={onPromoteConflict}
            reseedSignal={reseedSignal}
          />
        )}
      </aside>
    );
  }
  if (selection.kind === 'new-relationship') {
    return (
      <aside
        aria-label={t('worldKb.inspector.newRelationshipAria')}
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
      aria-label={t('worldKb.inspector.relationshipAria', { id: selection.relationship.relationship_id })}
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
