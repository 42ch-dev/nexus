/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Filter chapters by status.
 */
export type NexusChapterStatus = "not_started" | "outlined" | "draft" | "finalized" | "published";

/**
 * Query parameters for GET /v1/daemon/works/{work_id}/chapters (V1.65 P0). Cursor-based pagination with optional status filter.
 */
export interface ListChaptersQuery {
  status?: NexusChapterStatus;
  /**
   * Page size. Default set by handler.
   */
  limit?: number;
  /**
   * Opaque pagination cursor returned by the previous response's `pagination.next_cursor`.
   */
  cursor?: string;
}
