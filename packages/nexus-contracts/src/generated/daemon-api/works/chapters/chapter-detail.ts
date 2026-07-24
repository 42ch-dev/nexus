/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Lifecycle status of a work chapter (V1.65 P0).
 */
export type NexusChapterStatus = "not_started" | "outlined" | "draft" | "finalized" | "published";

/**
 * Response for GET /v1/daemon/works/{work_id}/chapters/{n} (V1.65 P0). Mirrors ChapterSummary plus content metadata. Does not read outline/body content.
 */
export interface ChapterDetail {
  work_id: string;
  chapter: number;
  volume: number;
  /**
   * Human title if materialized by P0; otherwise clients may derive display text from slug/chapter number. V1.65 returns null.
   */
  title?: string;
  slug?: string;
  planned_word_count: number;
  actual_word_count?: number;
  status: NexusChapterStatus;
  /**
   * Relative path to outline file, or empty string if not initialized.
   */
  outline_path?: string;
  /**
   * Relative path to body file, or empty string if not initialized. Body is read-only.
   */
  body_path?: string;
  created_at: string;
  updated_at: string;
  can_edit_outline: boolean;
  can_edit_structure: boolean;
  body_read_only: boolean;
  protection: NexusChapterProtection;
}
/**
 * Protection level describing what UI actions are allowed for a chapter (V1.65 P0).
 */
export interface NexusChapterProtection {
  /**
   * none = free edit; confirm_structure_edit = UI must show confirmation before structural edits; hard_block_delete = structural edits are blocked.
   */
  level: "none" | "confirm_structure_edit" | "hard_block_delete";
  /**
   * Human-readable explanation for the protection level.
   */
  reason: string;
}
