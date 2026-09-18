/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for POST /v1/daemon/actor-knowledge/entries. Stored-owner admission only; clients never send owner_creator_id. `audience` is the only governance input: an omitted audience creates an in-scope shared entry. The legacy `creator_only` key is not a member of this schema and its presence is rejected (including `false`), never ignored.
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
  /**
   * Optional authored summary for Character/binding owners; writes body.summary atomically. Rejected on World-owned create.
   */
  summary?: string;
  /**
   * Closed author audience for holder governance. Omitted means in-scope shared (no holder, no disclosure); explicit "shared" is the same. A private audience resolves against the admitted identity: the author never supplies a holder id or a management flag.
   */
  audience?: SharedKnowledgeAudience | AuthorOnlyKnowledgeAudience | CharacterPrivateKnowledgeAudience;
}
export interface SharedKnowledgeAudience {
  /**
   * In-scope shared knowledge: holder and disclosure stay unspecified.
   */
  kind: "shared";
}
export interface AuthorOnlyKnowledgeAudience {
  /**
   * Resolves to the admitted controlling Creator's own holder with disclosure owner-private.
   */
  kind: "author-only";
}
export interface CharacterPrivateKnowledgeAudience {
  /**
   * Resolves to a Character this Creator owns; for a World-owned row that Character must be bound to that World.
   */
  kind: "character-private";
  /**
   * Character ID (lowercase prefix chr_ and exactly 32 hex characters)
   */
  character_id: string;
}
