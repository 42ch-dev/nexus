import type { CapabilityInfo } from './CapabilityInfo';
import type { PaginationInfo } from '../../kb/PaginationInfo';
import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus ListCapabilitiesResponse
 *
 * Response for GET /v1/local/orchestration/capabilities (cursor-based pagination, F-P3). The array field is `items`; the legacy `capabilities` key was removed in `@42ch/nexus-contracts` 0.6.0.
 *
 * @schema_version 2
 * @source list-capabilities-response.schema.json
 */
/** Response for GET /v1/local/orchestration/capabilities (cursor-based pagination, F-P3). The array field is `items`; the legacy `capabilities` key was removed in `@42ch/nexus-contracts` 0.6.0. */
export interface ListCapabilitiesResponse {
  items: CapabilityInfo[];
  pagination: PaginationInfo;
}
