import type { FindingDetailResponse } from './FindingDetailResponse';
import type { PaginationInfo } from '../kb/PaginationInfo';
import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ListFindingsResponse
 *
 * Response for GET /v1/local/works/{work_id}/findings (cursor-based pagination, F-P2). New list endpoints use the canonical `items` array key (convention §4); the `pagination` envelope reuses the shared `PaginationInfo`.
 *
 * @schema_version 1
 * @source list-findings-response.schema.json
 */
/** Response for GET /v1/local/works/{work_id}/findings (cursor-based pagination, F-P2). New list endpoints use the canonical `items` array key (convention §4); the `pagination` envelope reuses the shared `PaginationInfo`. */
export interface ListFindingsResponse {
  items: FindingDetailResponse[];
  pagination: PaginationInfo;
}
