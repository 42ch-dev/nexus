import type { WorldKbRelationshipProjection } from './WorldKbRelationshipProjection';
import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus PatchWorldKbRelationshipResponse
 *
 * Success response for POST /v1/local/worlds/{world_id}/kb/patch-relationship (V1.74). Returns the committed relationship projection (absent on remove), the new per-row version, and validation diagnostics.
 *
 * @schema_version 1
 * @source world-kb-patch-relationship-response.schema.json
 */
/** Success response for POST /v1/local/worlds/{world_id}/kb/patch-relationship (V1.74). Returns the committed relationship projection (absent on remove), the new per-row version, and validation diagnostics. */
export interface WorldKbPatchRelationshipResponse {
  relationship?: WorldKbRelationshipProjection;
  version: number;
  validation_summary: { errors: string[]; warnings: string[] };
}
