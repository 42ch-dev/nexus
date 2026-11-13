/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Query parameters for GET /v1/daemon/patterns (cursor-based pagination, `package_dir` ascending as UTF-8 bytes). No `sort` parameter is exposed.
 */
export interface ListPatternsQuery {
  /**
   * Opaque pagination cursor returned by the previous response's `pagination.next_cursor`. Clients MUST NOT parse it. It addresses one `package_dir` in a listing ordered by that field compared as UTF-8 bytes, which is the order the reader sorts in on every host — never a locale-dependent collation.
   */
  cursor?: string;
  /**
   * Maximum number of items to return, defaulting to 100. The ceiling is 200: one listing page of local packages that a UI can render as a list. A value outside 1–200 is a typed 400, never a silently clamped page.
   */
  limit?: number;
}
