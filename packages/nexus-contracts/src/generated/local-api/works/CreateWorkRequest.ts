import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus CreateWorkRequest
 *
 * Request body for POST /v1/local/works.
 *
 * @schema_version 1
 * @source create-work-request.schema.json
 */
/** Request body for POST /v1/local/works. */
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
}
