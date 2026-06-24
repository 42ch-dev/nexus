import type { WorkSummary } from './WorkSummary';
import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ListWorksResponse
 *
 * Response for GET /v1/local/works.
 *
 * @schema_version 1
 * @source list-works-response.schema.json
 */
/** Response for GET /v1/local/works. */
export interface ListWorksResponse {
  works: WorkSummary[];
  total: number;
}
