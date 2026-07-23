/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Read projection for GET /v1/daemon/worlds/{world_id}/kb/candidates (V1.73). Pending promotion candidates with cursor pagination.
 */
export interface WorldKbCandidatesResponse {
  items: NexusWorldKbCandidateProjection[];
  pagination: NexusPaginationInfo;
}
/**
 * Pending promotion candidate projection for the World KB promotion inspector (V1.73). Backed by kb_extract_jobs + the pending KeyBlock row.
 */
export interface NexusWorldKbCandidateProjection {
  /**
   * Pending KeyBlock id (key_block_id).
   */
  candidate_id: string;
  /**
   * Extract job id (kb_extract_jobs.job_id).
   */
  job_id: string;
  /**
   * Owning World identifier.
   */
  world_id: string;
  /**
   * KeyBlock content type (data-model-v1.md §5.5). V1.54 P1: added game-bible variants (species, faction, magic_system, technology, deity, level, economy_tier). V1.55 P3: added script variants (dialogue, beat, act). V1.123 P1: added era (cross-profile world-shape marker for Brief layer).
   */
  block_type:
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
  canonical_name: string;
  /**
   * Promotion state, normally `pending`.
   */
  status?: string;
  /**
   * Per-row OCC version of the candidate row.
   */
  version: number;
  source_anchor_count?: number;
  created_at?: string;
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
