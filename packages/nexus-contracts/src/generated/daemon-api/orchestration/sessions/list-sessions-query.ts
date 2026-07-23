/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Query parameters for GET /v1/daemon/orchestration/sessions (cursor-based pagination + sort, F-F1).
 */
export interface ListSessionsQuery {
  creator_id?: string;
  /**
   * Maximum number of items to return.
   */
  limit?: number;
  /**
   * Opaque pagination cursor returned by the previous response's `pagination.next_cursor`.
   */
  cursor?: string;
  /**
   * Comma-separated sort terms. Allowed keys: `session_id` (default), `creator_id`, `preset_id`, `status`. Prefix `-` for descending.
   */
  sort?: string;
}
