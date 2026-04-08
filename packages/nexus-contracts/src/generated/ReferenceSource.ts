import type { ReferenceSourceType, ScanStatus, SchemaVersion } from './CommonTypes';
/**
 * Nexus ReferenceSource
 *
 * ReferenceSource - local-only registration of research/reference sources. Does NOT sync to platform; shared excerpts go through MemoryItem(memory_kind=research_material). Aligned with data-model-v1.md §5.9A.
 *
 * @schema_version 1
 * @source reference-source.schema.json
 */
/** ReferenceSource - local-only registration of research/reference sources. Does NOT sync to platform; shared excerpts go through MemoryItem(memory_kind=research_material). Aligned with data-model-v1.md §5.9A. */
export interface ReferenceSource {
  schema_version: number;
  reference_source_id: string;
  workspace_id: string;
  source_type: ReferenceSourceType;
  uri: string;
  title: string;
  tags?: string[];
  content_hash?: string;
  scan_status: ScanStatus;
  created_at: string;
  updated_at?: string;
}
