import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus StrategyPatchTransitionRequest
 *
 * Request body for POST /v1/local/strategies/{strategy_id}/transitions/patch (V1.71). Rewires an outer transition (linear next, conditional branch, or default target) and/or updates its condition label.
 *
 * @schema_version 1
 * @source strategy-patch-transition-request.schema.json
 */

/** Inline enum type */
export type StrategyPatchTransitionRequestTransitionKind = 'next' | 'branch' | 'default';

/** Request body for POST /v1/local/strategies/{strategy_id}/transitions/patch (V1.71). Rewires an outer transition (linear next, conditional branch, or default target) and/or updates its condition label. */
export interface StrategyPatchTransitionRequest {
  strategy_id: string;
  base_revision: number;
  source_state_id: string;
  old_target: string;
  new_target?: string;
  condition?: string;
  transition_kind?: StrategyPatchTransitionRequestTransitionKind;
}
