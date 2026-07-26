/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for POST /v1/daemon/worlds/{world_id}/kb/promote-candidate (V1.73). adopt/reject/merge a pending candidate via the entity-scope-model §5.5.2 promotion state machine. Per-row OCC on kb_extract_jobs.version.
 */
export interface WorldKbPromoteCandidateRequest {
  /**
   * Extract job id (kb_extract_jobs.job_id).
   */
  job_id: string;
  /**
   * Pending KnowledgeEntry id (key_block_id).
   */
  candidate_id: string;
  /**
   * Promotion action (entity-scope-model §5.5.2). `merge` requires merge_target_id.
   */
  action: "adopt" | "reject" | "merge";
  /**
   * Per-row version of the candidate row (kb_extract_jobs.version).
   */
  expected_version: number;
  /**
   * Confirmed KnowledgeEntry id to merge into. Required when action=merge.
   */
  merge_target_id?: string;
  patch?: NexusWorldKbEntityPatch;
}
/**
 * Optional fields applied on adopt (e.g. refine title/body before confirming).
 */
export interface NexusWorldKbEntityPatch {
  /**
   * New canonical_name (display title).
   */
  title?: string;
  /**
   * Replacement KnowledgeEntry body JSON (summary/attributes/tags/state/computable).
   */
  body?: {
    [k: string]: unknown | undefined;
  };
  /**
   * Replacement alias list.
   */
  aliases?: string[];
  /**
   * Re-classify the entity. Must be a valid BlockType (entity-scope-model §5.1.1).
   */
  block_type?:
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
}
