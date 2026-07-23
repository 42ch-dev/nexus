/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * GET /official-creator/quota 200 response body. SSOT: v1-spec schema/entitlements-wire-v1.md §4.
 */
export interface OfficialCreatorQuotaResponse {
  /**
   * Wire revision; V1.0 fixed to 1 per entitlements-wire-v1.md
   */
  schema_version: 1;
  /**
   * User ID (prefix: 'usr_')
   */
  user_id: string;
  /**
   * ISO 8601 / RFC 3339 UTC datetime string
   */
  quota_period_start: string;
  /**
   * ISO 8601 / RFC 3339 UTC datetime string
   */
  quota_period_end: string;
  official_runs_consumed: number;
  official_runs_limit: number;
  official_runs_remaining: number;
  max_concurrent_official_jobs: number;
}
