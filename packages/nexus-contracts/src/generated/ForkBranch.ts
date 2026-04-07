import type { SchemaVersion } from './CommonTypes';

/**
 * Nexus ForkBranch
 *
 * ForkBranch - describes a world branch forked from a parent world at a specific event. Aligned with data-model-v1.md §5.7.
 *
 * @schema_version 1
 * @source fork-branch.schema.json
 */

/** Inline enum type */
export type ForkBranchStatus = 'active' | 'archived';

/** Inline enum type */
export type ForkBranchVerificationStatus = 'unverified' | 'requested' | 'verified' | 'rejected';

/** ForkBranch - describes a world branch forked from a parent world at a specific event. Aligned with data-model-v1.md §5.7. */
export interface ForkBranch {
  schema_version: number;
  fork_branch_id: string;
  world_id: string;
  parent_world_id: string;
  parent_branch_id: string;
  forked_from_event_id: string;
  status: ForkBranchStatus;
  verification_status: ForkBranchVerificationStatus;
  created_by_creator_id: string;
  created_at: string;
}
