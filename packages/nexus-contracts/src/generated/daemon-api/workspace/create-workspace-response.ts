/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for POST /v1/daemon/workspaces.
 */
export interface CreateWorkspaceResponse {
  creator_id: string;
  workspace_slug: string;
  creative_root: string;
  operational_dir: string;
  state_db_path: string;
}
