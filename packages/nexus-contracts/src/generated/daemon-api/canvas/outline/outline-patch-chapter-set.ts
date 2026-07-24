/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Fields to update on a chapter via the outline canvas patch route (V1.72).
 */
export interface OutlinePatchChapterSet {
  /**
   * Display title for the chapter (UI-only; persisted in the work outline frontmatter).
   */
  title?: string;
  /**
   * Filename slug for the chapter.
   */
  slug?: string;
  /**
   * Planned word count for the chapter.
   */
  planned_word_count?: number;
  /**
   * Actual word count for the chapter, if known.
   */
  actual_word_count?: number;
  /**
   * Volume binding for the chapter.
   */
  volume?: number;
  /**
   * Lifecycle status of the chapter. published is read/protected in V1.72 patch routes unless explicitly authorized.
   */
  status?: "not_started" | "outlined" | "draft" | "finalized" | "published";
  /**
   * Chapter outline prose notes — the rich-text outline content for this chapter (V1.75 canvas-pivot parity-close). Replaces the V1.65 whole-document editor's per-chapter outline content. Persisted to the chapter's outline_path markdown file under the same outline_revision CAS; MUST NOT mutate body_path.
   */
  content?: string;
}
