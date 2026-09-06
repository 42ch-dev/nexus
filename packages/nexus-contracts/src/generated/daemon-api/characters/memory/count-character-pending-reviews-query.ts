/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Query for GET /v1/daemon/characters/{character_id}/memory/pending-review/count. `binding_id` selects one active binding-local scope; omitted selects the shared Character scope.
 */
export interface CountCharacterPendingReviewsQuery {
  /**
   * ActorWorldBinding ID (lowercase prefix awb_ and exactly 32 hex characters)
   */
  binding_id?: string;
}
