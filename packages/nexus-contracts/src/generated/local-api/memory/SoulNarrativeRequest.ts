import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus SoulNarrativeRequest
 *
 * Request body for POST /v1/local/memory/soul/reflect. The endpoint reads or regenerates the cached whole-Creator SOUL narrative; per-world narratives are out of scope for V1.81.
 *
 * @schema_version 1
 * @source soul-narrative-request.schema.json
 */
/** Request body for POST /v1/local/memory/soul/reflect. The endpoint reads or regenerates the cached whole-Creator SOUL narrative; per-world narratives are out of scope for V1.81. */
export interface SoulNarrativeRequest {
  creator_id: string;
  force_regenerate?: boolean;
}
