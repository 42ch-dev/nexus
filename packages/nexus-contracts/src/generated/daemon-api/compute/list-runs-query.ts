/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Query parameters for GET /v1/daemon/compute/runs — filters for the active creator's compute-run history (V1.147 P0 direct lane).
 */
export interface ListRunsQuery {
  /**
   * Restrict to runs targeting this World.
   */
  world_id?: string;
  /**
   * Restrict to runs of this module.
   */
  module_id?: string;
  /**
   * Restrict to one lifecycle status.
   */
  status?: "running" | "succeeded" | "failed" | "applied" | "discarded";
  /**
   * Page size (default 20, max 100).
   */
  limit?: number;
  /**
   * Opaque cursor from a previous page's `next_cursor`. Clients MUST NOT parse it.
   */
  cursor?: string;
}
