import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus AppendInspirationResponse
 *
 * Response for POST /v1/local/works/{work_id}/inspiration.
 *
 * @schema_version 1
 * @source append-inspiration-response.schema.json
 */
/** Response for POST /v1/local/works/{work_id}/inspiration. */
export interface AppendInspirationResponse {
  work_id: string;
  inspiration_count: number;
}
