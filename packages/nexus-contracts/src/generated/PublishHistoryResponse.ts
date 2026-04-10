import type { SchemaVersion } from './CommonTypes';
/**
 * Nexus PublishHistoryResponse
 *
 * Response body for POST /v1/publish/history.
 *
 * @schema_version 1
 * @source publish-history-response.schema.json
 */
/** Response body for POST /v1/publish/history. */
export interface PublishHistoryResponse {
  schema_version: number;
  entries: unknown[];
  next_cursor?: string;
  has_more: boolean;
}
