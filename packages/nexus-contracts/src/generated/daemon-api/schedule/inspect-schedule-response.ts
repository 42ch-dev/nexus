/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for GET /v1/daemon/orchestration/schedules/{schedule_id}.
 */
export interface InspectScheduleResponse {
  schedule: NexusScheduleSummary;
  /**
   * Schedule IDs this schedule depends on.
   */
  depends_on: string[];
  /**
   * Concurrency mode description string.
   */
  concurrency_kind: string;
}
/**
 * The full schedule summary.
 */
export interface NexusScheduleSummary {
  /**
   * Unique schedule identifier.
   */
  schedule_id: string;
  /**
   * Owning creator ID.
   */
  creator_id: string;
  /**
   * Preset ID this schedule runs.
   */
  preset_id: string;
  /**
   * Current schedule status.
   */
  status: string;
  /**
   * Execution policy: `legacy_inert`, `driven_v1`, or `system_inert` (A3).
   */
  execution_policy: string;
  /**
   * The owned run session ID, when the schedule has been admitted (A3).
   */
  current_session_id?: string;
  execution?: NexusExecutionProjection;
  /**
   * Human-readable label.
   */
  label?: string;
  /**
   * Current core context version number.
   */
  current_core_context_version: number;
  /**
   * ISO-8601 creation timestamp.
   */
  created_at: string;
  /**
   * ISO-8601 last-update timestamp.
   */
  updated_at: string;
}
/**
 * Shared recovery/legal-action projection (A2/A7).
 */
export interface NexusExecutionProjection {
  /**
   * Durable execution version; null when no run exists.
   */
  execution_version?: number | null;
  /**
   * Durable state revision; null when no run exists.
   */
  state_revision?: number | null;
  /**
   * Shared A7 recovery classification.
   */
  recovery_class:
    | "terminal"
    | "human_wait"
    | "safe_boundary"
    | "converge_merge"
    | "interrupted"
    | "legacy_inert"
    | "system_inert"
    | "legacy_unverified"
    | "unreadable";
  /**
   * Current durable A4 human wait, if any.
   */
  wait?: {
    wait_id: string;
    task_id: string;
    child_session_id?: string | null;
    child_task_id?: string | null;
    kind?: string;
  } | null;
  /**
   * Stable machine reason code for uncertain/terminal outcomes.
   */
  reason_code?: string | null;
  /**
   * Legal operator actions in the current durable state.
   */
  allowed_actions: ("start" | "continue" | "resume" | "cancel" | "new_run")[];
}
