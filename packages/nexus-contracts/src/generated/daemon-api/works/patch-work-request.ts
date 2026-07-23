/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for PATCH /v1/daemon/works/{work_id}.
 */
export interface PatchWorkRequest {
  title?: string;
  long_term_goal?: string;
  creative_brief?: string;
  intake_status?: string;
  status?: string;
  world_id?: string;
  story_ref?: string;
  primary_preset_id?: string;
  current_stage?: string;
  stage_status?: string;
  force?: boolean;
  auto_review_master_on_timeout?: boolean;
  auto_chain_interrupted?: boolean;
  work_profile?: string;
}
