/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * The agent-host provider document at `<home>/.nexus42/agent-host/config.toml`. GET /v1/daemon/agent-host/config returns it (native defaults when the file is absent); PUT replaces it with the same document. Provider entries are kept when disabled so a known-good provider can be re-enabled.
 */
export interface AgentHostConfig {
  /**
   * Maximum concurrent provider sessions.
   */
  max_sessions?: number;
  /**
   * Maximum concurrent operations per session.
   */
  max_ops_per_session?: number;
  /**
   * Provider lifecycle timeouts in milliseconds.
   */
  timeouts?: {
    launch_ms?: number;
    initialize_ms?: number;
    session_ms?: number;
    prompt_ms?: number;
    shutdown_ms?: number;
  };
  /**
   * Admission policy for providers, tools and model fallback.
   */
  policy?: {
    unknown_provider?: "deny" | "allow";
    unknown_tool_risk?: "deny" | "ask" | "allow";
    allow_model_fallback?: boolean;
  };
  /**
   * Configured providers. An enabled entry requires a supported protocol and a non-empty command (the launcher's own admission rule).
   */
  providers: {
    /**
     * Provider id (the catalog's `provider_id`).
     */
    id: string;
    /**
     * Provider protocol kind.
     */
    protocol: "acp" | "native_cli";
    /**
     * Launch command for an enabled provider.
     */
    command?: string | null;
    args?: string[];
    env?: {
      [k: string]: string | undefined;
    };
    enabled?: boolean;
  }[];
}
