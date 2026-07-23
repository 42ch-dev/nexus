/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Single inbox notification row (platform plan 20).
 */
export interface NotificationsInboxItem {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  /**
   * Notification id
   */
  notification_id: string;
  /**
   * Category for routing and UI
   */
  kind: "system" | "social" | "publish" | "workspace" | "other";
  title: string;
  /**
   * Optional detail body
   */
  body?: string;
  /**
   * When marked read; omit when unread
   */
  read_at?: string;
  /**
   * ISO 8601 / RFC 3339 UTC datetime string
   */
  created_at: string;
  /**
   * Deep link for clients when safe to expose
   */
  link_url?: string;
}
