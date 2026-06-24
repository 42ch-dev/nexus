import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus AppendInspirationRequest
 *
 * Request body for POST /v1/local/works/{work_id}/inspiration.
 *
 * @schema_version 1
 * @source append-inspiration-request.schema.json
 */
/** Request body for POST /v1/local/works/{work_id}/inspiration. */
export interface AppendInspirationRequest {
  note: string;
}
