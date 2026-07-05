import type { BlockType, SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus WorldKbEntityPatch
 *
 * Field set for world_kb.patch_entity (V1.73). `title` maps to kb_key_blocks.canonical_name; `body` to body_json; `block_type` re-classifies the entity (entity-scope-model §5.1.1). At least one property must be provided.
 *
 * @schema_version 1
 * @source world-kb-entity-patch.schema.json
 */
/** Field set for world_kb.patch_entity (V1.73). `title` maps to kb_key_blocks.canonical_name; `body` to body_json; `block_type` re-classifies the entity (entity-scope-model §5.1.1). At least one property must be provided. */
export interface WorldKbEntityPatch {
  title?: string;
  body?: Record<string, unknown>;
  aliases?: string[];
  block_type?: BlockType;
}
