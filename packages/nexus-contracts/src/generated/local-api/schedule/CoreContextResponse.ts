import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus CoreContextResponse
 *
 * Response for GET /v1/local/orchestration/schedules/{schedule_id}/core-context.
 *
 * @schema_version 1
 * @source core-context-response.schema.json
 */
/** Response for GET /v1/local/orchestration/schedules/{schedule_id}/core-context. */
export interface CoreContextResponse {
  version: number;
  payload_kind: string;
  content: unknown;
  derivation_kind: string;
  created_at: string;
}
