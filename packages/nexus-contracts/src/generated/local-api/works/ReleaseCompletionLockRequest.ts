import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ReleaseCompletionLockRequest
 *
 * Request body for POST /v1/local/works/{work_id}/completion-lock/release.
 *
 * @schema_version 1
 * @source release-completion-lock-request.schema.json
 */
/** Request body for POST /v1/local/works/{work_id}/completion-lock/release. */
export interface ReleaseCompletionLockRequest {
  reason: string;
}
