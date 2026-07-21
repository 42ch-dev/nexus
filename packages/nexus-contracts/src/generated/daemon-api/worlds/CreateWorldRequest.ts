import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus CreateWorldRequest
 *
 * Request body for POST /v1/daemon/worlds. The daemon resolves the active creator; clients never send ownership.
 *
 * @schema_version 1
 * @source create-world-request.schema.json
 */
/** Request body for POST /v1/daemon/worlds. The daemon resolves the active creator; clients never send ownership. */
export interface CreateWorldRequest {
  title: string;
}
