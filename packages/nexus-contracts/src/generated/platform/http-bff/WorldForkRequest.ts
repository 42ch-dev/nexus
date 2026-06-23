import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus WorldForkRequest
 *
 * Request body for POST /v1/worlds/fork — platform may derive parent world from URL, child world server-side, and creator from auth; body carries fork point and optional title.
 *
 * @schema_version 1
 * @source world-fork-request.schema.json
 */
/** Request body for POST /v1/worlds/fork — platform may derive parent world from URL, child world server-side, and creator from auth; body carries fork point and optional title. */
export interface WorldForkRequest {
  schema_version: number;
  parent_world_id?: string;
  child_world_id?: string;
  forked_from_event_id?: string;
  created_by_creator_id?: string;
  fork_title?: string;
}
