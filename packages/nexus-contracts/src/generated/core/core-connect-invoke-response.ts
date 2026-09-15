/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Canonical Connect invoke result envelope: exactly one of result or error is non-null, encoded as a closed success/failure union. Error codes reuse the shared CoreError categories, including not_supported for operations this service does not serve.
 */
export type CoreConnectInvokeResponse = ConnectInvokeSuccess | ConnectInvokeFailure;

/**
 * Operation succeeded: result carries the payload and error is null.
 */
export interface ConnectInvokeSuccess {
  /**
   * Operation result; non-null exactly when error is null.
   */
  result: {
    [k: string]: unknown | undefined;
  };
  /**
   * Null on success.
   */
  error: null;
}
/**
 * Operation refused or failed: error carries the wire envelope and result is null.
 */
export interface ConnectInvokeFailure {
  /**
   * Null on failure.
   */
  result: null;
  error: NexusCoreError;
}
/**
 * Wire error envelope; non-null exactly when result is null.
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
