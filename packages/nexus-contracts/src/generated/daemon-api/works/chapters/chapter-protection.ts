/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Protection level describing what UI actions are allowed for a chapter (V1.65 P0).
 */
export interface ChapterProtection {
  /**
   * none = free edit; confirm_structure_edit = UI must show confirmation before structural edits; hard_block_delete = structural edits are blocked.
   */
  level: "none" | "confirm_structure_edit" | "hard_block_delete";
  /**
   * Human-readable explanation for the protection level.
   */
  reason: string;
}
