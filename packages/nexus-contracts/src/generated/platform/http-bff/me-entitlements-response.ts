/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * GET /me/entitlements 200 response body. SSOT: v1-spec schema/entitlements-wire-v1.md §3.
 */
export interface MeEntitlementsResponse {
  /**
   * Wire revision; V1.0 fixed to 1 per entitlements-wire-v1.md
   */
  schema_version: 1;
  /**
   * User ID (prefix: 'usr_')
   */
  user_id: string;
  /**
   * User subscription tier (data-model-v1.md §5.1)
   */
  subscription_tier: "free" | "pro" | "studio" | "enterprise";
  /**
   * User account status (data-model-v1.md §5.1)
   */
  account_status: "active" | "suspended" | "deleted";
  official_creator: {
    eligible: boolean;
    max_concurrent_jobs: number;
  };
}
