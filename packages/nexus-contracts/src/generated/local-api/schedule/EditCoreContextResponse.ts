import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus EditCoreContextResponse
 *
 * Response for PATCH /v1/local/orchestration/schedules/{schedule_id}/core-context.
 *
 * @schema_version 1
 * @source edit-core-context-response.schema.json
 */
/** Response for PATCH /v1/local/orchestration/schedules/{schedule_id}/core-context. */
export interface EditCoreContextResponse {
  new_version: number;
}
