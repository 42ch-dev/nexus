/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for POST /v1/daemon/agent-host/sessions/{session_id}/operations.
 */
export interface OperationResponse {
  operation_id: string;
  session_id: string;
  status: string;
  capture?: NexusCharacterRunCaptureOutcome;
}
/**
 * Initial capture observation for Character prompts; omitted on legacy/Creator operations.
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
