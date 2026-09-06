/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Terminal or initial capture observation for a Character Host operation. Required nullable members avoid ambiguous absence on the wire.
 */
export interface CharacterRunCaptureOutcome {
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
