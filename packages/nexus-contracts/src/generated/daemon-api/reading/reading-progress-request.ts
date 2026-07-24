/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for PUT /v1/daemon/reading/progress. Upserts persisted scroll position per (creator, work, chapter). Creator scope is inferred from the active session.
 */
export interface ReadingProgressRequest {
  work_id: string;
  chapter: number;
  /**
   * Current scroll position in thousandths (0–10000). Must be stored as-is; clients interpret the unit.
   */
  scroll_progress: number;
}
