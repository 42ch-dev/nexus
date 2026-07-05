import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus LogoutResponse
 *
 * Response for POST /v1/daemon/creators/logout.
 *
 * @schema_version 1
 * @source logout-response.schema.json
 */
/** Response for POST /v1/daemon/creators/logout. */
export interface LogoutResponse {
  creator_id: string;
  cleared: boolean;
}
