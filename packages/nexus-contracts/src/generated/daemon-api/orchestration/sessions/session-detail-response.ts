/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for GET /v1/daemon/orchestration/sessions/{session_id} — full session detail with status.
 */
export interface SessionDetailResponse {
  session: NexusOrchestrationSessionSummary;
}
/**
 * Summary of an active orchestration engine session.
 */
export interface NexusOrchestrationSessionSummary {
  session_id: string;
  creator_id: string;
  preset_id: string;
  status: string;
  current_task_id?: string;
}
