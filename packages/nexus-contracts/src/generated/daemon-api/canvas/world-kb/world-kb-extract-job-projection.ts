/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Extract-job projection returned after a promotion action (V1.73). `version` maps to kb_extract_jobs.version CAS column.
 */
export interface WorldKbExtractJobProjection {
  /**
   * Extract job id.
   */
  job_id: string;
  world_id: string;
  /**
   * Job lifecycle status.
   */
  status: string;
  /**
   * Per-row OCC version (kb_extract_jobs.version).
   */
  version: number;
  /**
   * Remaining candidate key_block_ids attached to the job.
   */
  candidate_ids?: string[];
  updated_at?: string;
}
