import type { SessionSummary } from './SessionSummary';
import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus ListOrchestrationSessionsResponse
 *
 * Response for GET /v1/local/orchestration/sessions.
 *
 * @schema_version 1
 * @source list-sessions-response.schema.json
 */
/** Response for GET /v1/local/orchestration/sessions. */
export interface ListSessionsResponse {
  sessions: SessionSummary[];
}
