import type { ModuleSummary } from './ModuleSummary';
import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ListModulesResponse
 *
 * Response for GET /v1/daemon/compute/modules.
 *
 * @schema_version 1
 * @source list-modules-response.schema.json
 */
/** Response for GET /v1/daemon/compute/modules. */
export interface ListModulesResponse {
  items: ModuleSummary[];
  has_more: boolean;
}
