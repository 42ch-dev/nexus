/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Query parameters for GET/PUT/PATCH chapter detail, outline, and body routes (V1.65 P0).
 */
export interface ChapterContentQuery {
  /**
   * Volume number; defaults to 1 for single-volume Works.
   */
  volume?: number;
}
