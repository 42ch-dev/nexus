/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for marking notifications read (platform plan 20). Either pass explicit ids or mark_all.
 */
export interface NotificationsMarkReadRequest {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  /**
   * Ids to mark read; omit when using mark_all
   *
   * @maxItems 500
   */
  notification_ids?: string[];
  /**
   * When true, mark all visible notifications read for the principal
   */
  mark_all?: boolean;
}
