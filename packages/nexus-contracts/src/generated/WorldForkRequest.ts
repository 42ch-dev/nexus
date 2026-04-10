import type { SchemaVersion } from './CommonTypes';
/**
 * Nexus WorldForkRequest
 *
 * Request body for POST /v1/worlds/fork — create a forked world from a parent at a timeline event (platform client contract).
 *
 * @schema_version 1
 * @source world-fork-request.schema.json
 */
/** Request body for POST /v1/worlds/fork — create a forked world from a parent at a timeline event (platform client contract). */
export interface WorldForkRequest {
  schema_version: number;
  parent_world_id: string;
  child_world_id: string;
  forked_from_event_id: string;
  created_by_creator_id: string;
}
