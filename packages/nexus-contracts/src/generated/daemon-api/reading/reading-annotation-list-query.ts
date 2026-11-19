/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Query parameters for GET /v1/daemon/reading/annotations. Returns all annotations for the current creator on a given (work, volume, chapter). Creator scope is inferred from the active session.
 */
export interface ReadingAnnotationListQuery {
  work_id: string;
  /**
   * Volume number. Omit to use volume 1.
   */
  volume?: number;
  chapter: number;
}
