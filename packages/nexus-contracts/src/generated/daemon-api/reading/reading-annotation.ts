/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Shared annotation detail object returned by POST, PATCH, and as list items in GET /v1/daemon/reading/annotations. Represents a single persistent highlight with optional note, anchored by character offsets into the chapter body plain text.
 */
export interface ReadingAnnotation {
  /**
   * Stable, server-generated annotation identifier (e.g. ULID).
   */
  annotation_id: string;
  work_id: string;
  chapter: number;
  /**
   * Character offset where the highlight begins (inclusive).
   */
  start_offset: number;
  /**
   * Character offset where the highlight ends (exclusive). Guaranteed > start_offset.
   */
  end_offset: number;
  /**
   * The highlighted body text captured at creation time.
   */
  selected_text: string;
  /**
   * Highlight color. V1.89 enum: {yellow, blue, green, pink}.
   */
  color: "yellow" | "blue" | "green" | "pink";
  /**
   * Optional free-text note. Absent or null when no note has been set.
   */
  note?: string;
  /**
   * ISO 8601 creation timestamp.
   */
  created_at: string;
  /**
   * ISO 8601 last-update timestamp.
   */
  updated_at: string;
}
