import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus SoulNarrativeRequest
 *
 * Request body for POST /v1/local/memory/soul/reflect. Absent/null `world_id` reads or regenerates the Creator-level narrative; a present `world_id` scopes read/regeneration to that world's per-World narrative (ownership verified server-side).
 *
 * @schema_version 1
 * @source soul-narrative-request.schema.json
 */
/** Request body for POST /v1/local/memory/soul/reflect. Absent/null `world_id` reads or regenerates the Creator-level narrative; a present `world_id` scopes read/regeneration to that world's per-World narrative (ownership verified server-side). */
export interface SoulNarrativeRequest {
  creator_id: string;
  world_id?: string;
  force_regenerate?: boolean;
}
