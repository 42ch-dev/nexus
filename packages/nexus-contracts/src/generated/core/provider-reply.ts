/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Provider call reply envelope. Execute returns operation_id only, never the full transcript.
 */
export interface ProviderReply {
  request_id: string;
  ok: boolean;
  operation_id?: string | null;
  session_id?: string | null;
  health?: {
    provider_id: string;
    available: boolean;
    latency_ms?: number | null;
    message?: string | null;
  } | null;
  error?: NexusCoreError;
}
/**
 * Wire error envelope for nexus-core / native / service adapters (NexusApiError-compatible categories).
 */
export interface NexusCoreError {
  /**
   * NexusApiError-compatible category. not_supported refuses an operation this service does not serve (unsupported operations fail with this code and zero side effects; the finer lowercase peer wire code, e.g. op_unsupported, is preserved in details.wire_code).
   */
  code:
    | "uninitialized"
    | "auth_required"
    | "invalid_input"
    | "forbidden"
    | "not_found"
    | "not_supported"
    | "world_kb_conflict"
    | "world_kb_validation"
    | "writer_fenced"
    | "owner_busy"
    | "schema_mismatch"
    | "busy"
    | "closing"
    | "interrupted"
    | "internal";
  message: string;
  details?: {
    [k: string]: unknown | undefined;
  };
  http_status?: number;
}
