import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus ListOrchestrationSessionsQuery
 *
 * Query parameters for GET /v1/local/orchestration/sessions (cursor-based pagination + sort, F-F1).
 *
 * @schema_version 2
 * @source list-sessions-query.schema.json
 */
/** Query parameters for GET /v1/local/orchestration/sessions (cursor-based pagination + sort, F-F1). */
export interface ListSessionsQuery {
  creator_id?: string;
  limit?: number;
  cursor?: string;
  sort?: string;
}
