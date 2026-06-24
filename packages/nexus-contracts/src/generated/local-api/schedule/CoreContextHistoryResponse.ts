import type { CoreContextHistoryEntry } from './CoreContextHistoryEntry';
import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus CoreContextHistoryResponse
 *
 * Response for GET /v1/local/orchestration/schedules/{schedule_id}/core-context-history.
 *
 * @schema_version 1
 * @source core-context-history-response.schema.json
 */
/** Response for GET /v1/local/orchestration/schedules/{schedule_id}/core-context-history. */
export interface CoreContextHistoryResponse {
  entries: CoreContextHistoryEntry[];
}
