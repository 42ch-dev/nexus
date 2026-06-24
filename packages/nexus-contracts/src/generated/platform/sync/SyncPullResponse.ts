import type { Bundle } from './Bundle';
import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus SyncPullResponse
 *
 * Response body for POST /v1/sync/pull — bundles to apply locally plus server cursors.
 *
 * @schema_version 1
 * @source sync-pull-response.schema.json
 */
/** Response body for POST /v1/sync/pull — bundles to apply locally plus server cursors. */
export interface SyncPullResponse {
  schema_version: number;
  world_revision: number;
  confirmed_delta_sequence: number;
  is_up_to_date?: boolean;
  bundles: Bundle[];
}
