/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

export interface CoreChangeRow {
  sequence: string;
  world_id: string;
  resource_kind: string;
  resource_id: string;
  /**
   * Nullable decimal-string revision.
   */
  resource_revision?: string | null;
  change_kind: string;
  writer_id: string;
}
