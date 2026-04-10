import type { SchemaVersion } from './CommonTypes';
/**
 * Nexus PublishHistoryRequest
 *
 * Request body for POST /v1/publish/history — paginated publish history for a manuscript.
 *
 * @schema_version 1
 * @source publish-history-request.schema.json
 */
/** Request body for POST /v1/publish/history — paginated publish history for a manuscript. */
export interface PublishHistoryRequest {
  schema_version: number;
  world_id: string;
  manuscript_id: string;
  cursor?: string;
  limit?: number;
}
