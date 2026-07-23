/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Value object describing the baseline version of a bundle/entity/world. Aligned with data-model-v1.md §6.2.
 */
export interface VersionRef {
  /**
   * Entity type (e.g., 'world')
   */
  entity_type: string;
  /**
   * Entity ID
   */
  entity_id: string;
  /**
   * Revision number at baseline
   */
  revision: number;
}
