/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Field set for world_kb.patch_entity (V1.73). `title` maps to kb_key_blocks.canonical_name; `body` to body_json; `block_type` re-classifies the entity (entity-scope-model §5.1.1); `modules` merges per-entry functional-dialect modules into kb_key_blocks.modules_json (V1.165 P2, AR-4/PD-12: first-level key upsert; omit or {} preserves existing keys; unknown keys round-trip verbatim). At least one property must be provided.
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
  /**
   * Per-entry functional-dialect modules (modules.mental, modules.belief, modules.observation, etc.) merged into the entry's stored modules_json. Mirrors the spoke ModuleMap shape: keys are functional-dialect ids matching ^[a-z][a-z0-9_-]*$, values are objects or arrays. First-level key upsert (AR-4/PD-12): provided keys replace the whole first-level value; unspecified sibling keys are preserved; `{}` is a no-op; omitted inherits the stored modules.
   */
  modules?: {
    [k: string]:
      | (
          | {
              [k: string]: unknown | undefined;
            }
          | unknown[]
        )
      | undefined;
  };
}
