/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

export interface CoreChangesResponse {
  rows: NexusCoreChangeRow[];
  /**
   * Decimal string sequence watermark after this page.
   */
  next_sequence: string;
  /**
   * High-water snapshot sequence at read time.
   */
  snapshot_sequence: string;
  resync_required: boolean;
}
export interface NexusCoreChangeRow {
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
