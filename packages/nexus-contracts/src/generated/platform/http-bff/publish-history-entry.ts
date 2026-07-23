/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Single publish history row (platform Publish API).
 */
export interface PublishHistoryEntry {
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
