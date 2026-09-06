/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for POST /v1/daemon/agent-host/sessions/{session_id}/operations. Tagged by kind: prompt, set_model, or set_mode.
 */
export type ExecuteOperationRequest = Prompt | SetModel | SetMode;

export interface Prompt {
  kind: "prompt";
  content: string;
}
export interface SetModel {
  kind: "set_model";
  model: string;
}
export interface SetMode {
  kind: "set_mode";
  mode: string;
}
