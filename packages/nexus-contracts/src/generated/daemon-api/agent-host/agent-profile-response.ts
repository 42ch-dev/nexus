/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for GET/PUT /v1/daemon/agent-host/profile. Always 200; `profile: null` means no profile is stored yet (zero new error-fold rows).
 */
export interface AgentProfileResponse {
  /**
   * The stored daemon-global agent profile, or null when unset.
   */
  profile: NexusAgentHostProfile | null;
}
/**
 * Daemon-global agent profile selected in the browser path (I-000064). Persisted in the daemon home `config.toml` under the `agent_profile` table. Also the PUT request body for PUT /v1/daemon/agent-host/profile.
 */
export interface NexusAgentHostProfile {
  /**
   * Agent name as reported by the agent-host scan (or the `custom` sentinel for a manual launch command). Blank names are rejected with `invalid_input`.
   */
  name: string;
  /**
   * Optional launch-command override for the selected agent.
   */
  launch_command?: string;
}
