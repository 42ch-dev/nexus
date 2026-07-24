/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Query parameters for GET /v1/daemon/kb/entries.
 */
export interface ListKbEntriesQuery {
  creator_id?: string;
  workspace_slug?: string;
  scope?: string;
  q?: string;
  limit?: number;
  cursor?: string;
}
