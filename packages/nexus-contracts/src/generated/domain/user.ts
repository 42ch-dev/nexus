/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * End-user account for authentication and platform identity. Aligned with data-model-v1.md §5.1.
 */
export interface User {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  /**
   * Unique user identifier
   */
  user_id: string;
  /**
   * Unique login handle
   */
  username: string;
  /**
   * Primary email address
   */
  email: string;
  /**
   * Human-readable display name
   */
  display_name: string;
  /**
   * Account lifecycle state
   */
  account_status: "active" | "suspended" | "deleted";
  /**
   * Subscription / entitlements tier
   */
  subscription_tier: "free" | "pro" | "studio" | "enterprise";
  /**
   * Account creation time
   */
  created_at: string;
  /**
   * Last profile or status update
   */
  updated_at?: string;
}
