import type { MemoryType, SchemaVersion } from './CommonTypes';
/**
 * Nexus MemoryItem
 *
 * MemoryItem - structured memory for creator experience and world context. Aligned with data-model-v1.md §5.8.
 *
 * @schema_version 1
 * @source memory.schema.json
 */

/** Inline enum type */
export type MemoryMemoryKind = 'story_summary' | 'research_material' | 'review_note' | 'character_note' | 'world_building' | 'plot_outline' | 'theme_analysis' | 'custom';

/** Inline enum type */
export type MemoryStatus = 'active' | 'superseded' | 'archived';

/** MemoryItem - structured memory for creator experience and world context. Aligned with data-model-v1.md §5.8. */
export interface Memory {
  schema_version: number;
  memory_item_id: string;
  creator_id: string;
  world_id: string;
  memory_type: MemoryType;
  memory_kind?: MemoryMemoryKind;
  status: MemoryStatus;
  summary?: string;
  embedding_ref?: string;
  source_refs?: { kind: string; id: string }[];
  last_accessed_at?: string;
  last_reinforced_at?: string;
  created_at: string;
  updated_at?: string;
}
