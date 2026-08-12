/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response body for POST /v1/daemon/worlds/:world_id/forks (200 OK). branch_id is required — P2 PD-6 consumes it immediately to set the forked branch context.
 */
export interface CreateForkResponse {
  /**
   * The newly created fork branch id (fbk_ prefix).
   */
  branch_id: string;
  /**
   * The branch the new fork diverges from.
   */
  parent_branch_id: string;
  /**
   * The event on the parent branch that is the fork point.
   */
  forked_from_event_id: string;
  /**
   * ISO 8601 UTC timestamp of fork creation (the fork_created marker's created_at).
   */
  created_at: string;
}
