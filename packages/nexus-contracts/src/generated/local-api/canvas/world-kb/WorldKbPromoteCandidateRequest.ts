import type { WorldKbEntityPatch } from './WorldKbEntityPatch';
import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus WorldKbPromoteCandidateRequest
 *
 * Request body for POST /v1/local/worlds/{world_id}/kb/promote-candidate (V1.73). adopt/reject/merge a pending candidate via the entity-scope-model §5.5.2 promotion state machine. Per-row OCC on kb_extract_jobs.version.
 *
 * @schema_version 1
 * @source world-kb-promote-candidate-request.schema.json
 */

/** Inline enum type */
export type WorldKbPromoteCandidateRequestAction = 'adopt' | 'reject' | 'merge';

/** Request body for POST /v1/local/worlds/{world_id}/kb/promote-candidate (V1.73). adopt/reject/merge a pending candidate via the entity-scope-model §5.5.2 promotion state machine. Per-row OCC on kb_extract_jobs.version. */
export interface WorldKbPromoteCandidateRequest {
  job_id: string;
  candidate_id: string;
  action: WorldKbPromoteCandidateRequestAction;
  expected_version: number;
  merge_target_id?: string;
  patch?: WorldKbEntityPatch;
}
