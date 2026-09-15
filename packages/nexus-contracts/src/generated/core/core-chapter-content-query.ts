/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Core-owned chapter content query; identical wire shape to the existing chapter content query parameters (single underlying definition (allOf to the existing schema; keep in sync through the P5-T0 schema lane)). Query parameters stay separate from request bodies.
 */
export type CoreChapterContentQuery = NexusChapterContentQuery;

/**
 * Query parameters for GET/PUT/PATCH chapter detail, outline, and body routes (V1.65 P0).
 */
export interface NexusChapterContentQuery {
  /**
   * Volume number; defaults to 1 for single-volume Works.
   */
  volume?: number;
}
