/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * ForkBranch - describes a world branch forked from a parent world at a specific event. Aligned with data-model-v1.md §5.7.
 */
export interface ForkBranch {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  /**
   * Unique ForkBranch identifier
   */
  fork_branch_id: string;
  /**
   * Child world ID (the fork)
   */
  world_id: string;
  /**
   * Parent world ID (the source)
   */
  parent_world_id: string;
  /**
   * Parent fork branch ID
   */
  parent_branch_id: string;
  /**
   * TimelineEvent where the fork occurred
   */
  forked_from_event_id: string;
  /**
   * ForkBranch status
   */
  status: "active" | "archived";
  /**
   * ForkBranch verification status
   */
  verification_status: "unverified" | "requested" | "verified" | "rejected";
  /**
   * Creator who initiated the fork
   */
  created_by_creator_id: string;
  /**
   * Fork creation timestamp
   */
  created_at: string;
}
