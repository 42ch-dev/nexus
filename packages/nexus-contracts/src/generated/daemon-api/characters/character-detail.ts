/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for GET /v1/daemon/characters/:character_id.
 */
export interface CharacterDetail {
  character: NexusCharacter;
}
/**
 * Durable Creator-owned Character bearer. Clients never send owner_creator_id on create; the field is stored/read only.
 */
export interface NexusCharacter {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  /**
   * Character ID (lowercase prefix chr_ and exactly 32 hex characters)
   */
  character_id: string;
  /**
   * Owning Creator ID (`CreatorId`). Never accepted from create/bind request bodies.
   */
  owner_creator_id: string;
  /**
   * Character display name. Trimmed non-empty; at most 120 Unicode scalars.
   */
  display_name: string;
  /**
   * Character bearer status. v1.184 product surfaces never archive a Character.
   */
  status: "active" | "archived";
  /**
   * Optional Character-owned image URI (metadata only; at most 2048 bytes).
   */
  image_uri?: string;
  /**
   * Character-owned persona metadata object. Not a Canvas asset system. Serialized JSON at most 16384 bytes.
   */
  persona: {
    [k: string]: unknown | undefined;
  };
  /**
   * ISO 8601 / RFC 3339 UTC datetime string
   */
  created_at: string;
  /**
   * ISO 8601 / RFC 3339 UTC datetime string
   */
  updated_at: string;
}
