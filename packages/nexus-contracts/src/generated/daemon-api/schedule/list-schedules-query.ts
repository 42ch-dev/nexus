/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Query parameters for GET /v1/daemon/orchestration/schedules (cursor-based pagination + sort, F-F1).
 */
export interface ListSchedulesQuery {
  /**
   * Filter by creator ID.
   */
  creator_id?: string;
  /**
   * Filter by schedule status.
   */
  status?: string;
  /**
   * Maximum number of items to return.
   */
  limit?: number;
  /**
   * Opaque pagination cursor returned by the previous response's `pagination.next_cursor`.
   */
  cursor?: string;
  /**
   * Comma-separated sort terms. Allowed keys: `created_at` (default), `updated_at`, `status`, `preset_id`, `label`. Prefix `-` for descending.
   */
  sort?: string;
}
