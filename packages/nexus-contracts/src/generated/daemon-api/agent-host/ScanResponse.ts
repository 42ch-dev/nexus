import type { AgentScanEntry } from './AgentScanEntry';
import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus AgentScanResponse
 *
 * Response for POST /v1/daemon/agent-host/scan. Returns the ACP registry agent list annotated with local PATH-install availability. Additive V1.94 endpoint.
 *
 * @schema_version 1
 * @source scan-response.schema.json
 */
/** Response for POST /v1/daemon/agent-host/scan. Returns the ACP registry agent list annotated with local PATH-install availability. Additive V1.94 endpoint. */
export interface ScanResponse {
  agents: AgentScanEntry[];
}
