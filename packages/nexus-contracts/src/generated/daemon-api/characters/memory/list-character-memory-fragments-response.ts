/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for GET /v1/daemon/characters/{character_id}/memory/fragments (cursor pagination). Fragments are ordered created_at DESC, fragment_id DESC (deterministic).
 */
export interface ListCharacterMemoryFragmentsResponse {
  fragments: NexusCharacterMemoryFragmentInfo[];
  pagination: NexusPaginationInfo;
}
/**
 * One Character memory fragment. `binding_id` is the binding-local provenance; absent means shared Character scope. `revision` backs optimistic-concurrency promotion (local to shared).
 */
export interface NexusCharacterMemoryFragmentInfo {
  fragment_id: string;
  session_id: string;
  /**
   * Character ID (lowercase prefix chr_ and exactly 32 hex characters)
   */
  character_id: string;
  /**
   * Binding-local provenance; omitted for shared Character scope.
   */
  binding_id?: string;
  summary: string;
  keywords: string[];
  /**
   * ISO 8601 / RFC 3339 UTC datetime string
   */
  created_at: string;
  /**
   * Optional TTL hint (e.g. `30d`).
   */
  ttl?: string;
  /**
   * OCC revision; required as `expected_revision` for promotion.
   */
  revision: number;
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
