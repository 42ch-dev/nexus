import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus FindingDetailResponse
 *
 * Response for GET /v1/local/works/{work_id}/findings/{finding_id} and create/update responses.
 *
 * @schema_version 1
 * @source finding-detail-response.schema.json
 */
/** Response for GET /v1/local/works/{work_id}/findings/{finding_id} and create/update responses. */
export interface FindingDetailResponse {
  finding_id: string;
  work_id: string;
  chapter?: number;
  severity: string;
  status: string;
  title: string;
  description: string;
  target_executor: string;
  kind: string;
  rule_suggestion?: string;
  created_at: number;
  updated_at: number;
  routing_hint?: string;
}
