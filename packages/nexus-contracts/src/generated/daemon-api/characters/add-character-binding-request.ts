/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for POST /v1/daemon/characters/:character_id/bindings. Clients never send owner_creator_id.
 */
export interface AddCharacterBindingRequest {
  /**
   * World ID (prefix: 'wld_')
   */
  world_id: string;
  /**
   * Optional WorldSheet KnowledgeEntry id (at most 128 bytes).
   */
  world_sheet_entry_id?: string;
}
