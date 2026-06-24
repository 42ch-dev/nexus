import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus MemoryWebListResponse
 *
 * Paginated list response for memory web read APIs (platform plan 18). Items are read projections; full MemoryItem sync may use domain bundle types separately.
 *
 * @schema_version 1
 * @source memory-web-list-response.schema.json
 */

/** Inline enum type */
export type MemoryWebListResponseItemsMemoryType = 'canon' | 'working' | 'experience';

/** Inline enum type */
export type MemoryWebListResponseItemsMemoryKind = 'story_summary' | 'research_material' | 'review_note' | 'character_note' | 'world_building' | 'plot_outline' | 'theme_analysis' | 'custom';

/** Inline enum type */
export type MemoryWebListResponseItemsStatus = 'active' | 'superseded' | 'archived';

/** Paginated list response for memory web read APIs (platform plan 18). Items are read projections; full MemoryItem sync may use domain bundle types separately. */
export interface MemoryWebListResponse {
  schema_version: number;
  items: { memory_item_id: string; creator_id: string; world_id: string; memory_type: MemoryWebListResponseItemsMemoryType; memory_kind?: MemoryWebListResponseItemsMemoryKind; status: MemoryWebListResponseItemsStatus; summary?: string; created_at: string; updated_at?: string }[];
  next_cursor?: string;
  has_more: boolean;
}
