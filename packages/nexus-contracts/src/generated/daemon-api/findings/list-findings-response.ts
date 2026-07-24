/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for GET /v1/daemon/works/{work_id}/findings (cursor-based pagination, F-P2). New list endpoints use the canonical `items` array key (convention §4); the `pagination` envelope reuses the shared `PaginationInfo`.
 */
export interface ListFindingsResponse {
  items: NexusFindingDetailResponse[];
  pagination: NexusPaginationInfo;
}
/**
 * Response for GET /v1/daemon/works/{work_id}/findings/{finding_id} and create/update responses.
 */
export interface NexusFindingDetailResponse {
  finding_id: string;
  work_id: string;
  chapter?: number;
  severity: string;
  status: string;
  title: string;
  description: string;
  target_executor: string;
  kind: string;
  rule_suggestion?: string;
  created_at: number;
  updated_at: number;
  routing_hint?: string;
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
