/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Core taxonomy values for World KB typed relationships (V1.74). Use `custom` with a non-empty `custom_label` for out-of-enum narrative relationships.
 */
export type NexusWorldKbRelationshipKind =
  | "allied_with"
  | "opposes"
  | "parent_of"
  | "child_of"
  | "member_of"
  | "located_in"
  | "rules_over"
  | "references"
  | "serves"
  | "rival_of"
  | "mentor_of"
  | "custom";

/**
 * Read projection for GET /v1/daemon/worlds/{world_id}/kb/graph (V1.73). Entities + source-anchor provenance edges. `relationships` is always empty in V1.73 (no kb_relationships table until V1.74); derived reference edges render read-only from source_anchors.
 */
export interface WorldKbGraphResponse {
  /**
   * All non-deleted KnowledgeEntry entities in the World (confirmed + pending + manual).
   */
  entities: NexusWorldKbEntityProjection[];
  /**
   * Provenance edges derived from kb_source_anchors.
   */
  source_anchors: NexusWorldKbSourceAnchorProjection[];
  /**
   * Typed World KB relationship projections (V1.74). Symmetric rows emit both stored and symmetric_reverse projections.
   */
  relationships: NexusWorldKbRelationshipProjection[];
}
/**
 * Flat wire projection of a World KB KnowledgeEntry entity for canvas graph + inspector surfaces (V1.73). `version` maps to the SQLite per-row OCC column (kb_key_blocks.revision, NULL-normalized to 0).
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
 * Provenance edge projection derived from kb_source_anchors (V1.73). Rendered read-only on the canvas graph.
 */
export interface NexusWorldKbSourceAnchorProjection {
  /**
   * Source anchor identifier.
   */
  source_anchor_id: string;
  /**
   * KnowledgeEntry the anchor attaches to.
   */
  key_block_id: string;
  /**
   * Origin kind (e.g. chapter, review, manual).
   */
  source_type: string;
  /**
   * Locator string (path / chapter ref / review id).
   */
  reference: string;
  /**
   * ISO-8601 timestamp.
   */
  created_at?: string;
}
/**
 * Canonical wire projection of a World KB relationship row (V1.74; V1.76 adds needs_review + source). One stored row may yield two projections when symmetric=true: the stored direction and a derived symmetric_reverse direction.
 */
export interface NexusWorldKbRelationshipProjection {
  /**
   * Storage row identifier.
   */
  relationship_id: string;
  /**
   * Owning World identifier.
   */
  world_id: string;
  /**
   * Source KnowledgeEntry id.
   */
  source_entity_id: string;
  /**
   * Target KnowledgeEntry id.
   */
  target_entity_id: string;
  relation_type: NexusWorldKbRelationshipKind;
  /**
   * Narrative label when relation_type is `custom`.
   */
  custom_label?: string;
  /**
   * True when the stored row is meant to project in both directions.
   */
  symmetric: boolean;
  /**
   * Display-only confidence when present.
   */
  confidence?: number;
  /**
   * Grounding source-anchor projection ids (empty for asserted relationships).
   */
  source_anchor_ids: string[];
  /**
   * Opaque JSON metadata when present.
   */
  metadata?: {
    [k: string]: unknown | undefined;
  };
  /**
   * V1.76: true when the edge is an extraction suggestion not yet author-confirmed. The GET graph defaults to excluding needs_review rows; promotion clears the flag.
   */
  needs_review: boolean;
  /**
   * V1.76: provenance. 'manual' = author-created via patch route; 'extraction' = proposed by nexus.llm.extract. Read-only.
   */
  source: "manual" | "extraction";
  /**
   * Per-row OCC revision (kb_relationships.revision).
   */
  version: number;
  /**
   * ISO-8601 timestamp of last mutation.
   */
  updated_at: string;
  /**
   * stored = the row's authored direction; symmetric_reverse = derived reverse edge for symmetric rows.
   */
  projection_direction: "stored" | "symmetric_reverse";
}
