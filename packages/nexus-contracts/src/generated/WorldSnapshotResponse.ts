import type { SchemaVersion } from './CommonTypes';
/**
 * Nexus WorldSnapshotResponse
 *
 * Response body for POST /v1/worlds/snapshot — snapshot anchor and revision metadata.
 *
 * @schema_version 1
 * @source world-snapshot-response.schema.json
 */
/** Response body for POST /v1/worlds/snapshot — snapshot anchor and revision metadata. */
export interface WorldSnapshotResponse {
  schema_version: number;
  world_id: string;
  world_revision: number;
  at_event_id?: string;
  captured_at: string;
}
