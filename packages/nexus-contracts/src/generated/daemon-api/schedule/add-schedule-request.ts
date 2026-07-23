/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Concurrency mode for this schedule.
 */
export type NexusScheduleConcurrencyRequest = "serial" | "parallel_with" | "parallel_any";

/**
 * Request body for POST /v1/daemon/orchestration/schedules — create a new schedule.
 */
export interface AddScheduleRequest {
  /**
   * Creator ID that owns this schedule.
   */
  creator_id: string;
  /**
   * Preset ID to run (e.g. novel-writing, game-bible, script-writing).
   */
  preset_id: string;
  /**
   * Optional seed text for the preset's initial context.
   */
  seed?: string;
  /**
   * Human-readable label for this schedule run.
   */
  label?: string;
  /**
   * Schedule IDs this schedule depends on (must complete before this starts).
   */
  depends_on?: string[];
  concurrency?: NexusScheduleConcurrencyRequest;
  /**
   * Unix timestamp string for deferred execution.
   */
  scheduled_at?: string;
  /**
   * Structured input context for the preset (key-value pairs).
   */
  input?: {
    [k: string]: unknown | undefined;
  };
  /**
   * When true, bypass preset gate evaluation. Requires reason.
   */
  force_gates?: boolean;
  /**
   * Audit reason for force_gates (required when force_gates is true).
   */
  reason?: string;
}
