import type { BundleType, ManuscriptPhase, SchemaVersion, SourceAnchor } from './CommonTypes';

/**
 * Nexus DeltaBundle Envelope
 *
 * DeltaBundle envelope containing delta operations for world synchronization. Aligned with bundle-envelope-schema-v1.md §5.
 *
 * @schema_version 1
 * @source bundle.schema.json
 */

/** Inline enum type */
export type BundleDeltasDeltaType = 'world' | 'key_block' | 'timeline_event' | 'fork_branch' | 'memory_item' | 'story_manifest';

/** Inline enum type */
export type BundleDeltasOperation = 'create' | 'update' | 'upsert' | 'delete' | 'append';

/** Inline enum type */
export type BundleBundleApplyStatus = 'all_success' | 'partial' | 'failed';

/** Inline enum type */
export type BundleDeltaResultsDeltaApplyStatus = 'applied' | 'rejected' | 'skipped_dependency';

/** DeltaBundle envelope containing delta operations for world synchronization. Aligned with bundle-envelope-schema-v1.md §5. */
export interface Bundle {
  schema_version: number;
  bundle_id: string;
  command_id: string;
  workspace_id: string;
  world_id: string;
  creator_id: string;
  submitting_creator_id: string;
  bundle_type: BundleType;
  manuscript_phase?: ManuscriptPhase;
  output_manuscript?: boolean;
  idempotency_key: string;
  canonical_hash: string;
  base_versions: { world_revision?: number | null; timeline_head_id?: string; canon_revision?: number | null };
  last_confirmed_delta_sequence?: number;
  deltas: { delta_type: BundleDeltasDeltaType; operation: BundleDeltasOperation; target_entity_type?: string; target_entity_id?: string; payload: Record<string, unknown>; source_anchor?: SourceAnchor; local_timestamp: string }[];
  bundle_apply_status?: BundleBundleApplyStatus;
  delta_results?: { delta_index: number; delta_apply_status: BundleDeltaResultsDeltaApplyStatus; error_code?: string; applied_entity_revision?: number | null }[];
  created_at: string;
}
