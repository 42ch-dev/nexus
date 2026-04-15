import type { AccountStatus, SchemaVersion, SubscriptionTier } from './CommonTypes';
/**
 * MeEntitlementsResponseV1
 *
 * GET /me/entitlements 200 response body. SSOT: v1-spec schema/entitlements-wire-v1.md §3.
 *
 * @schema_version 1
 * @source me-entitlements-response.schema.json
 */

/** Inline enum type */
export type MeEntitlementsResponseRuntimePolicy = 'local_first' | 'cloud_enhanced';

/** GET /me/entitlements 200 response body. SSOT: v1-spec schema/entitlements-wire-v1.md §3. */
export interface MeEntitlementsResponse {
  schema_version: number;
  user_id: string;
  subscription_tier: SubscriptionTier;
  account_status: AccountStatus;
  official_creator: { eligible: boolean; max_concurrent_jobs: number };
  runtime_policy: MeEntitlementsResponseRuntimePolicy;
  memory_structured_write: boolean;
  memory_vector_index: boolean;
  local_first_embedding_remaining?: number;
}
