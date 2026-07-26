/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Success response for POST /v1/daemon/worlds/{world_id}/kb/promote-candidate (V1.73). `entity` is the resulting (or null for reject) KnowledgeEntry; `job` is the updated extract-job projection; `version` is the new per-row version.
 */
export interface WorldKbPromoteCandidateResponse {
  entity?: NexusWorldKbEntityProjection;
  job: NexusWorldKbExtractJobProjection;
  /**
   * New per-row version after the promotion was persisted.
   */
  version: number;
  validation_summary: {
    errors: string[];
    warnings: string[];
  };
}
/**
 * Resulting confirmed/merged KnowledgeEntry. Omitted for reject.
 */
export interface NexusWorldKbEntityProjection {
  /**
   * KnowledgeEntry identifier.
   */
  key_block_id: string;
  /**
   * Owning World identifier.
   */
  world_id: string;
  /**
   * Entity content type (entity-scope-model §5.1.1).
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
  /**
   * Display title / canonical name.
   */
  canonical_name: string;
  /**
   * Promotion lifecycle state: pending | confirmed | rejected | merged | manual | deleted (entity-scope-model §5.5).
   */
  status: string;
  /**
   * Per-row OCC revision (kb_key_blocks.revision, NULL normalized to 0).
   */
  version: number;
  /**
   * KnowledgeEntry body JSON (summary/attributes/tags/state/computable) when present.
   */
  body?: {
    [k: string]: unknown | undefined;
  };
  /**
   * Alias names for the entity.
   */
  aliases?: string[];
  /**
   * Number of source-anchor provenance edges referencing this entity.
   */
  source_anchor_count?: number;
  /**
   * ISO-8601 timestamp of last mutation.
   */
  updated_at?: string;
}
/**
 * Extract-job projection returned after a promotion action (V1.73). `version` maps to kb_extract_jobs.version CAS column.
 */
export interface NexusWorldKbExtractJobProjection {
  /**
   * Extract job id.
   */
  job_id: string;
  world_id: string;
  /**
   * Job lifecycle status.
   */
  status: string;
  /**
   * Per-row OCC version (kb_extract_jobs.version).
   */
  version: number;
  /**
   * Remaining candidate key_block_ids attached to the job.
   */
  candidate_ids?: string[];
  updated_at?: string;
}
