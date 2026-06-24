import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ActiveCreatorResponse
 *
 * Response for GET /v1/local/creators/active.
 *
 * @schema_version 1
 * @source active-creator-response.schema.json
 */
/** Response for GET /v1/local/creators/active. */
export interface ActiveCreatorResponse {
  creator_id: string;
  handle?: string;
  display_name?: string;
}
