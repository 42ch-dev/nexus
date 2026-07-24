/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Core taxonomy value; `custom` requires `custom_label`.
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
 * Author-editable payload for a World KB relationship (V1.74; V1.76 adds optional needs_review for promotion). Supplied inside WorldKbPatchRelationshipRequest for add/update actions.
 */
export interface WorldKbRelationshipInput {
  /**
   * Source KeyBlock id. Must be a non-deleted entity in the same world.
   */
  source_entity_id: string;
  /**
   * Target KeyBlock id. Must be a non-deleted entity in the same world and different from source_entity_id.
   */
  target_entity_id: string;
  relation_type: NexusWorldKbRelationshipKind;
  /**
   * Required narrative label when relation_type is `custom`; ignored for core enum values.
   */
  custom_label?: string;
  /**
   * When true, the graph read emits a derived reverse projection sharing the same relationship_id.
   */
  symmetric: boolean;
  /**
   * Optional author/assertion confidence, display-only in V1.74.
   */
  confidence?: number;
  /**
   * Optional source-anchor projection ids grounding this relationship.
   */
  source_anchor_ids?: string[];
  /**
   * Optional opaque JSON metadata.
   */
  metadata?: {
    [k: string]: unknown | undefined;
  };
  /**
   * V1.76: optional. When omitted on add, defaults to false (author-confirmed). When provided on update, sets the needs_review gate (false = promote/confirm the suggestion).
   */
  needs_review?: boolean;
}
