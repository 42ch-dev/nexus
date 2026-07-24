/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Pairing entity describing Creator <-> User association. Aligned with data-model-v1.md §5.2A.
 */
export interface Pairing {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  /**
   * Unique pairing identifier (prefix: 'prg_')
   */
  pairing_id: string;
  /**
   * Creator ID (prefix: 'ctr_')
   */
  creator_id: string;
  /**
   * User ID (prefix: 'usr_')
   */
  user_id: string;
  /**
   * How the pairing was established
   */
  pairing_source: "auto_cli" | "manual_web" | "platform_auto";
  /**
   * Pairing status
   */
  status: "active" | "revoked";
  /**
   * ISO 8601 / RFC 3339 UTC datetime string
   */
  created_at: string;
  /**
   * ISO 8601 / RFC 3339 UTC datetime string
   */
  revoked_at?: string;
}
