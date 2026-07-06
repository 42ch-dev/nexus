import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus AgentScanEntry
 *
 * A single ACP agent entry annotated with local PATH-install availability. Returned by POST /v1/daemon/agent-host/scan. Each entry maps to one registry agent (or a custom wizard-supplied launch command) with install status and best-effort version.
 *
 * @schema_version 1
 * @source agent-scan-entry.schema.json
 */
/** A single ACP agent entry annotated with local PATH-install availability. Returned by POST /v1/daemon/agent-host/scan. Each entry maps to one registry agent (or a custom wizard-supplied launch command) with install status and best-effort version. */
export interface AgentScanEntry {
  name: string;
  registry_agent_id?: string | null;
  launch_command?: string | null;
  installed: boolean;
  version?: string | null;
  description?: string | null;
  icon_url?: string | null;
}
