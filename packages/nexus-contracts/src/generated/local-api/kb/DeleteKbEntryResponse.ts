import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus DeleteKbEntryResponse
 *
 * Response for DELETE /v1/local/kb/entries/{entry_id}.
 *
 * @schema_version 1
 * @source delete-kb-entry-response.schema.json
 */
/** Response for DELETE /v1/local/kb/entries/{entry_id}. */
export interface DeleteKbEntryResponse {
  entry_id: string;
  deleted: boolean;
}
