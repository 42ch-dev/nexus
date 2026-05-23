import type { SchemaVersion } from './CommonTypes';
/**
 * ContextAssemblyV1
 *
 * Context Assembly request/response schemas retained for deferred direct platform cloud context assembly and CLI local in-process context assembly flows. In V1.26, only local CLI assembly is shipped: assemble-local uses Stage0/TwoStage in-process assembly, and assemble-moment uses local four-domain Moment assembly. There is no active daemon context-assemble Local API endpoint.
 *
 * @schema_version 1
 * @source context-assembly-v1.schema.json
 */

/** Inline enum type */
export type ContextAssembleRequestV1MemoryKinds = 'story_summary' | 'research_material' | 'review_note' | 'character_note' | 'world_building' | 'plot_outline' | 'theme_analysis' | 'location_reference' | 'timeline_note' | 'dialogue_snippet' | 'symbol_motif' | 'custom';

/** Request shape for deferred direct platform cloud context assembly. CLI may use this shape when platform cloud assembly becomes available; V1.26 shipped context assembly is local-only and does not send this request to a daemon context-assemble Local API endpoint. */
export interface ContextAssembleRequestV1 {
  request_id: string;
  workspace_id: string;
  creator_id: string;
  world_id: string;
  include_memory?: boolean;
  include_timeline?: boolean;
  include_story_summaries?: boolean;
  branch_id?: string | null;
  memory_query?: string | null;
  timeline_limit?: number;
  key_block_limit?: number;
  memory_kinds?: ContextAssembleRequestV1MemoryKinds[];
  max_timeline_events?: number | null;
  max_story_summaries?: number | null;
  as_of?: string | null;
}
/** Response shape for deferred direct platform cloud context assembly. Shipped V1.26 local assembly paths run in-process and do not receive this response from a daemon context-assemble Local API endpoint. */
export interface ContextAssembleResponseV1 {
  request_id: string;
  success: boolean;
  error_code?: string | null;
  error_message?: string | null;
  world_id: string;
  assembled_at: string;
  data_freshness_hint?: string | null;
  key_blocks?: { key_block_id: string; block_type: string; name: string; summary: string }[];
  timeline_events?: { event_id: string; event_type: string; description: string; occurred_at: string }[];
  story_summaries?: { story_manifest_id: string; title: string; summary_text: string; manifest_type: string }[];
  memory_items?: { memory_id: string; memory_kind: string; content: string }[];
}
