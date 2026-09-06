/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for PATCH /v1/daemon/characters/:character_id/bindings/:binding_id. Only world_sheet_entry_id is mutable; null unlinks, omission keeps.
 */
export interface UpdateCharacterBindingRequest {
  expected_revision: number;
  /**
   * Optional WorldSheet KnowledgeEntry id (kb_, at most 128 bytes). Null unlinks; omission keeps the current link.
   */
  world_sheet_entry_id?: string | null;
}
