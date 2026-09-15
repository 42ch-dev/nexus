/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Local outbox status: bounded page of durable send-queue entries with delivery state, retry accounting and last error. Entry field spelling is snake_case, preserved from the existing outbox entry surface.
 */
export interface CoreOutboxStatus {
  entries: {
    schema_version: 1;
    outbox_entry_id: string;
    bundle_id: string;
    idempotency_key: string;
    delivery_state: "staged" | "ready" | "sent" | "acked" | "conflicted" | "failed";
    retry_count?: number | null;
    last_error?: string | null;
    /**
     * RFC 3339 timestamp of the next scheduled retry.
     */
    next_retry_at?: string | null;
    created_at: string;
    updated_at?: string | null;
  }[];
}
