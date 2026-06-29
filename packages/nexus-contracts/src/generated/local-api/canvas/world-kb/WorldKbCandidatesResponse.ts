import type { PaginationInfo } from '../../kb/PaginationInfo';
import type { WorldKbCandidateProjection } from './WorldKbCandidateProjection';
import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus WorldKbCandidatesResponse
 *
 * Read projection for GET /v1/local/worlds/{world_id}/kb/candidates (V1.73). Pending promotion candidates with cursor pagination.
 *
 * @schema_version 1
 * @source world-kb-candidates-response.schema.json
 */
/** Read projection for GET /v1/local/worlds/{world_id}/kb/candidates (V1.73). Pending promotion candidates with cursor pagination. */
export interface WorldKbCandidatesResponse {
  items: WorldKbCandidateProjection[];
  pagination: PaginationInfo;
}
