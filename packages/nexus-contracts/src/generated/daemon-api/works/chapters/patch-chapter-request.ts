/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Status transition. Only `not_started -> outlined` is allowed automatically; other transitions may require explicit confirmation in future iterations.
 */
export type NexusChapterStatus = "not_started" | "outlined" | "draft" | "finalized" | "published";

/**
 * Request body for PATCH /v1/daemon/works/{work_id}/chapters/{n} (V1.65 P0). All fields optional. `title` is rejected because it is display-only until P0 materializes a title column.
 */
export interface PatchChapterRequest {
  /**
   * Rejected in V1.65 — display-only until a title column is materialized.
   */
  title?: string;
  /**
   * URL-safe chapter slug. Changing a published chapter slug is blocked.
   */
  slug?: string;
  /**
   * Target word count for the chapter.
   */
  planned_word_count?: number;
  /**
   * Volume the chapter belongs to. Must preserve (work_id, volume, chapter) uniqueness.
   */
  volume?: number;
  status?: NexusChapterStatus;
  /**
   * Must be true when mutating structure of a finalized chapter.
   */
  confirm_structural_edit?: boolean;
  /**
   * Required for explicit reverse/terminal transitions when implemented.
   */
  transition_reason?: string;
}
