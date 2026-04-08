import type { DeltaOperation, DeltaType, SchemaVersion, SourceAnchor } from './CommonTypes';
/**
 * Nexus Delta
 *
 * Single atomic change to an entity in a manuscript world. Aligned with data-model-v1.md §5.12.
 *
 * @schema_version 1
 * @source delta.schema.json
 */
/** Single atomic change to an entity in a manuscript world. Aligned with data-model-v1.md §5.12. */
export interface Delta {
  delta_type: DeltaType;
  operation: DeltaOperation;
  target_entity_type?: string;
  target_entity_id?: string;
  payload: Record<string, unknown>;
  source_anchor?: SourceAnchor;
  local_timestamp: string;
}
