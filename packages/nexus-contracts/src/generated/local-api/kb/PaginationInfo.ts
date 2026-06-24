import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus PaginationInfo
 *
 * Cursor-based pagination metadata.
 *
 * @schema_version 1
 * @source pagination-info.schema.json
 */
/** Cursor-based pagination metadata. */
export interface PaginationInfo {
  limit: number;
  next_cursor?: string;
}
