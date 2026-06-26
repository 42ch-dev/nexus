import type { PaginationInfo } from '../../kb/PaginationInfo';
import type { SessionSummary } from './SessionSummary';
import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus ListOrchestrationSessionsResponse
 *
 * Response for GET /v1/local/orchestration/sessions (cursor-based pagination, F-P3). The array field is `items`; the legacy `sessions` key was removed in `@42ch/nexus-contracts` 0.6.0.
 *
 * @schema_version 2
 * @source list-sessions-response.schema.json
 */
/** Response for GET /v1/local/orchestration/sessions (cursor-based pagination, F-P3). The array field is `items`; the legacy `sessions` key was removed in `@42ch/nexus-contracts` 0.6.0. */
export interface ListSessionsResponse {
  items: SessionSummary[];
  pagination: PaginationInfo;
}
