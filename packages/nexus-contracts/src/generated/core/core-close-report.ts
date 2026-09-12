/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

export interface CoreCloseReport {
  state: "closed" | "interrupted";
  cleanup_confirmed: boolean;
  pending_operations: string[];
  reason?: "user_requested" | "engine_replaced" | "schema_mismatch" | "writer_fenced" | null;
}
