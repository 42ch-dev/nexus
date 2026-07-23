/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for PATCH /v1/daemon/reading/annotations/{annotation_id}. Edits the highlight color and/or optional note. Both fields are optional; at least one must be present. The annotation_id comes from the URL path, not the body.
 */
export interface ReadingAnnotationPatchRequest {
  /**
   * New highlight color.
   */
  color?: "yellow" | "blue" | "green" | "pink";
  /**
   * New free-text note. Pass an empty string to clear an existing note.
   */
  note?: string;
}
