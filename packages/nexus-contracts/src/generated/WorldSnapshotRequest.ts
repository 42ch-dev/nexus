import type { SchemaVersion } from './CommonTypes';
/**
 * Nexus WorldSnapshotRequest
 *
 * Request body for POST /v1/worlds/snapshot — capture a read-only snapshot cursor with optional branch and size limits (platform API).
 *
 * @schema_version 1
 * @source world-snapshot-request.schema.json
 */
/** Request body for POST /v1/worlds/snapshot — capture a read-only snapshot cursor with optional branch and size limits (platform API). */
export interface WorldSnapshotRequest {
  schema_version: number;
  world_id: string;
  at_event_id?: string;
  branch_id?: string;
  key_block_limit?: number;
  timeline_event_limit?: number;
}
