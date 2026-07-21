import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus CreateWorldResponse
 *
 * Response body for POST /v1/daemon/worlds (201 Created).
 *
 * @schema_version 1
 * @source create-world-response.schema.json
 */

/** Inline enum type */
export type CreateWorldResponseStatus = 'active' | 'archived';

/** Response body for POST /v1/daemon/worlds (201 Created). */
export interface CreateWorldResponse {
  world_id: string;
  status: CreateWorldResponseStatus;
}
