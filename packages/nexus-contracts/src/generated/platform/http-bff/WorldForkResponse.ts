import type { ForkBranch } from '../../domain/ForkBranch';
import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus WorldForkResponse
 *
 * Response body for POST /v1/worlds/fork — created ForkBranch record.
 *
 * @schema_version 1
 * @source world-fork-response.schema.json
 */
/** Response body for POST /v1/worlds/fork — created ForkBranch record. */
export interface WorldForkResponse {
  schema_version: number;
  fork_branch: ForkBranch;
}
