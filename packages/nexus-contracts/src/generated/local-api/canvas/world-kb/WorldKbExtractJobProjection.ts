import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus WorldKbExtractJobProjection
 *
 * Extract-job projection returned after a promotion action (V1.73). `version` maps to kb_extract_jobs.version CAS column.
 *
 * @schema_version 1
 * @source world-kb-extract-job-projection.schema.json
 */
/** Extract-job projection returned after a promotion action (V1.73). `version` maps to kb_extract_jobs.version CAS column. */
export interface WorldKbExtractJobProjection {
  job_id: string;
  world_id: string;
  status: string;
  version: number;
  candidate_ids?: string[];
  updated_at?: string;
}
