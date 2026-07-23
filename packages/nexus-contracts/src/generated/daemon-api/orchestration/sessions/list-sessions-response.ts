/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for GET /v1/daemon/orchestration/sessions (cursor-based pagination, F-P3). The array field is `items`; the legacy `sessions` key was removed in `@42ch/nexus-contracts` 0.6.0.
 */
export interface ListSessionsResponse {
  items: NexusOrchestrationSessionSummary[];
  pagination: NexusPaginationInfo;
}
/**
 * Summary of an active orchestration engine session.
 */
export interface NexusOrchestrationSessionSummary {
  session_id: string;
  creator_id: string;
  preset_id: string;
  status: string;
  current_task_id?: string;
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
