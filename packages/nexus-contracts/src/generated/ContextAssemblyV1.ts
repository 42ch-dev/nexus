import type { SchemaVersion } from './CommonTypes';

/**
 * ContextAssemblyV1
 *
 * Context Assembly request/response schemas for POST /v1/local/context/assemble. CLI sends request to request a stable read-only context snapshot from the platform.
 *
 * @schema_version 1
 * @source context-assembly-v1.schema.json
 */

/** Inline enum type */
export type ContextAssembleRequestV1MemoryKinds = 'story_summary' | 'research_material' | 'review_note';

/** Request shape for POST /v1/local/context/assemble. CLI sends this to request a stable read-only context snapshot from the platform. */
export interface ContextAssembleRequestV1 {
  request_id: string;
  workspace_id: string;
  creator_id: string;
  world_id: string;
  include_memory?: boolean;
  include_timeline?: boolean;
  include_story_summaries?: boolean;
  memory_kinds?: ContextAssembleRequestV1MemoryKinds[];
  max_timeline_events?: number | null;
  max_story_summaries?: number | null;
}
/** Response shape for POST /v1/local/context/assemble. Platform returns a stable read-only context snapshot. */
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
