import type { PaginationInfo } from '../kb/PaginationInfo';
import type { WorkSummary } from './WorkSummary';
import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ListWorksResponse
 *
 * Response for GET /v1/local/works (cursor-based pagination, F-P3). The array field is `items`; the legacy `works` key was removed in `@42ch/nexus-contracts` 0.6.0.
 *
 * @schema_version 2
 * @source list-works-response.schema.json
 */
/** Response for GET /v1/local/works (cursor-based pagination, F-P3). The array field is `items`; the legacy `works` key was removed in `@42ch/nexus-contracts` 0.6.0. */
export interface ListWorksResponse {
  items: WorkSummary[];
  pagination: PaginationInfo;
}
