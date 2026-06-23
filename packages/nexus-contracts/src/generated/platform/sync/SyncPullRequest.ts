import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus SyncPullRequest
 *
 * Request body for POST /v1/sync/pull — incremental bundle fetch from the platform (CLI/daemon client contract).
 *
 * @schema_version 1
 * @source sync-pull-request.schema.json
 */
/** Request body for POST /v1/sync/pull — incremental bundle fetch from the platform (CLI/daemon client contract). */
export interface SyncPullRequest {
  schema_version: number;
  world_id: string;
  after_confirmed_delta_sequence?: number;
}
