/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for POST /v1/daemon/characters/{character_id}/memory/review. Drains one bounded batch (<= 50 rows) of the Character pending-review queue through the shared deterministic review pipeline. `binding_id` restricts the batch to one active binding-local scope; omitted drains the shared Character scope.
 */
export interface ReviewCharacterMemoryRequest {
  /**
   * ActorWorldBinding ID (lowercase prefix awb_ and exactly 32 hex characters)
   */
  binding_id?: string;
}
