/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for DELETE /v1/daemon/compute/runs — the number of terminal run rows removed (V1.147 P3 T2).
 */
export interface ClearRunsResponse {
  /**
   * Number of terminal runs deleted (applied|discarded|failed).
   */
  deleted: number;
}
