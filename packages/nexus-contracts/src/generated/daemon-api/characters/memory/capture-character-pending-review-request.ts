/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for POST /v1/daemon/characters/{character_id}/memory/pending-review. Captures one session-end digest into the Character review queue. `binding_id` marks binding-local provenance and must be an active binding of the path Character in an owned active World; omitted means shared Character scope. The owner Creator is always resolved from the active-Creator config — request bodies never carry `owner_creator_id`.
 */
export interface CaptureCharacterPendingReviewRequest {
  pending_id: string;
  session_id: string;
  /**
   * ActorWorldBinding ID (lowercase prefix awb_ and exactly 32 hex characters)
   */
  binding_id?: string;
  /**
   * Classification hint (brainstorm, outline, chapter, research); defaults to `unknown`.
   */
  task_kind?: string;
  raw_digest: string;
  /**
   * Client-supplied capture timestamp; defaults to server time.
   */
  created_at?: string;
}
