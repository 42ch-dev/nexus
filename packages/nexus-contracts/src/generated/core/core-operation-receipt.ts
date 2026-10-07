/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * One durable operation receipt (v1.207 P3, RN-OGA-5). Mirrors one `operation_receipts` row: the first-writer-wins record of a logical operation, asked BEFORE any re-apply so a retry/replay after cancel, timeout or crash is answered from the receipt instead of being applied twice. Status is terminal once finished/failed/cancelled/interrupted and is never downgraded back to running.
 */
export interface CoreOperationReceipt {
  /**
   * Durable operation id: `op_` prefix and exactly 32 lowercase hex characters.
   */
  operation_id: string;
  /**
   * The consumer lane that owns this receipt (compute Run or Connect invoke).
   */
  consumer: "compute_run" | "connect_invoke";
  /**
   * Consumer-scoped subject: compute Run `run_id`; Connect `<peer_session_id>/<op>`.
   */
  subject_id: string;
  /**
   * Receipt status. Terminal once finished/failed/cancelled/interrupted; never downgraded back to running.
   */
  status: "running" | "finished" | "failed" | "cancelled" | "interrupted";
  /**
   * SHA-256 of the canonical request. First-writer-wins: a conflicting write for the same operation id with a different fingerprint is refused `operation_id_conflict`, never silently deduped.
   */
  request_fingerprint: string;
  /**
   * Terminal result payload (JSON text); set only on `finished`.
   */
  result_json?: string | null;
  /**
   * Terminal error payload (JSON text); set only on `failed`.
   */
  error_json?: string | null;
  /**
   * RFC 3339 UTC timestamp of the receipt's first (running) write.
   */
  created_at: string;
  /**
   * RFC 3339 UTC timestamp of the last receipt write.
   */
  updated_at: string;
  /**
   * RFC 3339 UTC timestamp of the terminal settlement; null while running.
   */
  terminal_at?: string | null;
  /**
   * Monotonic store order assigned at write time.
   */
  sequence: number;
}
