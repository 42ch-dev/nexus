/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Workflow run admission request. Creator/workspace identity is minted from the authenticated Principal and selected workspace, never carried in the payload; the frozen input map holds preset variables.
 */
export interface CoreWorkflowRunRequest {
  /**
   * Preset identifier to execute.
   */
  preset_id: string;
  /**
   * Optional owning Work id (schedule/chain origin).
   */
  work_id?: string;
  /**
   * Frozen preset input map.
   */
  input?: {
    [k: string]: unknown | undefined;
  };
}
