/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Field set for world_kb.patch_entity (V1.73). `title` maps to kb_key_blocks.canonical_name; `body` to body_json; `block_type` re-classifies the entity (entity-scope-model §5.1.1). At least one property must be provided.
 */
export interface WorldKbEntityPatch {
  /**
   * New canonical_name (display title).
   */
  title?: string;
  /**
   * Replacement KnowledgeEntry body JSON (summary/attributes/tags/state/computable).
   */
  body?: {
    [k: string]: unknown | undefined;
  };
  /**
   * Replacement alias list.
   */
  aliases?: string[];
  /**
   * Re-classify the entity. Must be a valid BlockType (entity-scope-model §5.1.1).
   */
  block_type?:
    | "character"
    | "ability"
    | "scene"
    | "organization"
    | "item"
    | "conflict"
    | "info_point"
    | "event"
    | "species"
    | "faction"
    | "magic_system"
    | "technology"
    | "deity"
    | "level"
    | "economy_tier"
    | "dialogue"
    | "beat"
    | "act"
    | "era";
}
