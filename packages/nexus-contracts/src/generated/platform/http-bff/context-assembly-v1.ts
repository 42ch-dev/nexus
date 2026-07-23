/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Context Assembly request/response schemas retained for deferred direct platform cloud context assembly and CLI local in-process context assembly flows. In V1.26, only local CLI assembly is shipped: assemble-local uses Stage0/TwoStage in-process assembly, and assemble-moment uses local four-domain Moment assembly. There is no active daemon context-assemble Local API endpoint.
 */
export interface ContextAssemblyV1 {
  [k: string]: unknown | undefined;
}
/**
 * Request shape for deferred direct platform cloud context assembly. CLI may use this shape when platform cloud assembly becomes available; V1.26 shipped context assembly is local-only and does not send this request to a daemon context-assemble Local API endpoint.
 *
 * This interface was referenced by `ContextAssemblyV1`'s JSON-Schema
 * via the `definition` "ContextAssembleRequestV1".
 */
export interface ContextAssembleRequestV1 {
  /**
   * Caller-generated traceable ID for request/response correlation
   */
  request_id: string;
  /**
   * Workspace ID (prefix: 'wrk_')
   */
  workspace_id: string;
  /**
   * Creator ID (prefix: 'ctr_')
   */
  creator_id: string;
  /**
   * World ID (prefix: 'wld_')
   */
  world_id: string;
  /**
   * Include memory items in assembled context
   */
  include_memory?: boolean;
  /**
   * Include timeline events in assembled context
   */
  include_timeline?: boolean;
  /**
   * Include story summaries in assembled context
   */
  include_story_summaries?: boolean;
  /**
   * Branch ID for temporal filtering
   */
  branch_id?: string | null;
  /**
   * Natural language query for vector memory search
   */
  memory_query?: string | null;
  /**
   * Max timeline events to return
   */
  timeline_limit?: number;
  /**
   * Max key blocks to return
   */
  key_block_limit?: number;
  /**
   * Filter memory items by kind
   */
  memory_kinds?: (
    | "story_summary"
    | "research_material"
    | "review_note"
    | "character_note"
    | "world_building"
    | "plot_outline"
    | "theme_analysis"
    | "location_reference"
    | "timeline_note"
    | "dialogue_snippet"
    | "symbol_motif"
    | "custom"
  )[];
  /**
   * Maximum number of recent timeline events (null = platform default)
   */
  max_timeline_events?: number | null;
  /**
   * Maximum number of story summaries (null = platform default)
   */
  max_story_summaries?: number | null;
  /**
   * ISO-8601 instant for historical read-only context cut. When present, timeline events and key blocks are assembled as if the authoritative graph/state were evaluated at or before this timestamp. Optional per ADR-009. Vector retrieval on the as_of path is optional (fallback to non-vector paths per ADR-007).
   */
  as_of?: string | null;
}
/**
 * Response shape for deferred direct platform cloud context assembly. Shipped V1.26 local assembly paths run in-process and do not receive this response from a daemon context-assemble Local API endpoint.
 *
 * This interface was referenced by `ContextAssemblyV1`'s JSON-Schema
 * via the `definition` "ContextAssembleResponseV1".
 */
export interface ContextAssembleResponseV1 {
  /**
   * Echo of request_id for correlation
   */
  request_id: string;
  /**
   * Whether the assembly succeeded
   */
  success: boolean;
  /**
   * Error code if success=false (e.g., 'auth_expired', 'world_not_found', 'platform_unavailable')
   */
  error_code?: string | null;
  /**
   * Human-readable error message if success=false
   */
  error_message?: string | null;
  /**
   * World ID (prefix: 'wld_')
   */
  world_id: string;
  /**
   * ISO 8601 / RFC 3339 UTC datetime string
   */
  assembled_at: string;
  /**
   * Freshness indicator (e.g., 'last_indexed_bundle_id') to detect stale data
   */
  data_freshness_hint?: string | null;
  /**
   * Confirmed KeyBlocks relevant to the world
   */
  key_blocks?: {
    key_block_id: string;
    block_type: string;
    name: string;
    summary: string;
  }[];
  /**
   * Recent canon timeline events
   */
  timeline_events?: {
    event_id: string;
    event_type: string;
    description: string;
    occurred_at: string;
  }[];
  /**
   * Story summaries from StoryManifest.summary_text
   */
  story_summaries?: {
    story_manifest_id: string;
    title: string;
    summary_text: string;
    manifest_type: string;
  }[];
  /**
   * Memory slices (story_summary, research_material, review_note)
   */
  memory_items?: {
    memory_id: string;
    memory_kind: string;
    content: string;
  }[];
}
