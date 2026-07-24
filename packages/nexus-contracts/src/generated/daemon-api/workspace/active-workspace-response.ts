/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for GET /v1/daemon/workspace.
 */
export interface ActiveWorkspaceResponse {
  creator_id: string;
  workspace_slug: string;
  creative_root?: string;
  operational_dir: string;
}
