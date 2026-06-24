import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus UpdateFindingRequest
 *
 * Request body for PATCH /v1/local/works/{work_id}/findings/{finding_id}.
 *
 * @schema_version 1
 * @source update-finding-request.schema.json
 */
/** Request body for PATCH /v1/local/works/{work_id}/findings/{finding_id}. */
export interface UpdateFindingRequest {
  severity?: string;
  status?: string;
  title?: string;
  description?: string;
  target_executor?: string;
  kind?: string;
  rule_suggestion?: string;
}
