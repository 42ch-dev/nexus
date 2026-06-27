import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus StrategyPatchStateRequest
 *
 * Request body for POST /v1/local/strategies/{strategy_id}/states/{state_id}/patch (V1.71). Renames and/or updates the description of a single outer state-machine state.
 *
 * @schema_version 1
 * @source strategy-patch-state-request.schema.json
 */
/** Request body for POST /v1/local/strategies/{strategy_id}/states/{state_id}/patch (V1.71). Renames and/or updates the description of a single outer state-machine state. */
export interface StrategyPatchStateRequest {
  strategy_id: string;
  state_id: string;
  base_revision: number;
  set: { label?: string; description?: string };
}
