/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for POST /v1/worlds/fork — platform may derive parent world from URL, child world server-side, and creator from auth; body carries fork point and optional title.
 */
export interface WorldForkRequest {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  /**
   * Source world when not implied by route
   */
  parent_world_id?: string;
  /**
   * Desired child world id when client supplies it; otherwise server-generated
   */
  child_world_id?: string;
  /**
   * Timeline event that defines the fork point
   */
  forked_from_event_id?: string;
  /**
   * Creator initiating the fork when not injected from auth
   */
  created_by_creator_id?: string;
  /**
   * Optional human-readable fork label
   */
  fork_title?: string;
}
