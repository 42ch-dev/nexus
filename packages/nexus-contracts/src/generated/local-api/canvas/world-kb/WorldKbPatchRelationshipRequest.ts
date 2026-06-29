import type { WorldKbRelationshipInput } from './WorldKbRelationshipInput';
import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus PatchWorldKbRelationshipRequest
 *
 * Request body for POST /v1/local/worlds/{world_id}/kb/patch-relationship (V1.74). Action-discriminated add/update/remove for typed World KB relationships with per-row OCC on kb_relationships.revision.
 *
 * @schema_version 1
 * @source world-kb-patch-relationship-request.schema.json
 */

/** Inline enum type */
export type WorldKbPatchRelationshipRequestAction = 'add' | 'update' | 'remove';

/** Request body for POST /v1/local/worlds/{world_id}/kb/patch-relationship (V1.74). Action-discriminated add/update/remove for typed World KB relationships with per-row OCC on kb_relationships.revision. */
export interface WorldKbPatchRelationshipRequest {
  relationship_id?: string;
  action: WorldKbPatchRelationshipRequestAction;
  expected_version?: number;
  relationship?: WorldKbRelationshipInput;
}
