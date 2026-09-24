/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for POST /v1/daemon/compute/runs/{run_id}/discard — the run's proposals were dropped with no domain effect (V1.147 P0 direct lane).
 */
export interface DiscardRunResponse {
  /**
   * The discarded run.
   */
  run_id: string;
  /**
   * Always `discarded` on success.
   */
  status: "discarded";
}
