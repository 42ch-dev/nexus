/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Handler-local wire DTOs for the narrative read surface (`narrative-api` destination): the `WorldState` read-model projection and the list/get envelopes of `GET /v1/daemon/narrative/worlds`. Mirrors `nexus-narrative`'s serialized `WorldState` verbatim (read-only projection, not the authoritative domain aggregate).
 */
export type NarrativeApi = NarrativeWorldsListResponse;

/**
 * Response for `GET /v1/daemon/narrative/worlds` (empty list when no Worlds are seeded).
 */
export interface NarrativeWorldsListResponse {
  worlds: NarrativeWorldState[];
}
/**
 * Read-model projection of one World (aggregated narrative state; never the stored row).
 */
export interface NarrativeWorldState {
  world_id: string;
  title: string;
  slug: string;
  /**
   * World status (`active` / `archived` / `paused`).
   */
  status: string;
  is_fork: boolean;
  fork_branch_id?: string;
  parent_world_id?: string;
  forked_from_event_id?: string;
  /**
   * Canon revision counter.
   */
  canon_revision?: number;
  current_timeline_head_id?: string;
  current_time_pointer?: string;
  /**
   * RFC 3339 creation timestamp.
   */
  created_at: string;
}
