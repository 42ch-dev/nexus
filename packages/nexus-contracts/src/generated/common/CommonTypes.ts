/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * ISO 8601 / RFC 3339 UTC datetime string
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "Timestamp".
 */
export type Timestamp = string;
/**
 * World ID (prefix: 'wld_')
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "WorldId".
 */
export type WorldId = string;
/**
 * Creator ID (prefix: 'ctr_')
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "CreatorId".
 */
export type CreatorId = string;
/**
 * User ID (prefix: 'usr_')
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "UserId".
 */
export type UserId = string;
/**
 * User account status (data-model-v1.md §5.1)
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "AccountStatus".
 */
export type AccountStatus = "active" | "suspended" | "deleted";
/**
 * User subscription tier (data-model-v1.md §5.1)
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "SubscriptionTier".
 */
export type SubscriptionTier = "free" | "pro" | "studio" | "enterprise";
/**
 * KeyBlock ID (prefix: 'kb_')
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "KeyBlockId".
 */
export type KeyBlockId = string;
/**
 * TimelineEvent ID (prefix: 'evt_')
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "TimelineEventId".
 */
export type TimelineEventId = string;
/**
 * DeltaBundle ID (prefix: 'bdl_')
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "BundleId".
 */
export type BundleId = string;
/**
 * SyncCommand ID (prefix: 'cmd_')
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "CommandId".
 */
export type CommandId = string;
/**
 * Workspace ID (prefix: 'wrk_')
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "WorkspaceId".
 */
export type WorkspaceId = string;
/**
 * Monotonically increasing sequence number for deltas
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "DeltaSequence".
 */
export type DeltaSequence = number;
/**
 * Manuscript lifecycle phase (data-model-v1.md §7, §5.9B)
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "ManuscriptPhase".
 */
export type ManuscriptPhase = "brainstorm" | "draft" | "review" | "finalize" | "published";
/**
 * Manuscript aggregate ID (prefix: 'mss_')
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "ManuscriptId".
 */
export type ManuscriptId = string;
/**
 * StoryManifest ID (prefix: 'stm_')
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "StoryManifestId".
 */
export type StoryManifestId = string;
/**
 * Outcome of a publish-story operation (platform Publish API wire)
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "PublishStoryOutcome".
 */
export type PublishStoryOutcome = "submitted" | "published" | "rejected" | "invalid_state";
/**
 * World timeline evolution policy (data-model-v1.md §5.3)
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "TimePolicy".
 */
export type TimePolicy = "manual" | "owner_driven" | "event_driven";
/**
 * Visibility/access level (data-model-v1.md §5.3)
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "Visibility".
 */
export type Visibility = "private" | "unlisted" | "public";
/**
 * KeyBlock content type (data-model-v1.md §5.5). V1.54 P1: added game-bible variants (species, faction, magic_system, technology, deity, level, economy_tier). V1.55 P3: added script variants (dialogue, beat, act). V1.123 P1: added era (cross-profile world-shape marker for Brief layer).
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "BlockType".
 */
export type BlockType =
  | "character"
  | "ability"
  | "scene"
  | "organization"
  | "item"
  | "conflict"
  | "info_point"
  | "event"
  | "species"
  | "faction"
  | "magic_system"
  | "technology"
  | "deity"
  | "level"
  | "economy_tier"
  | "dialogue"
  | "beat"
  | "act"
  | "era";
/**
 * MemoryItem type (data-model-v1.md §5.8)
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "MemoryType".
 */
export type MemoryType = "canon" | "working" | "experience";
/**
 * DeltaBundle type (data-model-v1.md §5.11)
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "BundleType".
 */
export type BundleType = "world_sync" | "memory_sync" | "publish_metadata";
/**
 * Schema version as integer (e.g., 1)
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "SchemaVersion".
 */
export type SchemaVersion = number;
/**
 * Creator status (data-model-v1.md §5.2)
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "CreatorStatus".
 */
export type CreatorStatus = "active" | "archived" | "locked";
/**
 * How creator was registered (data-model-v1.md §5.2)
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "RegistrationSource".
 */
export type RegistrationSource = "cli" | "web_agent" | "platform";
/**
 * World status (data-model-v1.md §5.3)
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "WorldStatus".
 */
export type WorldStatus = "active" | "paused" | "archived";
/**
 * Membership role (data-model-v1.md §5.4)
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "MembershipRole".
 */
export type MembershipRole = "owner" | "maintainer" | "collaborator" | "official_creator";
/**
 * Membership status (data-model-v1.md §5.4)
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "MembershipStatus".
 */
export type MembershipStatus = "active" | "invited" | "suspended" | "removed";
/**
 * How pairing was established (data-model-v1.md §5.2A)
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "PairingSource".
 */
export type PairingSource = "auto_cli" | "manual_web" | "platform_auto";
/**
 * Pairing status (data-model-v1.md §5.2A)
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "PairingStatus".
 */
export type PairingStatus = "active" | "revoked";
/**
 * KeyBlock status (data-model-v1.md §5.5)
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "KeyBlockStatus".
 */
export type KeyBlockStatus = "provisional" | "confirmed" | "deprecated" | "merged" | "deleted";
/**
 * Timeline event type (data-model-v1.md §5.6)
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "TimelineEventType".
 */
export type TimelineEventType =
  "story_advance" | "state_update" | "fork_marker" | "official_progression" | "publish_marker";
/**
 * Timeline event status (data-model-v1.md §5.6)
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "TimelineEventStatus".
 */
export type TimelineEventStatus = "canon" | "provisional" | "rejected";
/**
 * Fork branch status (data-model-v1.md §5.7)
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "ForkBranchStatus".
 */
export type ForkBranchStatus = "active" | "archived";
/**
 * Fork branch verification status (data-model-v1.md §5.7)
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "VerificationStatus".
 */
export type VerificationStatus = "unverified" | "requested" | "verified" | "rejected";
/**
 * Memory content kind (data-model-v1.md §5.8, ADR-001)
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "MemoryKind".
 */
export type MemoryKind =
  | "story_summary"
  | "research_material"
  | "review_note"
  | "character_note"
  | "world_building"
  | "plot_outline"
  | "theme_analysis"
  | "personality_core"
  | "custom";
/**
 * Memory status (data-model-v1.md §5.8)
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "MemoryStatus".
 */
export type MemoryStatus = "active" | "superseded" | "archived";
/**
 * Story manifest type (data-model-v1.md §5.9)
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "ManifestType".
 */
export type ManifestType = "chapter" | "arc" | "story" | "excerpt";
/**
 * Story manifest status (data-model-v1.md §5.9)
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "StoryManifestStatus".
 */
export type StoryManifestStatus = "summary_ready" | "staged_for_publish" | "published" | "archived";
/**
 * Manuscript storage location (data-model-v1.md §5.9)
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "ManuscriptStorage".
 */
export type ManuscriptStorage = "none" | "local_workspace" | "platform_sandbox";
/**
 * Reference source type (data-model-v1.md §5.9A)
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "ReferenceSourceType".
 */
export type ReferenceSourceType = "file" | "pdf" | "url" | "note";
/**
 * Reference scan status (data-model-v1.md §5.9A)
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "ScanStatus".
 */
export type ScanStatus = "pending" | "scanned" | "failed" | "ignored";
/**
 * Sync command type (data-model-v1.md §5.10)
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "CommandType".
 */
export type CommandType =
  "advance_world" | "inject_future_event" | "extract_kb" | "sync_push" | "sync_pull" | "fork_world" | "publish_story";
/**
 * Command origin (data-model-v1.md §5.10)
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "CommandOrigin".
 */
export type CommandOrigin = "local_user" | "local_agent" | "official_creator" | "system";
/**
 * Command execution status (data-model-v1.md §5.10)
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "CommandStatus".
 */
export type CommandStatus = "pending" | "running" | "completed" | "failed" | "cancelled";
/**
 * Delta target aggregate type (data-model-v1.md §5.12)
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "DeltaType".
 */
export type DeltaType = "world" | "key_block" | "timeline_event" | "fork_branch" | "memory_item" | "story_manifest";
/**
 * Delta operation (data-model-v1.md §5.12)
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "DeltaOperation".
 */
export type DeltaOperation = "create" | "update" | "upsert" | "delete" | "append";
/**
 * Outbox delivery state (data-model-v1.md §5.13)
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "DeliveryState".
 */
export type DeliveryState = "staged" | "ready" | "sent" | "acked" | "conflicted" | "failed";
/**
 * Workspace binding status (data-model-v1.md §5.14)
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "BindingStatus".
 */
export type BindingStatus = "active" | "unlinked" | "stale";
/**
 * Agent profile kind (data-model-v1.md §5.15)
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "ProfileKind".
 */
export type ProfileKind = "local_agent" | "platform_hosted";
/**
 * Agent selection mode (data-model-v1.md §5.15)
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "SelectionMode".
 */
export type SelectionMode = "registry" | "manual_command" | "manual_remote";
/**
 * Agent transport method (data-model-v1.md §5.15)
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "Transport".
 */
export type Transport = "stdio" | "http" | "websocket";
/**
 * Agent profile status (data-model-v1.md §5.15)
 *
 * This interface was referenced by `CommonTypes`'s JSON-Schema
 * via the `definition` "AgentProfileStatus".
 */
export type AgentProfileStatus = "active" | "unavailable" | "deprecated";

/**
 * Common type definitions shared across all Nexus domain schemas. All enums aligned with data-model-v1.md §7.
 */
export interface CommonTypes {
  [k: string]: unknown | undefined;
}

/**
 * Value object for referencing platform Story summary entities without uploading full text. Aligned with data-model-v1.md §6.1.
 */
export interface SourceAnchor {
  /**
   * References to platform Story summary entities
   */
  story_summary_refs?: {
    /**
     * StoryManifest ID
     */
    story_manifest_id: string;
    /**
     * Summary unit ID
     */
    summary_unit_id: string;
    /**
     * Unit kind (e.g., 'chapter_summary')
     */
    unit_kind?: string;
    [k: string]: unknown | undefined;
  }[];
  /**
   * Optional excerpt text
   */
  excerpt?: string;
  /**
   * Optional anchor summary
   */
  summary?: string;
}
