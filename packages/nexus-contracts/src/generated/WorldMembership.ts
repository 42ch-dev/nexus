import type { MembershipRole, MembershipStatus, SchemaVersion } from './CommonTypes';
/**
 * Nexus WorldMembership
 *
 * WorldMembership entity describing Creator-World relationship with roles and permissions. Aligned with data-model-v1.md §5.4.
 *
 * @schema_version 1
 * @source world-membership.schema.json
 */
/** WorldMembership entity describing Creator-World relationship with roles and permissions. Aligned with data-model-v1.md §5.4. */
export interface WorldMembership {
  schema_version: number;
  membership_id: string;
  world_id: string;
  creator_id: string;
  role: MembershipRole;
  membership_status: MembershipStatus;
  joined_at: string;
  permissions?: { can_sync_kb?: boolean; can_publish?: boolean; can_fork?: boolean; can_invite_official_creator?: boolean; can_confirm_canon?: boolean };
}
