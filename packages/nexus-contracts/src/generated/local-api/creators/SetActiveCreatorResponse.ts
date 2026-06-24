import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus SetActiveCreatorResponse
 *
 * Response for POST /v1/local/creators/active.
 *
 * @schema_version 1
 * @source set-active-creator-response.schema.json
 */
/** Response for POST /v1/local/creators/active. */
export interface SetActiveCreatorResponse {
  creator_id: string;
}
