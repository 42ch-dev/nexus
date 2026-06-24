import type { CapabilityInfo } from './CapabilityInfo';
import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus ListCapabilitiesResponse
 *
 * Response for GET /v1/local/orchestration/capabilities.
 *
 * @schema_version 1
 * @source list-capabilities-response.schema.json
 */
/** Response for GET /v1/local/orchestration/capabilities. */
export interface ListCapabilitiesResponse {
  capabilities: CapabilityInfo[];
}
