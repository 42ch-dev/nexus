/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for POST /v1/daemon/actor-knowledge/entries. Stored-owner admission only; clients never send owner_creator_id.
 */
export interface AddKnowledgeEntryRequest {
  owner_kind: "world" | "character" | "actor_world_binding";
  /**
   * World ID (prefix: 'wld_')
   */
  world_id?: string;
  /**
   * Character ID (lowercase prefix chr_ and exactly 32 hex characters)
   */
  character_id?: string;
  /**
   * ActorWorldBinding ID (lowercase prefix awb_ and exactly 32 hex characters)
   */
  binding_id?: string;
  /**
   * World-owned only. Rejected for Character and binding owners.
   */
  creator_only?: boolean;
  /**
   * KnowledgeEntry content type (data-model-v1.md §5.5). V1.54 P1: added game-bible variants (species, faction, magic_system, technology, deity, level, economy_tier). V1.55 P3: added script variants (dialogue, beat, act). V1.123 P1: added era (cross-profile world-shape marker for Brief layer).
   */
  block_type:
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
  canonical_name: string;
}
