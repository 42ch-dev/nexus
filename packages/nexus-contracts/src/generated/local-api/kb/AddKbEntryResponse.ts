import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus AddKbEntryResponse
 *
 * Response for POST /v1/local/kb/entries.
 *
 * @schema_version 1
 * @source add-kb-entry-response.schema.json
 */
/** Response for POST /v1/local/kb/entries. */
export interface AddKbEntryResponse {
  entry_id: string;
  title: string;
}
