/**
 * World KB conflict modal hosts (V1.74 A10 split + A6 relationship variant).
 *
 * Bridges the canvas conflict state to the KB-flavored entity, promotion, and
 * relationship conflict modals. The "Reapply" path rebuilds the patch from the
 * captured form and bumps the local current-version on recurring 409s.
 */
import { useEffect, useState } from 'react';

import type { WorldKbEntityProjection } from '@42ch/nexus-contracts';
import {
  usePatchWorldKbEntity,
  usePatchWorldKbRelationship,
  usePromoteWorldKbCandidate,
  isWorldKbConflictError,
} from '@/lib/canvas/use-world-kb-data';

import {
  WorldKbEntityConflictModal,
  WorldKbPromoteConflictModal,
} from './world-kb-conflict-modal';
import { WorldKbRelationshipConflictModal } from './world-kb-relationship-conflict-modal';
import { entityName } from './relationship-inspector';
import type {
  EntityConflictState,
  PromoteConflictState,
  RelationshipConflictState,
  Selection,
} from './world-kb-canvas-types';
import { patchFromForm } from './world-kb-canvas-utils';

interface EntityConflictHostProps {
  state: EntityConflictState | null;
  selection: Selection;
  worldId: string;
  onUseCurrent: () => void;
  onDismiss: () => void;
  onResolved: () => void;
}

export function EntityConflictHost({
  state,
  selection,
  worldId,
  onUseCurrent,
  onDismiss,
  onResolved,
}: EntityConflictHostProps) {
  const patchEntity = usePatchWorldKbEntity(worldId);
  const [currentVersion, setCurrentVersion] = useState(state?.currentVersion ?? 0);
  useEffect(() => {
    setCurrentVersion(state?.currentVersion ?? 0);
  }, [state?.currentVersion]);

  if (!state || !selection || selection.kind !== 'entity') return null;

  function handleReapply() {
    if (!state || !selection || selection.kind !== 'entity') return;
    patchEntity.mutate(
      {
        entity_id: selection.entity.key_block_id,
        expected_version: currentVersion,
        patch: patchFromForm(state.reapplyForm, state.dirtyFields),
      },
      {
        onSuccess: () => {
          onResolved();
          onDismiss();
        },
        onError: (error) => {
          if (isWorldKbConflictError(error)) {
            setCurrentVersion(error.details.current_version);
          }
        },
      },
    );
  }

  return (
    <WorldKbEntityConflictModal
      open
      draft={state.modalDraft}
      currentVersion={currentVersion}
      onUseCurrent={onUseCurrent}
      onReapply={handleReapply}
      onDismiss={onDismiss}
    />
  );
}

interface RelationshipConflictHostProps {
  state: RelationshipConflictState | null;
  selection: Selection;
  worldId: string;
  entities: WorldKbEntityProjection[];
  onUseCurrent: () => void;
  onDismiss: () => void;
  onResolved: () => void;
}

export function RelationshipConflictHost({
  state,
  worldId,
  entities,
  onUseCurrent,
  onDismiss,
  onResolved,
}: RelationshipConflictHostProps) {
  const patchRelationship = usePatchWorldKbRelationship(worldId);
  const [currentVersion, setCurrentVersion] = useState(state?.currentVersion ?? 0);
  useEffect(() => {
    setCurrentVersion(state?.currentVersion ?? 0);
  }, [state?.currentVersion]);

  if (!state) return null;
  const sourceName = entityName(state.draft.sourceEntityId, entities);
  const targetName = entityName(state.draft.targetEntityId, entities);

  function buildRequest() {
    return {
      relationship_id: state!.relationshipId,
      action: 'update' as const,
      expected_version: currentVersion,
      relationship: {
        source_entity_id: state!.draft.sourceEntityId,
        target_entity_id: state!.draft.targetEntityId,
        relation_type: state!.draft.relationType,
        custom_label: state!.draft.relationType === 'custom' ? state!.draft.customLabel.trim() : undefined,
        symmetric: state!.draft.symmetric,
        confidence: state!.draft.confidence,
        source_anchor_ids: state!.draft.sourceAnchorIds,
      },
    };
  }

  function handleReapply() {
    patchRelationship.mutate(buildRequest(), {
      onSuccess: () => {
        onResolved();
        onDismiss();
      },
      onError: (error) => {
        if (isWorldKbConflictError(error)) {
          setCurrentVersion(error.details.current_version);
        }
      },
    });
  }

  return (
    <WorldKbRelationshipConflictModal
      open
      draft={{
        relationshipId: state.relationshipId,
        sourceName,
        targetName,
        form: state.draft,
      }}
      currentVersion={currentVersion}
      onUseCurrent={onUseCurrent}
      onReapply={handleReapply}
      onDismiss={onDismiss}
    />
  );
}

interface PromoteConflictHostProps {
  state: PromoteConflictState | null;
  selection: Selection;
  worldId: string;
  onUseCurrent: () => void;
  onDismiss: () => void;
  onResolved: () => void;
}

export function PromoteConflictHost({
  state,
  selection,
  worldId,
  onUseCurrent,
  onDismiss,
  onResolved,
}: PromoteConflictHostProps) {
  const promoteCandidate = usePromoteWorldKbCandidate(worldId);
  const [currentVersion, setCurrentVersion] = useState(state?.currentVersion ?? 0);
  useEffect(() => {
    setCurrentVersion(state?.currentVersion ?? 0);
  }, [state?.currentVersion]);

  if (!state || !selection || selection.kind !== 'candidate') return null;

  function handleReapply() {
    if (!state || !selection || selection.kind !== 'candidate') return;
    promoteCandidate.mutate(
      {
        job_id: selection.candidate.job_id,
        candidate_id: selection.candidate.candidate_id,
        action: state.draft.action,
        expected_version: currentVersion,
        merge_target_id:
          state.draft.action === 'merge' ? state.draft.mergeTargetId : undefined,
      },
      {
        onSuccess: () => {
          onResolved();
          onDismiss();
        },
        onError: (error) => {
          if (isWorldKbConflictError(error)) {
            setCurrentVersion(error.details.current_version);
          }
        },
      },
    );
  }

  return (
    <WorldKbPromoteConflictModal
      open
      draft={state.draft}
      currentVersion={currentVersion}
      onUseCurrent={onUseCurrent}
      onReapply={handleReapply}
      onDismiss={onDismiss}
    />
  );
}
