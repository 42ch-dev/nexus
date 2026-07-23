/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for POST /v1/publish/story — platform Publish API (display fields, idempotency, chapter selection).
 */
export interface PublishStoryRequest {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  /**
   * World that owns the publish operation
   */
  world_id: string;
  /**
   * Optional manuscript aggregate; platform may derive context without it
   */
  manuscript_id?: string;
  /**
   * Optional specific manifest; when omitted the platform may use the active manifest
   */
  story_manifest_id?: string;
  /**
   * Display title for the published story
   */
  title: string;
  /**
   * Optional longer description or synopsis
   */
  summary?: string;
  /**
   * Ordered chapter artifact identifiers to include in this publish
   *
   * @minItems 1
   */
  chapter_ids: [string, ...string[]];
  /**
   * Client-supplied idempotency token for safe retries
   */
  idempotency_key: string;
  /**
   * Optional link to originating sync command
   */
  sync_command_id?: string;
}
