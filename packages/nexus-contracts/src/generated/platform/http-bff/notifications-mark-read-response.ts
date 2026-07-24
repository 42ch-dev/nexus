/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for mark-read mutations (platform plan 20).
 */
export interface NotificationsMarkReadResponse {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  success: boolean;
  /**
   * Rows affected
   */
  updated_count: number;
  /**
   * Error detail when success is false
   */
  error?: string;
}
