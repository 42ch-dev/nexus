import type { BlockType, SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus WorldKbEntityProjection
 *
 * Flat wire projection of a World KB KeyBlock entity for canvas graph + inspector surfaces (V1.73). `version` maps to the SQLite per-row OCC column (kb_key_blocks.revision, NULL-normalized to 0).
 *
 * @schema_version 1
 * @source world-kb-entity-projection.schema.json
 */
/** Flat wire projection of a World KB KeyBlock entity for canvas graph + inspector surfaces (V1.73). `version` maps to the SQLite per-row OCC column (kb_key_blocks.revision, NULL-normalized to 0). */
export interface WorldKbEntityProjection {
  key_block_id: string;
  world_id: string;
  block_type: BlockType;
  canonical_name: string;
  status: string;
  version: number;
  body?: Record<string, unknown>;
  aliases?: string[];
  source_anchor_count?: number;
  updated_at?: string;
}
