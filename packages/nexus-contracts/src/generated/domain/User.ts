import type { AccountStatus, SchemaVersion, SubscriptionTier } from '../common/CommonTypes';
/**
 * Nexus User Entity
 *
 * End-user account for authentication and platform identity. Aligned with data-model-v1.md §5.1.
 *
 * @schema_version 1
 * @source user.schema.json
 */
/** End-user account for authentication and platform identity. Aligned with data-model-v1.md §5.1. */
export interface User {
  schema_version: number;
  user_id: string;
  username: string;
  email: string;
  display_name: string;
  account_status: AccountStatus;
  subscription_tier: SubscriptionTier;
  created_at: string;
  updated_at?: string;
}
