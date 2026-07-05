import type { FindingBatchPatch } from './FindingBatchPatch';
import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus BatchUpdateFindingsRequest
 *
 * Request body for PATCH /v1/daemon/findings/batch. Bulk-updates status and/or target_executor for up to 100 findings. Creator-scoped; each individual update reuses the existing update_finding DAO validation.
 *
 * @schema_version 1
 * @source batch-update-findings-request.schema.json
 */
/** Request body for PATCH /v1/daemon/findings/batch. Bulk-updates status and/or target_executor for up to 100 findings. Creator-scoped; each individual update reuses the existing update_finding DAO validation. */
export interface BatchUpdateFindingsRequest {
  finding_ids: string[];
  patch: FindingBatchPatch;
}
