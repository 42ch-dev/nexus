/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for GET /v1/daemon/kb/entries/{entry_id}.
 */
export interface GetKbEntryResponse {
  entry_id: string;
  title: string;
  created_at: string;
  content: string;
}
