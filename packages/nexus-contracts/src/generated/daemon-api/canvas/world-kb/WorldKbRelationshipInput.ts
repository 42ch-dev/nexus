import type { WorldKbRelationshipKind } from './WorldKbRelationshipKind';
import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus WorldKbRelationshipInput
 *
 * Author-editable payload for a World KB relationship (V1.74; V1.76 adds optional needs_review for promotion). Supplied inside WorldKbPatchRelationshipRequest for add/update actions.
 *
 * @schema_version 1
 * @source world-kb-relationship-input.schema.json
 */
/** Author-editable payload for a World KB relationship (V1.74; V1.76 adds optional needs_review for promotion). Supplied inside WorldKbPatchRelationshipRequest for add/update actions. */
export interface WorldKbRelationshipInput {
  source_entity_id: string;
  target_entity_id: string;
  relation_type: WorldKbRelationshipKind;
  custom_label?: string;
  symmetric: boolean;
  confidence?: number;
  source_anchor_ids?: string[];
  metadata?: Record<string, unknown>;
  needs_review?: boolean;
}
