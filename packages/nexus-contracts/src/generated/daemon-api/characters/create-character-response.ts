/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response body for POST /v1/daemon/characters (201 Created): the Character plus its initial active binding.
 */
export interface CreateCharacterResponse {
  character: NexusCharacter;
  binding: NexusActorWorldBinding;
}
/**
 * Durable Creator-owned Character bearer. Clients never send owner_creator_id on create; the field is stored/read only.
 */
export interface NexusCharacter {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  /**
   * Character ID (lowercase prefix chr_ and exactly 32 hex characters)
   */
  character_id: string;
  /**
   * Owning Creator ID (lowercase ctr_ + 32 hex). Never accepted from create/bind request bodies.
   */
  owner_creator_id: string;
  /**
   * Character display name. Trimmed non-empty; at most 120 Unicode scalars.
   */
  display_name: string;
  /**
   * Character bearer status. v1.184 product surfaces never archive a Character.
   */
  status: "active" | "archived";
  /**
   * Optional Character-owned image URI (metadata only).
   */
  image_uri?: string;
  /**
   * Character-owned persona metadata object. Not a Canvas asset system.
   */
  persona: {
    [k: string]: unknown | undefined;
  };
  /**
   * ISO 8601 / RFC 3339 UTC datetime string
   */
  created_at: string;
  /**
   * ISO 8601 / RFC 3339 UTC datetime string
   */
  updated_at: string;
}
/**
 * Character to World association. Distinct from WorldMembership (Creator to World). v1 APIs create active rows only.
 */
export interface NexusActorWorldBinding {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  /**
   * ActorWorldBinding ID (lowercase prefix awb_ and exactly 32 hex characters)
   */
  binding_id: string;
  /**
   * Character ID (lowercase prefix chr_ and exactly 32 hex characters)
   */
  character_id: string;
  /**
   * World ID (prefix: 'wld_')
   */
  world_id: string;
  /**
   * ActorWorldBinding status. v1.184 APIs create active rows only; inactive is reserved storage vocabulary.
   */
  status: "active" | "inactive";
  /**
   * Optional WorldSheet KnowledgeEntry id (block_type=character lore). Absent when unbound.
   */
  world_sheet_entry_id?: string;
  /**
   * ISO 8601 / RFC 3339 UTC datetime string
   */
  created_at: string;
  /**
   * ISO 8601 / RFC 3339 UTC datetime string
   */
  updated_at: string;
}
