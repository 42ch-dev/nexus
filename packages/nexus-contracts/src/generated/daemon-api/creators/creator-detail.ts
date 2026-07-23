/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for GET /v1/daemon/creators/{creator_id}.
 */
export interface CreatorDetail {
  creator_id: string;
  handle?: string;
  display_name?: string;
  has_api_key: boolean;
  has_cached_token: boolean;
  is_active: boolean;
}
