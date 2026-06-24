import type { SessionSummary } from './SessionSummary';
import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus OrchestrationSessionDetailResponse
 *
 * Response for GET /v1/local/orchestration/sessions/{session_id} — full session detail with status.
 *
 * @schema_version 1
 * @source session-detail-response.schema.json
 */
/** Response for GET /v1/local/orchestration/sessions/{session_id} — full session detail with status. */
export interface SessionDetailResponse {
  session: SessionSummary;
}
