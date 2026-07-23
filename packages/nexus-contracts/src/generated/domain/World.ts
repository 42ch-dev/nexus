/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * World entity - a narrative universe maintained by creators with timeline evolution. Aligned with data-model-v1.md §5.3.
 */
export interface World {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  /**
   * Unique world identifier
   */
  world_id: string;
  /**
   * World owner creator ID
   */
  owner_creator_id: string;
  /**
   * World title
   */
  title: string;
  /**
   * URL-friendly world slug
   */
  slug: string;
  /**
   * World status
   */
  status: "active" | "paused" | "archived";
  /**
   * World visibility
   */
  visibility: "private" | "unlisted" | "public";
  /**
   * Timeline evolution policy
   */
  time_policy: "manual" | "owner_driven" | "event_driven";
  /**
   * Current canon revision number
   */
  canon_revision?: number;
  /**
   * Current timeline head event ID
   */
  current_timeline_head_id?: string;
  /**
   * World time progression pointer
   */
  current_time_pointer?: string;
  /**
   * Root fork branch ID
   */
  root_fork_branch_id?: string;
  /**
   * World rule flags
   */
  world_rules?: {
    time_moves_forward?: boolean;
    history_mutation_requires_fork?: boolean;
    [k: string]: unknown | undefined;
  };
  /**
   * World creation timestamp
   */
  created_at: string;
  /**
   * Last update timestamp
   */
  updated_at?: string;
}
