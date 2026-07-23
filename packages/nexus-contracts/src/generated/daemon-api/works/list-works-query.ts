/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Query parameters for GET /v1/daemon/works (cursor-based pagination + sort, F-P1 / F-F1).
 */
export interface ListWorksQuery {
  status?: string;
  intake_status?: string;
  limit?: number;
  /**
   * Opaque pagination cursor returned by the previous response's `pagination.next_cursor`.
   */
  cursor?: string;
  /**
   * Comma-separated sort terms. Allowed keys: `updated_at` (default), `title`, `status`, `intake_status`. Prefix `-` for descending.
   */
  sort?: string;
}
