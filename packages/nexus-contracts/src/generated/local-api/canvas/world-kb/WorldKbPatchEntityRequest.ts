import type { WorldKbEntityPatch } from './WorldKbEntityPatch';
import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus PatchWorldKbEntityRequest
 *
 * Request body for POST /v1/local/worlds/{world_id}/kb/patch-entity (V1.73). Edits an entity (KeyBlock) title/body/aliases/block_type with per-row OCC on kb_key_blocks.revision.
 *
 * @schema_version 1
 * @source world-kb-patch-entity-request.schema.json
 */
/** Request body for POST /v1/local/worlds/{world_id}/kb/patch-entity (V1.73). Edits an entity (KeyBlock) title/body/aliases/block_type with per-row OCC on kb_key_blocks.revision. */
export interface WorldKbPatchEntityRequest {
  entity_id: string;
  expected_version: number;
  patch: WorldKbEntityPatch;
  idempotency_key?: string;
}
