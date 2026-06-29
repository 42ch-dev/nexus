/**
 * World KB relationship inspector logic (V1.74 A6).
 *
 * Pure form construction, validation, and patch-request builders extracted
 * from the React component so both modules stay under the 250-line split cap.
 */
import type {
  WorldKbEntityProjection,
  WorldKbRelationshipKind,
  WorldKbRelationshipProjection,
} from '@42ch/nexus-contracts';

export const CORE_KINDS: WorldKbRelationshipKind[] = [
  'allied_with',
  'opposes',
  'parent_of',
  'child_of',
  'member_of',
  'located_in',
  'rules_over',
  'references',
  'serves',
  'rival_of',
  'mentor_of',
  'custom',
];

export interface RelationshipForm {
  sourceEntityId: string;
  targetEntityId: string;
  relationType: WorldKbRelationshipKind;
  customLabel: string;
  symmetric: boolean;
  confidence: number;
  sourceAnchorIds: string[];
}

export type RelationshipFormErrors = Partial<Record<keyof RelationshipForm, string>>;

export function formFromRelationship(rel: WorldKbRelationshipProjection): RelationshipForm {
  return {
    sourceEntityId: rel.source_entity_id,
    targetEntityId: rel.target_entity_id,
    relationType: rel.relation_type,
    customLabel: rel.custom_label ?? '',
    symmetric: rel.symmetric,
    confidence: rel.confidence ?? 1,
    sourceAnchorIds: rel.source_anchor_ids ?? [],
  };
}

export function entityName(id: string, entities: WorldKbEntityProjection[]): string {
  return entities.find((e) => e.key_block_id === id)?.canonical_name ?? id;
}

export function initialRelationshipForm(
  relationship: WorldKbRelationshipProjection | undefined,
  initialSourceEntityId: string | undefined,
  initialTargetEntityId: string | undefined,
): RelationshipForm {
  if (relationship) return formFromRelationship(relationship);
  return {
    sourceEntityId: initialSourceEntityId ?? '',
    targetEntityId: initialTargetEntityId ?? '',
    relationType: 'references',
    customLabel: '',
    symmetric: false,
    confidence: 1,
    sourceAnchorIds: [],
  };
}

export function validateRelationshipForm(form: RelationshipForm): RelationshipFormErrors {
  const next: RelationshipFormErrors = {};
  if (!form.sourceEntityId) next.sourceEntityId = 'Source entity is required.';
  if (!form.targetEntityId) next.targetEntityId = 'Target entity is required.';
  if (form.sourceEntityId && form.targetEntityId && form.sourceEntityId === form.targetEntityId) {
    next.targetEntityId = 'Source and target must be different entities.';
  }
  if (form.relationType === 'custom' && !form.customLabel.trim()) {
    next.customLabel = 'Custom label is required for the Custom relation type.';
  }
  if (form.confidence < 0 || form.confidence > 1 || Number.isNaN(form.confidence)) {
    next.confidence = 'Confidence must be between 0 and 1.';
  }
  return next;
}

export interface RelationshipPatchRequest {
  relationship_id?: string;
  action: 'add' | 'update' | 'remove';
  expected_version?: number;
  relationship?: {
    source_entity_id: string;
    target_entity_id: string;
    relation_type: WorldKbRelationshipKind;
    custom_label?: string;
    symmetric: boolean;
    confidence?: number;
    source_anchor_ids: string[];
  };
}

export function buildRelationshipPatchRequest(
  form: RelationshipForm,
  relationship: WorldKbRelationshipProjection | undefined,
): RelationshipPatchRequest {
  const payload = {
    source_entity_id: form.sourceEntityId,
    target_entity_id: form.targetEntityId,
    relation_type: form.relationType,
    custom_label: form.relationType === 'custom' ? form.customLabel.trim() : undefined,
    symmetric: form.symmetric,
    confidence: form.confidence,
    source_anchor_ids: form.sourceAnchorIds,
  };
  if (relationship) {
    return {
      relationship_id: relationship.relationship_id,
      action: 'update' as const,
      expected_version: relationship.version,
      relationship: payload,
    };
  }
  return { action: 'add' as const, relationship: payload };
}

export function buildRelationshipRemoveRequest(
  relationship: WorldKbRelationshipProjection,
): RelationshipPatchRequest {
  return {
    relationship_id: relationship.relationship_id,
    action: 'remove',
    expected_version: relationship.version,
  };
}
