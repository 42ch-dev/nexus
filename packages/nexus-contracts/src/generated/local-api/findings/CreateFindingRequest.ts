import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus CreateFindingRequest
 *
 * Request body for POST /v1/local/works/{work_id}/findings.
 *
 * @schema_version 1
 * @source create-finding-request.schema.json
 */
/** Request body for POST /v1/local/works/{work_id}/findings. */
export interface CreateFindingRequest {
  chapter?: number;
  severity: string;
  title: string;
  description?: string;
  target_executor?: string;
  kind?: string;
  rule_suggestion?: string;
}
