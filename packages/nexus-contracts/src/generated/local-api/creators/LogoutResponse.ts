import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus LogoutResponse
 *
 * Response for POST /v1/local/creators/logout.
 *
 * @schema_version 1
 * @source logout-response.schema.json
 */
/** Response for POST /v1/local/creators/logout. */
export interface LogoutResponse {
  creator_id: string;
  cleared: boolean;
}
