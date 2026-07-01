import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus MemoryFragmentInfo
 *
 * A single memory-fragment row in the list-fragments response. V1.79 exposes keyword and creation-time metadata for read-only SOUL visualization; write-only/internal fragment fields (session_id, creator_id, ttl) remain out of this response.
 *
 * @schema_version 1
 * @source memory-fragment-info.schema.json
 */
/** A single memory-fragment row in the list-fragments response. V1.79 exposes keyword and creation-time metadata for read-only SOUL visualization; write-only/internal fragment fields (session_id, creator_id, ttl) remain out of this response. */
export interface MemoryFragmentInfo {
  fragment_id: string;
  summary: string;
  world_id?: string | null;
  keywords?: string[];
  created_at?: string;
}
