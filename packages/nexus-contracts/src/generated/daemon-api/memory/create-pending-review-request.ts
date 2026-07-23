/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for POST /v1/daemon/memory/pending-review. Called by the CLI at session-end capture. Optional fields (`world_id`, `task_kind`, `created_at`) default at runtime (`task_kind` → "unknown", `created_at` → current RFC 3339 timestamp); validation limits (pending_id/session_id/world_id ≤ 128, raw_digest ≤ 64KB, task_kind ≤ 64) stay handler-owned and are intentionally NOT encoded here (no behavior redesign).
 */
export interface CreatePendingReviewRequest {
  pending_id: string;
  session_id: string;
  creator_id: string;
  world_id?: string;
  task_kind?: string;
  raw_digest: string;
  created_at?: string;
}
