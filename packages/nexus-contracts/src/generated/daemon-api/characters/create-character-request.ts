/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for POST /v1/daemon/characters. The daemon resolves the active Creator; clients never send owner_creator_id. Requires an owned world_id for the atomic initial binding.
 */
export interface CreateCharacterRequest {
  /**
   * Character display name (1-120 after trim).
   */
  display_name: string;
  /**
   * Owned World for the initial active ActorWorldBinding.
   */
  world_id: string;
  /**
   * Optional Character-owned image URI.
   */
  image_uri?: string;
  /**
   * Optional persona metadata object.
   */
  persona?: {
    [k: string]: unknown | undefined;
  };
  /**
   * Optional WorldSheet link on the initial binding.
   */
  world_sheet_entry_id?: string;
}
