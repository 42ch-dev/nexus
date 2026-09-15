/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Owned write command for the durable JS-provider operation journal, replacing direct pool SQL. The sequence is assigned by the store; an already-terminal row is never downgraded.
 */
export interface CoreProviderJournalWrite {
  operation_id: string;
  session_id: string;
  provider_id: string;
  status: "running" | "finished" | "failed" | "interrupted" | "cancelled";
}
