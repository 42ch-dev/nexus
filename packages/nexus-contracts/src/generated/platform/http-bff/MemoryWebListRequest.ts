import type { MemoryKind, MemoryStatus, MemoryType, SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus MemoryWebListRequest
 *
 * Request body for memory web read — list / filter MemoryItem rows for a world (platform plan 18). Aligns with domain memory.schema.json field semantics.
 *
 * @schema_version 1
 * @source memory-web-list-request.schema.json
 */
/** Request body for memory web read — list / filter MemoryItem rows for a world (platform plan 18). Aligns with domain memory.schema.json field semantics. */
export interface MemoryWebListRequest {
  schema_version: number;
  world_id: string;
  cursor?: string;
  limit?: number;
  memory_types?: MemoryType[];
  memory_kinds?: MemoryKind[];
  statuses?: MemoryStatus[];
}
