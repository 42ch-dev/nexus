import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus GetKbEntryResponse
 *
 * Response for GET /v1/local/kb/entries/{entry_id}.
 *
 * @schema_version 1
 * @source get-kb-entry-response.schema.json
 */
/** Response for GET /v1/local/kb/entries/{entry_id}. */
export interface GetKbEntryResponse {
  entry_id: string;
  title: string;
  created_at: string;
  content: string;
}
