/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for POST /v1/worlds/snapshot — capture a read-only snapshot cursor with optional branch and size limits (platform API).
 */
export interface WorldSnapshotRequest {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  /**
   * World to snapshot
   */
  world_id: string;
  /**
   * Optional anchor event; when omitted, platform uses latest head for the world
   */
  at_event_id?: string;
  /**
   * Optional ForkBranch scope for the snapshot
   */
  branch_id?: string;
  /**
   * Optional cap on knowledge entries included in snapshot payload shaping
   */
  key_block_limit?: number;
  /**
   * Optional cap on timeline events included in snapshot payload shaping
   */
  timeline_event_limit?: number;
}
