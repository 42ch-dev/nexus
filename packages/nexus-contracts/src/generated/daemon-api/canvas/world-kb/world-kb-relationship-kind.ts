/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Core taxonomy values for World KB typed relationships (V1.74). Use `custom` with a non-empty `custom_label` for out-of-enum narrative relationships.
 */
export type WorldKbRelationshipKind =
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
