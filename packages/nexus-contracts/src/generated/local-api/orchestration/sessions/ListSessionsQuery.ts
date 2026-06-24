import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus ListOrchestrationSessionsQuery
 *
 * Query parameters for GET /v1/local/orchestration/sessions.
 *
 * @schema_version 1
 * @source list-sessions-query.schema.json
 */
/** Query parameters for GET /v1/local/orchestration/sessions. */
export interface ListSessionsQuery {
  creator_id?: string;
}
