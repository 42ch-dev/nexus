/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for POST /v1/daemon/characters/{character_id}/soul/reflect. Reads or regenerates the cached whole-Character SOUL narrative. `binding_id` scopes stats/cache to one active binding-local World life; omitted uses the shared Character scope. Synthesis is explicit/on-demand: it runs only when `force_regenerate` is true and the insufficient-data gate passes.
 */
export interface CharacterSoulNarrativeRequest {
  /**
   * ActorWorldBinding ID (lowercase prefix awb_ and exactly 32 hex characters)
   */
  binding_id?: string;
  force_regenerate: boolean;
}
