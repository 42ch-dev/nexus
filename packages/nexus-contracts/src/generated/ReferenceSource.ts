import type { SchemaVersion } from './CommonTypes';
/**
 * Nexus ReferenceSource
 *
 * ReferenceSource - local-only registration of research/reference sources. Does NOT sync to platform; shared excerpts go through MemoryItem(memory_kind=research_material). Aligned with data-model-v1.md §5.9A.
 *
 * @schema_version 1
 * @source reference-source.schema.json
 */

/** Inline enum type */
export type ReferenceSourceSourceType = 'file' | 'pdf' | 'url' | 'note';

/** Inline enum type */
export type ReferenceSourceScanStatus = 'pending' | 'scanned' | 'failed' | 'ignored';

/** ReferenceSource - local-only registration of research/reference sources. Does NOT sync to platform; shared excerpts go through MemoryItem(memory_kind=research_material). Aligned with data-model-v1.md §5.9A. */
export interface ReferenceSource {
  schema_version: number;
  reference_source_id: string;
  workspace_id: string;
  source_type: ReferenceSourceSourceType;
  uri: string;
  title: string;
  tags?: string[];
  content_hash?: string;
  scan_status: ReferenceSourceScanStatus;
  created_at: string;
  updated_at?: string;
}
