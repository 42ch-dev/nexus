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
}
