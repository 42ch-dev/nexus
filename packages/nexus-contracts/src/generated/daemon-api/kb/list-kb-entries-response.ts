/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for GET /v1/daemon/kb/entries.
 */
export interface ListKbEntriesResponse {
  items: NexusKbEntrySummary[];
  pagination: NexusPaginationInfo;
}
/**
 * Summary row for a KB entry in list responses.
 */
export interface NexusKbEntrySummary {
  entry_id: string;
  title: string;
  created_at: string;
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
