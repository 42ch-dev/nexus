/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

export interface CoreChangesRequest {
  /**
   * Non-negative decimal string cursor (exclusive lower bound).
   */
  after_sequence: string;
  limit?: number;
}
