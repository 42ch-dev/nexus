/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Paginated notifications list (platform plan 20). Item shape matches NotificationsInboxItem fields for wire stability.
 */
export interface NotificationsListResponse {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  items: {
    notification_id: string;
    kind: "system" | "social" | "publish" | "workspace" | "other";
    title: string;
    body?: string;
    /**
     * ISO 8601 / RFC 3339 UTC datetime string
     */
    read_at?: string;
    /**
     * ISO 8601 / RFC 3339 UTC datetime string
     */
    created_at: string;
    link_url?: string;
  }[];
  next_cursor?: string;
  has_more: boolean;
}
