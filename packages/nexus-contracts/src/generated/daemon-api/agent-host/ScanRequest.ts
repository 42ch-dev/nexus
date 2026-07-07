import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus AgentScanRequest
 *
 * Request body for POST /v1/daemon/agent-host/scan. Triggers a combined registry-list + PATH-probe operation that returns ACP agent entries annotated with local-install availability. Additive V1.94 endpoint — no breaking change to existing agent-host routes.
 *
 * @schema_version 1
 * @source scan-request.schema.json
 */

/** Inline enum type */
export type ScanRequestFilter = 'installed' | 'all';

/** Request body for POST /v1/daemon/agent-host/scan. Triggers a combined registry-list + PATH-probe operation that returns ACP agent entries annotated with local-install availability. Additive V1.94 endpoint — no breaking change to existing agent-host routes. */
export interface ScanRequest {
  filter?: ScanRequestFilter;
  registry_refresh?: boolean;
}
