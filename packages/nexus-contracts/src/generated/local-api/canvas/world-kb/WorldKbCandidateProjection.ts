import type { BlockType, SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus WorldKbCandidateProjection
 *
 * Pending promotion candidate projection for the World KB promotion inspector (V1.73). Backed by kb_extract_jobs + the pending KeyBlock row.
 *
 * @schema_version 1
 * @source world-kb-candidate-projection.schema.json
 */
/** Pending promotion candidate projection for the World KB promotion inspector (V1.73). Backed by kb_extract_jobs + the pending KeyBlock row. */
export interface WorldKbCandidateProjection {
  candidate_id: string;
  job_id: string;
  world_id: string;
  block_type: BlockType;
  canonical_name: string;
  status?: string;
  version: number;
  source_anchor_count?: number;
  created_at?: string;
}
