/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for POST /v1/daemon/strategies/{strategy_id}/states/{state_id}/prompt/patch (V1.71). Atomically updates a prompt-template file referenced by a state or inner-graph node inside the Strategy bundle.
 */
export interface StrategyPatchPromptTemplateRequest {
  /**
   * Preset / Strategy identifier from the URL path. Must match the path parameter.
   */
  strategy_id: string;
  /**
   * State identifier from the URL path.
   */
  state_id: string;
  /**
   * Revision observed by the client on the last canonical read.
   */
  base_revision: number;
  /**
   * Relative path to the prompt template inside the bundle (e.g. prompts/outline-exit.md).
   */
  template_ref: string;
  /**
   * Prompt-template content update.
   */
  set: {
    /**
     * Complete replacement Markdown content of the prompt template.
     */
    body: string;
  };
}
