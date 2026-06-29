/**
 * World KB canvas orchestrator types (V1.74 A10 split).
 *
 * Shared selection + conflict state types used by the thin orchestrator and
 * the extracted header / inspector / conflict-host modules.
 */
import type {
  WorldKbCandidateProjection,
  WorldKbEntityProjection,
  WorldKbRelationshipProjection,
} from '@42ch/nexus-contracts';

import type { EntityEditForm } from './entity-inspector';
import type { RelationshipForm } from './relationship-inspector';
import type {
  WorldKbEntityConflictDraft,
  WorldKbPromoteConflictDraft,
} from './world-kb-conflict-modal';
import type { WorldKbNodeData } from './types';

/** Editable entity fields tracked for conflict reapply. */
export type EntityField = 'title' | 'body' | 'aliases' | 'block_type';

/** Current canvas selection — entity, candidate, relationship, new relationship, or nothing. */
export type Selection =
  | { kind: 'entity'; node: WorldKbNodeData; entity: WorldKbEntityProjection }
  | { kind: 'candidate'; node: WorldKbNodeData; candidate: WorldKbCandidateProjection }
  | { kind: 'relationship'; relationship: WorldKbRelationshipProjection }
  | { kind: 'new-relationship'; initialSourceEntityId?: string; initialTargetEntityId?: string }
  | null;

/** Conflict state captured when `patch_entity` returns 409. */
export interface EntityConflictState {
  /** Modal draft (KB-flavored copy) — includes entityName. */
  modalDraft: WorldKbEntityConflictDraft;
  /** Raw form captured at conflict time, used to reapply the user's edit. */
  reapplyForm: EntityEditForm;
  dirtyFields: EntityField[];
  currentVersion: number;
}

/** Conflict state captured when `promote_candidate` returns 409. */
export interface PromoteConflictState {
  draft: WorldKbPromoteConflictDraft;
  currentVersion: number;
}

/** Conflict state captured when `patch_relationship` returns 409. */
export interface RelationshipConflictState {
  relationshipId: string;
  draft: RelationshipForm;
  currentVersion: number;
}
