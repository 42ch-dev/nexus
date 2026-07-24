/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * A single ACP agent entry annotated with local PATH-install availability. Returned by POST /v1/daemon/agent-host/scan. Each entry maps to one registry agent (or a custom wizard-supplied launch command) with install status and best-effort version.
 */
export interface AgentScanEntry {
  /**
   * Agent display name from the ACP registry.
   */
  name: string;
  /**
   * Matching ACP registry agent ID (e.g. 'claude-acp'). Null for custom wizard-supplied entries that have no registry match.
   */
  registry_agent_id?: string | null;
  /**
   * Known launch command for this agent. Sourced from the registry's per-platform binary cmd field (e.g. 'claude-acp') or supplied by the user in the wizard's custom path input. Null when neither is available.
   */
  launch_command?: string | null;
  /**
   * True when the binary referenced by launch_command (or derived from registry distribution metadata) is found on the system PATH via a which-equivalent lookup.
   */
  installed: boolean;
  /**
   * Best-effort version string from a `--version` probe of the installed binary. Null when the binary is not installed, or when the version probe fails or times out (≤2s timeout).
   */
  version?: string | null;
  /**
   * Agent description from the ACP registry. Null when no registry entry exists (custom wizard entries).
   */
  description?: string | null;
  /**
   * Agent icon URL from the ACP registry. Null when no registry entry or icon is available.
   */
  icon_url?: string | null;
}
