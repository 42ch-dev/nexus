import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus CreateWorkResponse
 *
 * Response for POST /v1/daemon/works.
 *
 * @schema_version 1
 * @source create-work-response.schema.json
 */
/** Response for POST /v1/daemon/works. */
export interface CreateWorkResponse {
  work_id: string;
  status: string;
}
