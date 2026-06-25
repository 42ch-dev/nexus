import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus PatchWorkRequest
 *
 * Request body for PATCH /v1/local/works/{work_id}.
 *
 * @schema_version 1
 * @source patch-work-request.schema.json
 */
/** Request body for PATCH /v1/local/works/{work_id}. */
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
