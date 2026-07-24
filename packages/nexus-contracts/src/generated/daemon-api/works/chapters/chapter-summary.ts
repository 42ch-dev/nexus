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
 * Summary row for a work chapter in list responses (V1.65 P0). Lightweight — does not read outline/body files.
 */
export interface ChapterSummary {
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
}
