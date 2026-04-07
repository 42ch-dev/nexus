import type { SchemaVersion, SourceAnchor } from './CommonTypes';
/**
 * Nexus Delta
 *
 * Single atomic change to an entity in a manuscript world. Aligned with data-model-v1.md §5.12.
 *
 * @schema_version 1
 * @source delta.schema.json
 */

/** Inline enum type */
export type DeltaDeltaType = 'world' | 'key_block' | 'timeline_event' | 'fork_branch' | 'memory_item' | 'story_manifest';

/** Inline enum type */
export type DeltaOperation = 'create' | 'update' | 'upsert' | 'delete' | 'append';

/** Single atomic change to an entity in a manuscript world. Aligned with data-model-v1.md §5.12. */
export interface Delta {
  delta_type: DeltaDeltaType;
  operation: DeltaOperation;
  target_entity_type?: string;
  target_entity_id?: string;
  payload: Record<string, unknown>;
  source_anchor?: SourceAnchor;
  local_timestamp: string;
}
