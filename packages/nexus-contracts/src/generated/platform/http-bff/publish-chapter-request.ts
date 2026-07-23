/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for POST /v1/publish/chapters — publish a single chapter artifact (platform Publish API).
 */
export interface PublishChapterRequest {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  /**
   * World ID (prefix: 'wld_')
   */
  world_id: string;
  /**
   * StoryManifest ID (prefix: 'stm_')
   */
  story_manifest_id: string;
  /**
   * Client-supplied idempotency token for safe retries
   */
  idempotency_key: string;
  /**
   * Optional display title
   */
  title?: string;
  /**
   * Optional chapter summary
   */
  summary?: string;
  /**
   * Optional link to originating sync command
   */
  sync_command_id?: string;
}
