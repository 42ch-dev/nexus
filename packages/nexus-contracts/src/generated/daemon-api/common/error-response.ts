/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Canonical Daemon API error detail. The daemon wraps this as `{ success: false, error: ErrorResponse, request_id?: string }` on the wire; this schema models the stable, contract-locked `error` detail shared across all Daemon API failure paths (F-E1).
 */
export interface ErrorResponse {
  /**
   * Stable, machine-readable error code. New endpoints use lowercase snake_case `<resource>_<failure>` (e.g. `work_not_found`); the daemon's legacy coarse codes (e.g. `INVALID_INPUT`, `NOT_FOUND`) remain valid.
   */
  code: string;
  /**
   * Human-readable, actionable error message safe for CLI/UI display.
   */
  message: string;
  /**
   * Optional structured context such as IDs, field names, or validation paths. Do not place unstructured stack traces here.
   */
  details?: {
    [k: string]: unknown | undefined;
  };
}
