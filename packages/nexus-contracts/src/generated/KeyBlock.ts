import type { BlockType, SchemaVersion, SourceAnchor } from './CommonTypes';

/**
 * Nexus KeyBlock
 *
 * KeyBlock - a structured knowledge unit in a world timeline. Aligned with data-model-v1.md §5.5.
 *
 * @schema_version 1
 * @source key-block.schema.json
 */

/** Inline enum type */
export type KeyBlockStatus = 'provisional' | 'confirmed' | 'deprecated' | 'merged' | 'deleted';

/** KeyBlock - a structured knowledge unit in a world timeline. Aligned with data-model-v1.md §5.5. */
export interface KeyBlock {
  schema_version: number;
  key_block_id: string;
  world_id: string;
  block_type: BlockType;
  canonical_name: string;
  status: KeyBlockStatus;
  revision?: number;
  body?: { summary?: string; attributes?: Record<string, unknown>; tags?: string[] };
  source_anchor?: SourceAnchor;
  created_from_command_id?: string;
  created_at: string;
  updated_at?: string;
}
