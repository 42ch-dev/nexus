import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus BatchUpdateFindingsResponse
 *
 * Response for PATCH /v1/daemon/works/{work_id}/findings/batch. Returns partial-success counts and lists of IDs that could not be updated. Always HTTP 200 unless the request exceeds the cap or a DB error occurs.
 *
 * @schema_version 1
 * @source batch-update-findings-response.schema.json
 */
/** Response for PATCH /v1/daemon/works/{work_id}/findings/batch. Returns partial-success counts and lists of IDs that could not be updated. Always HTTP 200 unless the request exceeds the cap or a DB error occurs. */
export interface BatchUpdateFindingsResponse {
  updated: number;
  not_found?: string[];
  conflict?: string[];
}
