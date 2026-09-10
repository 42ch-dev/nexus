/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Summary of an active orchestration engine session.
 */
export interface SessionSummary {
  session_id: string;
  creator_id: string;
  preset_id: string;
  status: string;
  current_task_id?: string;
  /**
   * Stable reason for terminal/uncertain outcomes (e.g. unconfirmed cancel cleanup).
   */
  failure_reason?: string;
  execution?: NexusExecutionProjection;
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
