/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for POST /v1/daemon/works.
 */
export interface CreateWorkRequest {
  title: string;
  long_term_goal: string;
  initial_idea: string;
  world_id?: string;
  story_ref?: string;
  primary_preset_id?: string;
  client_request_id?: string;
  lineage_from_work_id?: string;
  set_pool_active?: boolean;
  work_profile?: string;
}
