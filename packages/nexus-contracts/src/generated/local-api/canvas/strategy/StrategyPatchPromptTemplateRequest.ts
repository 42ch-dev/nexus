import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus StrategyPatchPromptTemplateRequest
 *
 * Request body for POST /v1/local/strategies/{strategy_id}/states/{state_id}/prompt/patch (V1.71). Atomically updates a prompt-template file referenced by a state or inner-graph node inside the Strategy bundle.
 *
 * @schema_version 1
 * @source strategy-patch-prompt-template-request.schema.json
 */
/** Request body for POST /v1/local/strategies/{strategy_id}/states/{state_id}/prompt/patch (V1.71). Atomically updates a prompt-template file referenced by a state or inner-graph node inside the Strategy bundle. */
export interface StrategyPatchPromptTemplateRequest {
  strategy_id: string;
  state_id: string;
  base_revision: number;
  template_ref: string;
  set: { body: string };
}
