import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus CoreContextHistoryEntry
 *
 * Single entry in core context version history.
 *
 * @schema_version 1
 * @source core-context-history-entry.schema.json
 */
/** Single entry in core context version history. */
export interface CoreContextHistoryEntry {
  version: number;
  payload_kind: string;
  content?: unknown;
  derivation_kind: string;
  created_at: string;
}
