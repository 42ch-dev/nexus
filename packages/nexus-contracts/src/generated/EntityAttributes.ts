import type { BlockType, SchemaVersion } from './CommonTypes';
/**
 * Nexus Entity Attributes (Static Compute Params)
 *
 * Per-BlockType static attribute schemas for computable KeyBlocks (V1.61 KB structured layer, compass Q4). Attributes are IMMUTABLE compute parameters (e.g. max_hp, base_atk) stored inside a KeyBlock body. The character shape is fully specified; other block types are permissive placeholders (additionalProperties: true) to be tightened as compute modules are added. Note: 'environment' is not a Nexus BlockType; the combat-relevant computable block types are used here instead.
 *
 * @schema_version 1
 * @source entity-attributes.schema.json
 */
/** Per-BlockType static attribute schemas for computable KeyBlocks (V1.61 KB structured layer, compass Q4). Attributes are IMMUTABLE compute parameters (e.g. max_hp, base_atk) stored inside a KeyBlock body. The character shape is fully specified; other block types are permissive placeholders (additionalProperties: true) to be tightened as compute modules are added. Note: 'environment' is not a Nexus BlockType; the combat-relevant computable block types are used here instead. */
export interface EntityAttributes {
  schema_version: number;
  block_type: BlockType;
  attributes?: Record<string, unknown>;
}
/** Fully-specified static attributes for character KeyBlocks. Additional module-declared stats are permitted. */
export interface CharacterAttributes {
  max_hp?: number;
  base_atk?: number;
  base_def?: number;
  speed?: number;
  level?: number;
}
