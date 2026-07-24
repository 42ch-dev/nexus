/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Query parameters for GET /v1/daemon/works/{work_id}/findings (cursor-based pagination, F-P2).
 */
export interface ListFindingsQuery {
  chapter?: number;
  status?: string;
  severity?: string;
  limit?: number;
  /**
   * Opaque pagination cursor returned by the previous response's `pagination.next_cursor`.
   */
  cursor?: string;
}
