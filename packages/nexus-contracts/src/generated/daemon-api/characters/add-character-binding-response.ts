/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for POST /v1/daemon/characters/:character_id/bindings (201 Created).
 */
export interface AddCharacterBindingResponse {
  binding: NexusActorWorldBinding;
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
