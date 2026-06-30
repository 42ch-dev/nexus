import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus MemoryFragmentInfo
 *
 * A single memory-fragment row in the list-fragments response. The Local API intentionally exposes only `fragment_id` and `summary`; internal fragment fields (session_id, creator_id, keywords, created_at, ttl) are not part of this response.
 *
 * @schema_version 1
 * @source memory-fragment-info.schema.json
 */
/** A single memory-fragment row in the list-fragments response. The Local API intentionally exposes only `fragment_id` and `summary`; internal fragment fields (session_id, creator_id, keywords, created_at, ttl) are not part of this response. */
export interface MemoryFragmentInfo {
  fragment_id: string;
  summary: string;
}
