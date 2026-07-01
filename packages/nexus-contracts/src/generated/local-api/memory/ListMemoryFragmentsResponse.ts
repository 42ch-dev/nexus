import type { MemoryFragmentInfo } from './MemoryFragmentInfo';
import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ListMemoryFragmentsResponse
 *
 * Response body for GET /v1/local/memory/fragments. Fragments are produced only by the `review` route (no CRUD on this surface). Unlike the pending-review list, this response is NOT paginated (returns up to `limit` rows).
 *
 * @schema_version 1
 * @source list-memory-fragments-response.schema.json
 */
/** Response body for GET /v1/local/memory/fragments. Fragments are produced only by the `review` route (no CRUD on this surface). Unlike the pending-review list, this response is NOT paginated (returns up to `limit` rows). */
export interface ListMemoryFragmentsResponse {
  fragments: MemoryFragmentInfo[];
}
