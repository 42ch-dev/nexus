/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for GET /v1/daemon/compute/runs — cursor-paginated list of compute runs for the active creator (V1.147 P0 direct lane).
 */
export interface RunListResponse {
  /**
   * Array of RunSummary rows, ordered by created_at descending.
   */
  items: NexusRunSummary[];
  /**
   * True when another page exists (equivalent to next_cursor being non-null).
   */
  has_more: boolean;
  /**
   * Opaque cursor for the next page. Clients MUST NOT parse it. Non-null only when has_more is true.
   */
  next_cursor?: string;
}
/**
 * Summary row for a compute run in a paginated list (GET /v1/daemon/compute/runs). Lightweight — excludes full proposals and invocation params.
 */
export interface NexusRunSummary {
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
