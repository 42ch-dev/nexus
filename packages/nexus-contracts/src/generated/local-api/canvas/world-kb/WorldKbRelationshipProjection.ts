import type { WorldKbRelationshipKind } from './WorldKbRelationshipKind';
import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus WorldKbRelationshipProjection
 *
 * Canonical wire projection of a World KB relationship row (V1.74; V1.76 adds needs_review + source). One stored row may yield two projections when symmetric=true: the stored direction and a derived symmetric_reverse direction.
 *
 * @schema_version 1
 * @source world-kb-relationship-projection.schema.json
 */

/** Inline enum type */
export type WorldKbRelationshipProjectionSource = 'manual' | 'extraction';

/** Inline enum type */
export type WorldKbRelationshipProjectionProjectionDirection = 'stored' | 'symmetric_reverse';

/** Canonical wire projection of a World KB relationship row (V1.74; V1.76 adds needs_review + source). One stored row may yield two projections when symmetric=true: the stored direction and a derived symmetric_reverse direction. */
export interface WorldKbRelationshipProjection {
  relationship_id: string;
  world_id: string;
  source_entity_id: string;
  target_entity_id: string;
  relation_type: WorldKbRelationshipKind;
  custom_label?: string;
  symmetric: boolean;
  confidence?: number;
  source_anchor_ids: string[];
  metadata?: Record<string, unknown>;
  needs_review: boolean;
  source: WorldKbRelationshipProjectionSource;
  version: number;
  updated_at: string;
  projection_direction: WorldKbRelationshipProjectionProjectionDirection;
}
