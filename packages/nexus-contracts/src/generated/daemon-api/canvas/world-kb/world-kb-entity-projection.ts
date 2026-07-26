/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Flat wire projection of a World KB KnowledgeEntry entity for canvas graph + inspector surfaces (V1.73). `version` maps to the SQLite per-row OCC column (kb_key_blocks.revision, NULL-normalized to 0).
 */
export interface WorldKbEntityProjection {
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
