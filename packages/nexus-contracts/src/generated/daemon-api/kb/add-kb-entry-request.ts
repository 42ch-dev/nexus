/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for POST /v1/daemon/kb/entries.
 */
export interface AddKbEntryRequest {
  creator_id: string;
  workspace_slug?: string;
  scope?: string;
  title?: string;
  content?: string;
  file_path?: string;
}
