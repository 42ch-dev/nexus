import type { SchemaVersion } from './CommonTypes';
/**
 * OfficialCreatorQuotaResponseV1
 *
 * GET /official-creator/quota 200 response body. SSOT: v1-spec schema/entitlements-wire-v1.md §4.
 *
 * @schema_version 1
 * @source official-creator-quota-response.schema.json
 */
/** GET /official-creator/quota 200 response body. SSOT: v1-spec schema/entitlements-wire-v1.md §4. */
export interface OfficialCreatorQuotaResponse {
  schema_version: number;
  user_id: string;
  quota_period_start: string;
  quota_period_end: string;
  official_runs_consumed: number;
  official_runs_limit: number;
  official_runs_remaining: number;
  max_concurrent_official_jobs: number;
}
