/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for POST /v1/daemon/worlds/:world_id/forks. The daemon resolves the active creator; clients never send ownership. Creates a local timeline fork — a new branch within the owned world diverging from the fork-point event on the stated parent branch.
 */
export interface CreateForkRequest {
  /**
   * The branch the new fork diverges from (the world's root branch or an existing fork branch).
   */
  parent_branch_id: string;
  /**
   * The event on the parent branch that is the fork point (branch head after which the new branch diverges).
   */
  forked_from_event_id: string;
  /**
   * Optional human-readable label for the new fork branch (1-200 chars when present).
   */
  label?: string;
}
