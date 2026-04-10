import type { SchemaVersion } from './CommonTypes';
/**
 * Nexus WorldSnapshotRequest
 *
 * Request body for POST /v1/worlds/snapshot — capture a read-only snapshot cursor for a world (platform client contract).
 *
 * @schema_version 1
 * @source world-snapshot-request.schema.json
 */
/** Request body for POST /v1/worlds/snapshot — capture a read-only snapshot cursor for a world (platform client contract). */
export interface WorldSnapshotRequest {
  schema_version: number;
  world_id: string;
  at_event_id?: string;
}
