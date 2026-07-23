/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * StoryManifest entity for platform-side chapter/arc manifest and summary. Aligned with data-model-v1.md §5.9.
 */
export interface StoryManifest {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  /**
   * Unique StoryManifest identifier (prefix: 'stm_')
   */
  story_manifest_id: string;
  /**
   * World ID (prefix: 'wld_')
   */
  world_id: string;
  /**
   * Creator ID (prefix: 'ctr_')
   */
  creator_id: string;
  /**
   * Manifest type
   */
  manifest_type: "chapter" | "arc" | "story" | "excerpt";
  /**
   * Manifest status
   */
  status: "summary_ready" | "staged_for_publish" | "published" | "archived";
  /**
   * Story title
   */
  title: string;
  /**
   * Platform-side summary unit ID
   */
  summary_unit_id: string;
  /**
   * Platform-authoritative summary text
   */
  summary_text?: string;
  /**
   * Whether manuscript output is enabled
   */
  output_manuscript?: boolean;
  /**
   * Manuscript storage location
   */
  manuscript_storage?: "none" | "local_workspace" | "platform_sandbox";
  /**
   * Local workspace path (when manuscript_storage=local_workspace)
   */
  local_path?: string;
  /**
   * Platform sandbox path (when manuscript_storage=platform_sandbox)
   */
  sandbox_path?: string | null;
  /**
   * Content hash (sha256:xxx)
   */
  content_hash?: string | null;
  /**
   * Published artifact reference
   */
  published_artifact_id?: string | null;
  /**
   * ISO 8601 / RFC 3339 UTC datetime string
   */
  created_at: string;
  /**
   * ISO 8601 / RFC 3339 UTC datetime string
   */
  updated_at?: string;
}
