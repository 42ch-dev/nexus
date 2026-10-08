/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

export interface CoreConnectGapEvent {
  reason: "lagging" | "oversized" | "history_unavailable" | "interrupted" | "stale_cursor";
  requires_transcript_reconciliation: true;
  operation_id: string | null;
  resync_required: true;
  inspect_url: string;
}
