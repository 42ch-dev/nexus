import type { KbEntrySummary } from './KbEntrySummary';
import type { PaginationInfo } from './PaginationInfo';
import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ListKbEntriesResponse
 *
 * Response for GET /v1/local/kb/entries.
 *
 * @schema_version 1
 * @source list-kb-entries-response.schema.json
 */
/** Response for GET /v1/local/kb/entries. */
export interface ListKbEntriesResponse {
  items: KbEntrySummary[];
  pagination: PaginationInfo;
}
