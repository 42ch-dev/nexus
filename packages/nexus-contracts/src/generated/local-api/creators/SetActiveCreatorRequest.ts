import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus SetActiveCreatorRequest
 *
 * Request body for POST /v1/local/creators/active.
 *
 * @schema_version 1
 * @source set-active-creator-request.schema.json
 */
/** Request body for POST /v1/local/creators/active. */
export interface SetActiveCreatorRequest {
  creator_id: string;
}
