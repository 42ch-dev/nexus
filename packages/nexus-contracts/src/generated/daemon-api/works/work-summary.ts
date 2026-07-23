/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Summary row for a work in list responses.
 */
export interface WorkSummary {
  work_id: string;
  title: string;
  status: string;
  intake_status: string;
  primary_preset_id: string;
  updated_at: string;
  completion_locked_at?: string;
}
