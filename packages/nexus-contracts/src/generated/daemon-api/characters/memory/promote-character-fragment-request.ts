/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for POST /v1/daemon/characters/{character_id}/memory/fragments/{fragment_id}:promote. Explicit, revision-checked promotion of one binding-local fragment to shared Character memory. The fragment id is preserved, `binding_id` provenance is cleared atomically, and only the affected narrative cache scopes are invalidated. Stale revisions fail with a stable 409 version_mismatch; already-shared fragments fail 409 character_fragment_already_shared. No implicit promotion exists.
 */
export interface PromoteCharacterFragmentRequest {
  /**
   * The fragment revision the caller observed; promotion applies only when it still matches.
   */
  expected_revision: number;
}
