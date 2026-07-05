import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus KbEntrySummary
 *
 * Summary row for a KB entry in list responses.
 *
 * @schema_version 1
 * @source kb-entry-summary.schema.json
 */
/** Summary row for a KB entry in list responses. */
export interface KbEntrySummary {
  entry_id: string;
  title: string;
  created_at: string;
}
