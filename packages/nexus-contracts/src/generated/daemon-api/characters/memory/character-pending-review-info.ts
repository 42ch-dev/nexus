/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * One Character pending-review queue row. `binding_id` is the binding-local provenance (one World life); absent means shared Character scope. Rows are only ever read or written under the owning active Character; the server resolves the owner from stored state.
 */
export interface CharacterPendingReviewInfo {
  pending_id: string;
  session_id: string;
  /**
   * Character ID (lowercase prefix chr_ and exactly 32 hex characters)
   */
  character_id: string;
  /**
   * Binding-local provenance; omitted for shared Character scope.
   */
  binding_id?: string;
  task_kind: string;
  raw_digest: string;
  /**
   * ISO 8601 / RFC 3339 UTC datetime string
   */
  created_at: string;
}
