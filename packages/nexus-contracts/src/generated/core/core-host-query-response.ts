/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Closed v1 Actor identity sum: Creator | Character. No unknown kinds; a payload cannot carry both bearer ids.
 */
export type NexusActorRef = CreatorActorRef | CharacterActorRef;

/**
 * Host query response envelope with optional typed branches.
 */
export interface CoreHostQueryResponse {
  health?: {
    running: boolean;
    active_sessions: number;
    active_operations: number;
  };
  catalog?: {
    providers: {
      provider_id: string;
      display_name: string;
      protocol_kind: string;
    }[];
  };
  sessions?: NexusAgentHostSessionListResponse;
  session?: NexusAgentHostSessionResponse;
  operation?: NexusAgentHostOperationResponse;
  scan?: {
    entries: NexusAgentScanEntry[];
  };
}
/**
 * Paginated list for GET /v1/daemon/agent-host/sessions.
 */
export interface NexusAgentHostSessionListResponse {
  items: NexusAgentHostSessionResponse[];
  pagination: NexusPaginationInfo;
}
/**
 * Agent-host session summary. Optional actor_ref and viewpoint are omitted for legacy sessions.
 */
export interface NexusAgentHostSessionResponse {
  session_id: string;
  provider_id: string;
  state: string;
  active_op_id?: string;
  model?: string;
  actor_ref?: NexusActorRef;
  viewpoint?: NexusSessionViewpoint;
}
export interface CreatorActorRef {
  /**
   * Actor kind discriminant for a Creator bearer.
   */
  actor_kind: "creator";
  /**
   * Creator bearer id (`CreatorId`).
   */
  creator_id: string;
}
export interface CharacterActorRef {
  /**
   * Actor kind discriminant for a Character bearer. Unrelated to KnowledgeEntry block_type=character.
   */
  actor_kind: "character";
  /**
   * Character ID: lowercase chr_ prefix and exactly 32 hex characters.
   */
  character_id: string;
}
/**
 * Viewpoint for an Actor-mode agent-host session. Contains World plus optional binding/branch/event. Never carries an Actor id.
 */
export interface NexusSessionViewpoint {
  /**
   * World ID (prefix: 'wld_')
   */
  world_id: string;
  /**
   * Required for Character actor_ref; must be omitted for Creator.
   */
  binding_id?: string;
  /**
   * Optional ForkBranch id participating in session isolation.
   */
  branch_id?: string;
  /**
   * Optional rewind/event anchor participating in session isolation.
   */
  event_id?: string;
}
/**
 * Cursor-based pagination metadata.
 */
export interface NexusPaginationInfo {
  limit: number;
  /**
   * Opaque cursor returned by the previous page. Clients MUST NOT parse it. Non-null only when another page exists.
   */
  next_cursor?: string;
  /**
   * True when the client may request another page (equivalent to `next_cursor` being non-null).
   */
  has_more: boolean;
}
/**
 * Response for POST /v1/daemon/agent-host/sessions/{session_id}/operations.
 */
export interface NexusAgentHostOperationResponse {
  operation_id: string;
  session_id: string;
  status: string;
  capture?: NexusCharacterRunCaptureOutcome;
}
/**
 * Initial capture observation for Character prompts; omitted on legacy/Creator operations.
 */
export interface NexusCharacterRunCaptureOutcome {
  status: "disabled" | "pending" | "captured" | "skipped" | "failed";
  pending_id: string | null;
  code:
    | "run_incomplete"
    | "run_failed"
    | "run_cancelled"
    | "capture_too_large"
    | "capture_empty_output"
    | "capture_scope_changed"
    | "capture_store_failed"
    | null;
}
/**
 * A single ACP agent entry annotated with local PATH-install availability. Returned by POST /v1/daemon/agent-host/scan. Each entry maps to one registry agent (or a custom wizard-supplied launch command) with install status and best-effort version.
 */
export interface NexusAgentScanEntry {
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
