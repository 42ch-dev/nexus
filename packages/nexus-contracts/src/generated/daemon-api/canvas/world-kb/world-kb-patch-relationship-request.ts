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
 * Request body for POST /v1/daemon/worlds/{world_id}/kb/patch-relationship (V1.74). Action-discriminated add/update/remove for typed World KB relationships with per-row OCC on kb_relationships.revision.
 */
export interface WorldKbPatchRelationshipRequest {
  /**
   * Storage row id; required for update and remove, omitted for add.
   */
  relationship_id?: string;
  /**
   * Discriminator: add creates a new row; update mutates an existing row; remove deletes it.
   */
  action: "add" | "update" | "remove";
  /**
   * Per-row version observed by the client; required for update/remove, omitted or 0 for add.
   */
  expected_version?: number;
  relationship?: NexusWorldKbRelationshipInput;
}
/**
 * Payload required for add and update; omitted for remove.
 */
export interface NexusWorldKbRelationshipInput {
  /**
   * Source KnowledgeEntry id. Must be a non-deleted entity in the same world.
   */
  source_entity_id: string;
  /**
   * Target KnowledgeEntry id. Must be a non-deleted entity in the same world and different from source_entity_id.
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
