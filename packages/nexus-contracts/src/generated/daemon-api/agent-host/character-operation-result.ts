/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Authoritative per-operation run and capture outcome for owner-authorized Character Host operations.
 */
export interface CharacterOperationResult {
  operation_id: string;
  session_id: string;
  run_status: "running" | "succeeded" | "incomplete" | "failed" | "cancelled";
  finish_reason: "end_turn" | "max_tokens" | "max_turn_requests" | "refusal" | "cancelled" | null;
  capture: NexusCharacterRunCaptureOutcome;
}
/**
 * Terminal or initial capture observation for a Character Host operation. Required nullable members avoid ambiguous absence on the wire.
 */
export interface NexusCharacterRunCaptureOutcome {
  status: "disabled" | "pending" | "captured" | "skipped" | "failed";
  pending_id: string | null;
  code:
    | "run_incomplete"
    | "run_failed"
    | "run_cancelled"
    | "capture_too_large"
    | "capture_empty_output"
    | "capture_scope_changed"
    | "capture_store_failed"
    | null;
}
