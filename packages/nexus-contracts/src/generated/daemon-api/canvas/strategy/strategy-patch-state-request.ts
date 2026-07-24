/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for POST /v1/daemon/strategies/{strategy_id}/states/{state_id}/patch (V1.71). Renames and/or updates the description of a single outer state-machine state.
 */
export interface StrategyPatchStateRequest {
  /**
   * Preset / Strategy identifier from the URL path. Must match the path parameter.
   */
  strategy_id: string;
  /**
   * State identifier from the URL path. Must match the path parameter.
   */
  state_id: string;
  /**
   * Revision observed by the client on the last canonical read.
   */
  base_revision: number;
  /**
   * Fields to update on the state. At least one property must be provided.
   */
  set: {
    /**
     * New human-facing state id/label. Renaming a state also rewrites all references (next targets, initial, terminal).
     */
    label?: string;
    /**
     * New state description.
     */
    description?: string;
  };
}
