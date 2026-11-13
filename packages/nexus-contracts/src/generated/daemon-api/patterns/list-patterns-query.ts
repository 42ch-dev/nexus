/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Query parameters for GET /v1/daemon/patterns (cursor-based pagination, default `package_dir` ascending). No `sort` parameter is exposed.
 */
export interface ListPatternsQuery {
  /**
   * Opaque pagination cursor returned by the previous response's `pagination.next_cursor`. Clients MUST NOT parse it.
   */
  cursor?: string;
  /**
   * Maximum number of items to return.
   */
  limit?: number;
}
