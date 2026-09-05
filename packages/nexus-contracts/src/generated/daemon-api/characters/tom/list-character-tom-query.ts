/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Query string for GET /v1/daemon/characters/{character_id}/tom. Reads only carriers owned by the viewer Character or its selected active binding; world-owned and other Characters' carriers are never read.
 */
export interface ListCharacterTomQuery {
  /**
   * Selected owned active World.
   */
  world_id: string;
  /**
   * The viewer Character's selected active binding in world_id.
   */
  binding_id: string;
  /**
   * Page size bound (1..=100, default 50).
   */
  limit?: number;
  /**
   * Opaque keyset cursor from a previous page. Clients MUST NOT parse it.
   */
  cursor?: string;
}
