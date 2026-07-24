/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for PATCH /v1/daemon/findings/batch. Returns partial-success counts and lists of IDs that could not be updated. Always HTTP 200 unless the request exceeds the cap or a DB error occurs.
 */
export interface BatchUpdateFindingsResponse {
  /**
   * Number of findings successfully updated.
   */
  updated: number;
  /**
   * IDs that do not exist or are not owned by the active creator.
   */
  not_found?: string[];
  /**
   * IDs where the requested status transition is illegal per the findings lifecycle.
   */
  conflict?: string[];
}
