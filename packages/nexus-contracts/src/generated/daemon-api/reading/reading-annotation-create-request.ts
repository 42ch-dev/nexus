/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for POST /v1/daemon/reading/annotations. Creates a persistent highlight anchored by character offsets into the chapter body plain text. Creator scope is inferred from the active session.
 */
export interface ReadingAnnotationCreateRequest {
  work_id: string;
  chapter: number;
  /**
   * Character offset into the current body plain text where the highlight begins (inclusive).
   */
  start_offset: number;
  /**
   * Character offset into the current body plain text where the highlight ends (exclusive). Must be strictly greater than start_offset.
   */
  end_offset: number;
  /**
   * The highlighted body text captured at creation time. Used for drift detection in the UI.
   */
  selected_text: string;
  /**
   * Highlight color. V1.89 enum: {yellow, blue, green, pink}.
   */
  color: "yellow" | "blue" | "green" | "pink";
  /**
   * Optional free-text note attached to the highlight.
   */
  note?: string;
}
