/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for GET /v1/daemon/works (cursor-based pagination, F-P3). The array field is `items`; the legacy `works` key was removed in `@42ch/nexus-contracts` 0.6.0.
 */
export interface ListWorksResponse {
  items: NexusWorkSummary[];
  pagination: NexusPaginationInfo;
}
/**
 * Summary row for a work in list responses.
 */
export interface NexusWorkSummary {
  work_id: string;
  title: string;
  status: string;
  intake_status: string;
  primary_preset_id: string;
  updated_at: string;
  completion_locked_at?: string;
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
