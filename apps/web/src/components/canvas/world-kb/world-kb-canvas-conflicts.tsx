/**
 * World KB canvas conflict hosts composition (V1.74 A10 split).
 *
 * Extracted from `world-kb-canvas.tsx` so the orchestrator facade stays under
 * the 250-line cap. Composes the entity, promotion, and relationship conflict
 * hosts with their dismissal / refetch callbacks.
 */
import type { WorldKbEntityProjection } from '@42ch/nexus-contracts';

import {
  EntityConflictHost,
  PromoteConflictHost,
  RelationshipConflictHost,
} from './world-kb-conflict-hosts';
import type {
  EntityConflictState,
  PromoteConflictState,
  RelationshipConflictState,
  Selection,
} from './world-kb-canvas-types';

export interface WorldKbCanvasConflictsProps {
  entityConflict: EntityConflictState | null;
  promoteConflict: PromoteConflictState | null;
  relationshipConflict: RelationshipConflictState | null;
  selection: Selection;
  worldId: string;
  confirmedEntities: WorldKbEntityProjection[];
  setEntityConflict: (value: EntityConflictState | null) => void;
  setPromoteConflict: (value: PromoteConflictState | null) => void;
  setRelationshipConflict: (value: RelationshipConflictState | null) => void;
  bumpReseed: () => void;
  refetchGraph: () => void;
  refetchCandidates: () => void;
}

export function WorldKbCanvasConflicts({
  entityConflict,
  promoteConflict,
  relationshipConflict,
  selection,
  worldId,
  confirmedEntities,
  setEntityConflict,
  setPromoteConflict,
  setRelationshipConflict,
  bumpReseed,
  refetchGraph,
  refetchCandidates,
}: WorldKbCanvasConflictsProps) {
  return (
    <>
      <EntityConflictHost
        state={entityConflict}
        selection={selection}
        worldId={worldId}
        onUseCurrent={() => {
          setEntityConflict(null);
          bumpReseed();
          refetchGraph();
        }}
        onDismiss={() => setEntityConflict(null)}
        onResolved={() => {
          setEntityConflict(null);
          bumpReseed();
        }}
      />

      <PromoteConflictHost
        state={promoteConflict}
        selection={selection}
        worldId={worldId}
        onUseCurrent={() => {
          setPromoteConflict(null);
          bumpReseed();
          refetchGraph();
          refetchCandidates();
        }}
        onDismiss={() => setPromoteConflict(null)}
        onResolved={() => {
          setPromoteConflict(null);
          bumpReseed();
        }}
      />

      <RelationshipConflictHost
        state={relationshipConflict}
        selection={selection}
        worldId={worldId}
        entities={confirmedEntities}
        onUseCurrent={() => {
          setRelationshipConflict(null);
          bumpReseed();
          refetchGraph();
        }}
        onDismiss={() => setRelationshipConflict(null)}
        onResolved={() => {
          setRelationshipConflict(null);
          bumpReseed();
        }}
      />
    </>
  );
}
