/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for POST /v1/daemon/strategies/{strategy_id}/transitions/patch (V1.109). Rewires or creates an outer transition (linear next, conditional branch, or default target) and/or updates its condition label.
 */
export interface StrategyPatchTransitionRequest {
  /**
   * Preset / Strategy identifier from the URL path. Must match the path parameter.
   */
  strategy_id: string;
  /**
   * Revision observed by the client on the last canonical read.
   */
  base_revision: number;
  /**
   * State whose outgoing transition is being modified.
   */
  source_state_id: string;
  /**
   * Current target state identifier of the transition to replace. Required when op is 'update' (default).
   */
  old_target?: string;
  /**
   * New target state identifier. Required when op is 'create', optional when op is 'update'.
   */
  new_target?: string;
  /**
   * For conditional / labeled branches, identifies the branch to update or create.
   */
  condition?: string;
  /**
   * Optional disambiguator for the transition form being edited.
   */
  transition_kind?: "next" | "branch" | "default";
  /**
   * Operation to perform: 'create' adds a new outgoing transition, 'update' (default) rewires an existing transition.
   */
  op?: "create" | "update";
}
