/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for GET /v1/daemon/orchestration/schedules (cursor-based pagination, F-P3). The array field is `items`; the legacy `schedules` key was removed in `@42ch/nexus-contracts` 0.6.0.
 */
export interface ListSchedulesResponse {
  /**
   * List of schedule summaries.
   */
  items: NexusScheduleSummary[];
  pagination: NexusPaginationInfo;
}
/**
 * Summary row for a schedule in list/inspect responses.
 */
export interface NexusScheduleSummary {
  /**
   * Unique schedule identifier.
   */
  schedule_id: string;
  /**
   * Owning creator ID.
   */
  creator_id: string;
  /**
   * Preset ID this schedule runs.
   */
  preset_id: string;
  /**
   * Current schedule status.
   */
  status: string;
  /**
   * Human-readable label.
   */
  label?: string;
  /**
   * Current core context version number.
   */
  current_core_context_version: number;
  /**
   * ISO-8601 creation timestamp.
   */
  created_at: string;
  /**
   * ISO-8601 last-update timestamp.
   */
  updated_at: string;
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
