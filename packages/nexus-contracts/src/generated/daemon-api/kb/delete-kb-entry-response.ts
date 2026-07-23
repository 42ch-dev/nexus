/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for DELETE /v1/daemon/kb/entries/{entry_id}.
 */
export interface DeleteKbEntryResponse {
  entry_id: string;
  deleted: boolean;
}
