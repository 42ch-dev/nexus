/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Canonical Connect invoke result envelope: exactly one of result or error is non-null. Error codes reuse the shared CoreError categories, including unsupported operations.
 */
export interface CoreConnectInvokeResponse {
  /**
   * Operation result; null on failure.
   */
  result: {
    [k: string]: unknown | undefined;
  } | null;
  /**
   * Wire error envelope; null on success.
   */
  error: NexusCoreError | null;
}
/**
 * Wire error envelope for nexus-core / native / service adapters (NexusApiError-compatible categories).
 */
export interface NexusCoreError {
  code:
    | "uninitialized"
    | "auth_required"
    | "invalid_input"
    | "forbidden"
    | "not_found"
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
