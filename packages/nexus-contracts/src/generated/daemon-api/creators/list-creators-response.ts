/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for GET /v1/daemon/creators.
 */
export interface ListCreatorsResponse {
  items: NexusCreatorInfo[];
  pagination: NexusPaginationInfo;
}
/**
 * Creator info row from local identity store.
 */
export interface NexusCreatorInfo {
  creator_id: string;
  display_name: string;
  status: string;
  cached_at?: string;
}
/**
 * Cursor-based pagination metadata.
 */
export interface NexusPaginationInfo {
  limit: number;
  /**
   * Opaque cursor returned by the previous page. Clients MUST NOT parse it. Non-null only when another page exists.
   */
  next_cursor?: string;
  /**
   * True when the client may request another page (equivalent to `next_cursor` being non-null).
   */
  has_more: boolean;
}
