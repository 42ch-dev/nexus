/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for GET /v1/daemon/creators/{creator_id}. holder_entry_id is the read-only service-managed holder projection of this already-authorized identity; there is no holder CRUD route.
 */
export interface CreatorDetail {
  creator_id: string;
  /**
   * Read-only service-managed holder KnowledgeEntry id for this Creator (`hld_` namespace). Never accepted from a request body.
   */
  holder_entry_id?: string;
  handle?: string;
  display_name?: string;
  has_api_key: boolean;
  has_cached_token: boolean;
  is_active: boolean;
}
