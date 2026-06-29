import type { WorldKbEntityProjection } from './WorldKbEntityProjection';
import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus WorldKbPatchEntityResponse
 *
 * Success response for POST /v1/local/worlds/{world_id}/kb/patch-entity (V1.73). Returns the updated entity projection, the new per-row version, and validation diagnostics.
 *
 * @schema_version 1
 * @source world-kb-patch-entity-response.schema.json
 */
/** Success response for POST /v1/local/worlds/{world_id}/kb/patch-entity (V1.73). Returns the updated entity projection, the new per-row version, and validation diagnostics. */
export interface WorldKbPatchEntityResponse {
  entity: WorldKbEntityProjection;
  version: number;
  validation_summary: { errors: string[]; warnings: string[] };
}
