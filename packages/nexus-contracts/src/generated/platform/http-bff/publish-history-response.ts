/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response body for POST /v1/publish/history.
 */
export interface PublishHistoryResponse {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  entries: NexusPublishHistoryEntry[];
  /**
   * Opaque cursor for the next page; omit when not available
   */
  next_cursor?: string;
  /**
   * True when additional pages may exist
   */
  has_more: boolean;
}
/**
 * Single publish history row (platform Publish API).
 */
export interface NexusPublishHistoryEntry {
  /**
   * When the publish attempt was recorded
   */
  occurred_at: string;
  /**
   * Outcome of a publish-story operation (platform Publish API wire)
   */
  outcome: "submitted" | "published" | "rejected" | "invalid_state";
  /**
   * StoryManifest ID (prefix: 'stm_')
   */
  story_manifest_id?: string;
  published_artifact_id?: string;
  message?: string;
}
