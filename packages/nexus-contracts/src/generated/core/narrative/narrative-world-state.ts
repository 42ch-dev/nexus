/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Read-model projection of one World (aggregated narrative state; never the stored row). Mirrors `nexus-narrative` serialized `WorldState` verbatim.
 */
export interface NarrativeWorldState {
  world_id: string;
  title: string;
  slug: string;
  status: string;
  is_fork: boolean;
  fork_branch_id?: string;
  parent_world_id?: string;
  forked_from_event_id?: string;
  canon_revision?: number;
  current_timeline_head_id?: string;
  current_time_pointer?: string;
  created_at: string;
}
