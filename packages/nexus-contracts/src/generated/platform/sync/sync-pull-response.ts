/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response body for POST /v1/sync/pull — bundles to apply locally plus server cursors.
 */
export interface SyncPullResponse {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  /**
   * Server world revision after the returned window
   */
  world_revision: number;
  /**
   * Server confirmed delta sequence after the returned window
   */
  confirmed_delta_sequence: number;
  /**
   * True when there are no further bundles to fetch for this cursor
   */
  is_up_to_date?: boolean;
  /**
   * Bundles the client should apply (e.g. stage into local outbox)
   */
  bundles: NexusDeltaBundleEnvelope[];
}
/**
 * DeltaBundle envelope containing delta operations for world synchronization. Aligned with bundle-envelope-schema-v1.md §5.
 */
export interface NexusDeltaBundleEnvelope {
  /**
   * Envelope schema version
   */
  schema_version: 1;
  /**
   * Unique bundle instance ID
   */
  bundle_id: string;
  /**
   * Attributing SyncCommand ID
   */
  command_id: string;
  /**
   * Local workspace binding
   */
  workspace_id: string;
  /**
   * Target world
   */
  world_id: string;
  /**
   * Initiating creator
   */
  creator_id: string;
  /**
   * Actual submitting creator (may equal creator_id in single-creator scenarios)
   */
  submitting_creator_id: string;
  /**
   * world_sync | memory_sync | publish_metadata
   */
  bundle_type: "world_sync" | "memory_sync" | "publish_metadata";
  /**
   * Optional but recommended: manuscript phase for downstream gate validation
   */
  manuscript_phase?: "brainstorm" | "draft" | "review" | "finalize" | "published";
  /**
   * Whether this execution requires manuscript output
   */
  output_manuscript?: boolean;
  /**
   * Client-generated idempotency key
   */
  idempotency_key: string;
  /**
   * Bundle content digest per v1-spec ADR-006: SHA-256 over JSON bytes of `deltas` only (Rust ref: serde_json::to_vec). Wire: `sha256:` + 64 lowercase hex. OSS companion: .mstar/specs/canonical-hash.md; golden vector in nexus-sync tests.
   */
  canonical_hash: string;
  /**
   * Optimistic concurrency baseline. At least world_revision or timeline_head_id should be provided.
   */
  base_versions: {
    /**
     * World revision at client-side baseline
     */
    world_revision?: number | null;
    /**
     * Timeline head event ID at baseline
     */
    timeline_head_id?: string;
    /**
     * Optional canon revision
     */
    canon_revision?: number | null;
    [k: string]: unknown | undefined;
  };
  /**
   * Last confirmed delta sequence for conflict detection
   */
  last_confirmed_delta_sequence?: number;
  /**
   * Ordered list of delta operations
   *
   * @minItems 1
   */
  deltas: [NexusDelta, ...NexusDelta[]];
  /**
   * Server-side write-back: bundle-level apply result
   */
  bundle_apply_status?: "all_success" | "partial" | "failed";
  /**
   * Server-side per-delta results
   */
  delta_results?: {
    /**
     * Index into deltas[]
     */
    delta_index: number;
    /**
     * Per-delta apply result
     */
    delta_apply_status: "applied" | "rejected" | "skipped_dependency";
    /**
     * Error code if rejected
     */
    error_code?: string;
    /**
     * Entity revision after successful apply
     */
    applied_entity_revision?: number | null;
    [k: string]: unknown | undefined;
  }[];
  /**
   * Bundle creation timestamp (RFC 3339 UTC)
   */
  created_at: string;
}
/**
 * Single atomic change to an entity in a manuscript world. Aligned with data-model-v1.md §5.12.
 */
export interface NexusDelta {
  /**
   * Target aggregate type for this delta
   */
  delta_type: "world" | "key_block" | "timeline_event" | "fork_branch" | "memory_item" | "story_manifest";
  /**
   * Operation to apply
   */
  operation: "create" | "update" | "upsert" | "delete" | "append";
  /**
   * Sub-type (e.g., 'character' when delta_type='key_block')
   */
  target_entity_type?: string;
  /**
   * Target entity ID (null for create)
   */
  target_entity_id?: string;
  /**
   * Delta payload (validated by per-type sub-schema)
   */
  payload: {
    [k: string]: unknown | undefined;
  };
  source_anchor?: NexusSourceAnchor;
  /**
   * Local timestamp of this delta (RFC 3339 UTC)
   */
  local_timestamp: string;
}
/**
 * Optional source anchor for provenance
 */
export interface NexusSourceAnchor {
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
