/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Summary row for a compute run in a paginated list (GET /v1/daemon/compute/runs). Lightweight — excludes full proposals and invocation params.
 */
export interface RunSummary {
  /**
   * Unique run identifier.
   */
  run_id: string;
  /**
   * Run lifecycle status. Product mapping: succeeded = "Needs review", applied = "Applied", discarded = "Discarded", failed = "Failed".
   */
  status: "running" | "succeeded" | "failed" | "applied" | "discarded";
  /**
   * Module that was invoked.
   */
  module_id: string;
  /**
   * Version of the module at invocation time.
   */
  module_version: string;
  /**
   * World the run targeted.
   */
  world_id: string;
  /**
   * ISO 8601 UTC timestamp of run creation.
   */
  created_at: string;
  /**
   * ISO 8601 UTC timestamp of last status change (succeeded/failed/applied/discarded).
   */
  updated_at?: string;
  /**
   * ISO 8601 UTC timestamp when accepted. Present only when status is "applied".
   */
  accepted_at?: string;
}
