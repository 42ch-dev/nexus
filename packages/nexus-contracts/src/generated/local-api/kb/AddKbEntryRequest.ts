import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus AddKbEntryRequest
 *
 * Request body for POST /v1/local/kb/entries.
 *
 * @schema_version 1
 * @source add-kb-entry-request.schema.json
 */
/** Request body for POST /v1/local/kb/entries. */
export interface AddKbEntryRequest {
  creator_id: string;
  workspace_slug?: string;
  scope?: string;
  title?: string;
  content?: string;
  file_path?: string;
}
