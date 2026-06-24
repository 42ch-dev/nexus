import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ListKbEntriesQuery
 *
 * Query parameters for GET /v1/local/kb/entries.
 *
 * @schema_version 1
 * @source list-kb-entries-query.schema.json
 */
/** Query parameters for GET /v1/local/kb/entries. */
export interface ListKbEntriesQuery {
  creator_id?: string;
  workspace_slug?: string;
  scope?: string;
  q?: string;
  limit?: number;
  cursor?: string;
}
