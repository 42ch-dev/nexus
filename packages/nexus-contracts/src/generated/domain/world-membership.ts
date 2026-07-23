/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * WorldMembership entity describing Creator-World relationship with roles and permissions. Aligned with data-model-v1.md §5.4.
 */
export interface WorldMembership {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  /**
   * Unique membership identifier (prefix: 'mbr_')
   */
  membership_id: string;
  /**
   * World ID (prefix: 'wld_')
   */
  world_id: string;
  /**
   * Creator ID (prefix: 'ctr_')
   */
  creator_id: string;
  /**
   * Membership role
   */
  role: "owner" | "maintainer" | "collaborator" | "official_creator";
  /**
   * Membership status
   */
  membership_status: "active" | "invited" | "suspended" | "removed";
  /**
   * ISO 8601 / RFC 3339 UTC datetime string
   */
  joined_at: string;
  /**
   * Permission flags
   */
  permissions?: {
    can_sync_kb?: boolean;
    can_publish?: boolean;
    can_fork?: boolean;
    can_invite_official_creator?: boolean;
    can_confirm_canon?: boolean;
    [k: string]: unknown | undefined;
  };
}
