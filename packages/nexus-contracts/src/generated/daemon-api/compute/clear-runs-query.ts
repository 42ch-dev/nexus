/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Query parameters for DELETE /v1/daemon/compute/runs — World-scoped clear of terminal run history (V1.147 P3 T2).
 */
export interface ClearRunsQuery {
  /**
   * World whose terminal runs are cleared (must be owned by the active creator).
   */
  world_id: string;
  /**
   * Optional terminal-state filter; absent → every terminal run of the World. Running and succeeded (needs-review) rows are never deleted.
   */
  status?: "applied" | "discarded" | "failed";
}
