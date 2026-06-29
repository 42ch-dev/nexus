/**
 * World KB relationship edge projection (V1.74 A6).
 *
 * Pure functions that convert the canonical `WorldKbRelationshipProjection[]`
 * into React Flow edges plus human-readable labels. Kept separate from the
 * entity/anchor projection so each module stays under the 250-line split cap.
 */
import type { Edge } from '@xyflow/react';
import type {
  WorldKbRelationshipKind,
  WorldKbRelationshipProjection,
} from '@42ch/nexus-contracts';

import type { WorldKbEdgeData } from './types';

/** Human-readable label for each core relationship kind (Title Case). */
export const RELATIONSHIP_KIND_LABELS: Record<WorldKbRelationshipKind, string> = {
  allied_with: 'Allied With',
  opposes: 'Opposes',
  parent_of: 'Parent Of',
  child_of: 'Child Of',
  member_of: 'Member Of',
  located_in: 'Located In',
  rules_over: 'Rules Over',
  references: 'References',
  serves: 'Serves',
  rival_of: 'Rival Of',
  mentor_of: 'Mentor Of',
  custom: 'Custom',
};

/** Human-readable edge label: core kind title-cased, or custom label verbatim. */
export function relationshipEdgeLabel(rel: WorldKbRelationshipProjection): string {
  if (rel.relation_type === 'custom' && rel.custom_label) return rel.custom_label;
  return RELATIONSHIP_KIND_LABELS[rel.relation_type] ?? rel.relation_type;
}

/**
 * Derive relationship edges from the graph projection.
 *
 * Both the stored direction and the derived symmetric_reverse direction are
 * rendered; the edge label shows the relation type + optional custom label,
 * and the data payload carries confidence + grounding anchor ids for badges.
 *
 * Note: the backend (`project_relationships_for_world`) already swaps
 * source/target when emitting the `symmetric_reverse` projection, so this
 * function consumes `source_entity_id`/`target_entity_id` verbatim. Swapping
 * here too would double-swap and render both edges in the same direction.
 */
export function deriveRelationshipEdges(
  relationships: WorldKbRelationshipProjection[],
): Edge[] {
  return relationships.map((rel) => {
    const data: WorldKbEdgeData = {
      relationType: 'relationship',
      sourceAnchorIds: rel.source_anchor_ids ?? [],
      confidence: rel.confidence,
      promotionState: undefined,
    };
    const label = relationshipEdgeLabel(rel);
    const strokeColor = rel.relation_type === 'custom'
      ? 'var(--color-canvas-worldkb-relationship-edge-custom)'
      : rel.symmetric
        ? 'var(--color-canvas-worldkb-relationship-edge-symmetric)'
        : 'var(--color-canvas-worldkb-relationship-edge-default)';
    const style = { stroke: strokeColor };
    return {
      id: `relationship:${rel.relationship_id}:${rel.projection_direction}`,
      source: `entity:${rel.source_entity_id}`,
      target: `entity:${rel.target_entity_id}`,
      type: 'default',
      label,
      data,
      selectable: true,
      focusable: true,
      style,
    } satisfies Edge;
  });
}
