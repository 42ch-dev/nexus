import type { BlockType, SchemaVersion } from './CommonTypes';
/**
 * Nexus Entity State (Dynamic Compute Runtime)
 *
 * Per-BlockType dynamic state schemas for computable KeyBlocks (V1.61 KB structured layer, compass Q4/Q5). State is MUTABLE runtime data (e.g. current_hp, status_effects) nested by block_type so the same KeyBlock can serve different module types without field-name collisions (compass Q5: state.character.current_hp). The character shape is fully specified; other block types are permissive placeholders.
 *
 * @schema_version 1
 * @source entity-state.schema.json
 */
/** Per-BlockType dynamic state schemas for computable KeyBlocks (V1.61 KB structured layer, compass Q4/Q5). State is MUTABLE runtime data (e.g. current_hp, status_effects) nested by block_type so the same KeyBlock can serve different module types without field-name collisions (compass Q5: state.character.current_hp). The character shape is fully specified; other block types are permissive placeholders. */
export interface EntityState {
  schema_version: number;
  block_type: BlockType;
  state?: Record<string, unknown>;
}
/** Fully-specified dynamic state for character KeyBlocks. Additional module-declared runtime fields are permitted. */
export interface CharacterState {
  current_hp?: number;
  status_effects?: string[];
  position?: string;
  is_alive?: boolean;
}
