import type { WorldKbEntityProjection } from './WorldKbEntityProjection';
import type { WorldKbExtractJobProjection } from './WorldKbExtractJobProjection';
import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus WorldKbPromoteCandidateResponse
 *
 * Success response for POST /v1/local/worlds/{world_id}/kb/promote-candidate (V1.73). `entity` is the resulting (or null for reject) KeyBlock; `job` is the updated extract-job projection; `version` is the new per-row version.
 *
 * @schema_version 1
 * @source world-kb-promote-candidate-response.schema.json
 */
/** Success response for POST /v1/local/worlds/{world_id}/kb/promote-candidate (V1.73). `entity` is the resulting (or null for reject) KeyBlock; `job` is the updated extract-job projection; `version` is the new per-row version. */
export interface WorldKbPromoteCandidateResponse {
  entity?: WorldKbEntityProjection;
  job: WorldKbExtractJobProjection;
  version: number;
  validation_summary: { errors: string[]; warnings: string[] };
}
