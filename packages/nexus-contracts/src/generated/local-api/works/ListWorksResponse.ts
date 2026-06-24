import type { PaginationInfo } from '../kb/PaginationInfo';
import type { WorkSummary } from './WorkSummary';
import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ListWorksResponse
 *
 * Response for GET /v1/local/works (cursor-based pagination, F-P1). The legacy `total` field is removed; array field name `works` is retained (the `works` -> `items` rename is deferred to F-P3).
 *
 * @schema_version 1
 * @source list-works-response.schema.json
 */
/** Response for GET /v1/local/works (cursor-based pagination, F-P1). The legacy `total` field is removed; array field name `works` is retained (the `works` -> `items` rename is deferred to F-P3). */
export interface ListWorksResponse {
  works: WorkSummary[];
  pagination: PaginationInfo;
}
