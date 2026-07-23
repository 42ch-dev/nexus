/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for listing notifications (platform plan 20).
 */
export interface NotificationsListRequest {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  cursor?: string;
  limit?: number;
  /**
   * When true, only unread rows
   */
  unread_only?: boolean;
}
