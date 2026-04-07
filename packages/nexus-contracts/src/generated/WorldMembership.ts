import type { SchemaVersion } from './CommonTypes';
/**
 * Nexus WorldMembership
 *
 * WorldMembership entity describing Creator-World relationship with roles and permissions. Aligned with data-model-v1.md §5.4.
 *
 * @schema_version 1
 * @source world-membership.schema.json
 */

/** Inline enum type */
export type WorldMembershipRole = 'owner' | 'maintainer' | 'collaborator' | 'official_creator';

/** Inline enum type */
export type WorldMembershipMembershipStatus = 'active' | 'invited' | 'suspended' | 'removed';

/** WorldMembership entity describing Creator-World relationship with roles and permissions. Aligned with data-model-v1.md §5.4. */
export interface WorldMembership {
  schema_version: number;
  membership_id: string;
  world_id: string;
  creator_id: string;
  role: WorldMembershipRole;
  membership_status: WorldMembershipMembershipStatus;
  joined_at: string;
  permissions?: { can_sync_kb?: boolean; can_publish?: boolean; can_fork?: boolean; can_invite_official_creator?: boolean; can_confirm_canon?: boolean };
}
