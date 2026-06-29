/**
 * World KB canvas orchestrator state hook (V1.74 A6).
 *
 * Encapsulates selection, conflict state, reseed signaling, and selection
 * lifecycle so the thin canvas facade stays focused on rendering and composition.
 */
import { useEffect, useState } from 'react';
import type { Edge } from '@xyflow/react';

import type {
  WorldKbCandidateProjection,
  WorldKbEntityProjection,
  WorldKbRelationshipProjection,
} from '@42ch/nexus-contracts';

import type { RelationshipForm } from './relationship-inspector';
import type { EntityEditForm } from './entity-inspector';
import type {
  EntityConflictState,
  PromoteConflictState,
  RelationshipConflictState,
  Selection,
} from './world-kb-canvas-types';
import { worldKbNodeId, type WorldKbNodeData } from './types';

interface UseWorldKbCanvasStateArgs {
  entities: WorldKbEntityProjection[];
  candidateItems: WorldKbCandidateProjection[];
  relationships: WorldKbRelationshipProjection[];
}

export function useWorldKbCanvasState({
  entities,
  candidateItems,
  relationships,
}: UseWorldKbCanvasStateArgs) {
  const [selection, setSelection] = useState<Selection>(null);
  const [entityConflict, setEntityConflict] = useState<EntityConflictState | null>(null);
  const [promoteConflict, setPromoteConflict] = useState<PromoteConflictState | null>(null);
  const [relationshipConflict, setRelationshipConflict] = useState<RelationshipConflictState | null>(null);
  const [reseedSignal, setReseedSignal] = useState(0);

  function bumpReseed() {
    setReseedSignal((s) => s + 1);
  }

  // Reset selection when the graph refetch drops the backing projection.
  useEffect(() => {
    if (!selection) return;
    if (selection.kind === 'entity') {
      const fresh = entities.find((e) => e.key_block_id === selection.entity.key_block_id);
      if (!fresh) setSelection(null);
    } else if (selection.kind === 'candidate') {
      const fresh = candidateItems.find((c) => c.candidate_id === selection.candidate.candidate_id);
      if (!fresh) setSelection(null);
    } else if (selection.kind === 'relationship') {
      const fresh = relationships.find((r) => r.relationship_id === selection.relationship.relationship_id);
      if (!fresh) setSelection(null);
    }
    // new-relationship selection is transient and never needs invalidation.
  }, [entities, candidateItems, relationships, selection]);

  function onSelectNode(node: WorldKbNodeData) {
    if (node.candidateId) {
      const candidate = candidateItems.find((c) => c.candidate_id === node.candidateId);
      if (candidate) setSelection({ kind: 'candidate', node, candidate });
    } else if (node.keyBlockId) {
      const entity = entities.find((e) => e.key_block_id === node.keyBlockId);
      if (entity) setSelection({ kind: 'entity', node, entity });
    }
  }

  function onEdgeClick(_event: React.MouseEvent, edge: Edge) {
    if (typeof edge.id !== 'string' || !edge.id.startsWith('relationship:')) return;
    const relationshipId = edge.id.split(':')[1];
    const relationship = relationships.find((r) => r.relationship_id === relationshipId);
    if (relationship) {
      setSelection({ kind: 'relationship', relationship });
    }
  }

  function onSelectRelationship(relationship: WorldKbRelationshipProjection) {
    setSelection({ kind: 'relationship', relationship });
  }

  function onCreateRelationship(initial?: { sourceEntityId?: string; targetEntityId?: string }) {
    setSelection({
      kind: 'new-relationship',
      initialSourceEntityId: initial?.sourceEntityId,
      initialTargetEntityId: initial?.targetEntityId,
    });
  }

  const selectedNodeId =
    selection && selection.kind !== 'relationship' && selection.kind !== 'new-relationship'
      ? worldKbNodeId(selection.node)
      : null;

  const selectedRelationshipId =
    selection?.kind === 'relationship' ? selection.relationship.relationship_id : null;

  return {
    selection,
    setSelection,
    selectedNodeId,
    selectedRelationshipId,
    entityConflict,
    promoteConflict,
    relationshipConflict,
    reseedSignal,
    bumpReseed,
    setEntityConflict,
    setPromoteConflict,
    setRelationshipConflict,
    onSelectNode,
    onSelectRelationship,
    onCreateRelationship,
    onEdgeClick,
  };
}

export function buildEntityConflict(
  selection: Selection,
  payload: {
    currentVersion: number;
    entityId: string;
    conflictingPath: string;
    draft: EntityEditForm;
    dirtyFields: ('title' | 'body' | 'aliases' | 'block_type')[];
  },
): EntityConflictState {
  const entityName = selection?.kind === 'entity' ? selection.node.name : 'this entity';
  const draftValues: Partial<Record<'title' | 'body' | 'aliases' | 'block_type', string>> = {};
  if (payload.dirtyFields.includes('title')) draftValues.title = payload.draft.title;
  if (payload.dirtyFields.includes('block_type')) draftValues.block_type = payload.draft.block_type;
  if (payload.dirtyFields.includes('aliases')) draftValues.aliases = payload.draft.aliasesText;
  if (payload.dirtyFields.includes('body')) draftValues.body = payload.draft.bodyText;
  return {
    currentVersion: payload.currentVersion,
    reapplyForm: payload.draft,
    dirtyFields: payload.dirtyFields,
    modalDraft: {
      entityName,
      fields: payload.dirtyFields,
      changedFields: [],
      draftValues,
    },
  };
}

export function handleRelationshipConflict(
  setRelationshipConflict: React.Dispatch<React.SetStateAction<RelationshipConflictState | null>>,
  payload: { currentVersion: number; relationshipId: string; draft: RelationshipForm },
) {
  setRelationshipConflict({
    currentVersion: payload.currentVersion,
    relationshipId: payload.relationshipId,
    draft: payload.draft,
  });
}

export function handlePromoteConflict(
  setPromoteConflict: React.Dispatch<React.SetStateAction<PromoteConflictState | null>>,
  payload: {
    currentVersion: number;
    candidateName: string;
    newStatus: 'adopted' | 'rejected' | 'merged';
    action: 'adopt' | 'reject' | 'merge';
    mergeTargetId?: string;
    mergeTargetLabel?: string;
  },
) {
  setPromoteConflict({ currentVersion: payload.currentVersion, draft: payload });
}
