/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * One durable JS-provider operation journal record (bounded retention, newest-first read). Status is terminal once finished/failed/interrupted/cancelled and is never downgraded back to running by a restart.
 */
export interface CoreProviderOperation {
  operation_id: string;
  session_id: string;
  provider_id: string;
  status: "running" | "finished" | "failed" | "interrupted" | "cancelled";
  /**
   * Monotonic journal sequence assigned at write time.
   */
  sequence: number;
}
