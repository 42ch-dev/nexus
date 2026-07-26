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
 * Canonical wire projection of a World KB relationship row (V1.74; V1.76 adds needs_review + source). One stored row may yield two projections when symmetric=true: the stored direction and a derived symmetric_reverse direction.
 */
export interface WorldKbRelationshipProjection {
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
