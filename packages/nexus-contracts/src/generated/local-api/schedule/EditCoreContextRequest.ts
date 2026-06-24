import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus EditCoreContextRequest
 *
 * Request body for PATCH /v1/local/orchestration/schedules/{schedule_id}/core-context.
 *
 * @schema_version 1
 * @source edit-core-context-request.schema.json
 */
/** Request body for PATCH /v1/local/orchestration/schedules/{schedule_id}/core-context. */
export interface EditCoreContextRequest {
  op: string;
  body?: string;
  patch?: unknown;
  path?: string;
}
