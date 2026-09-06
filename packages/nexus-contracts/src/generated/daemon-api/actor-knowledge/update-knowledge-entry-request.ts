/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for PATCH /v1/daemon/characters/:character_id/knowledge/:entry_id. Only canonical_name and summary are mutable; null removes summary, omission keeps it.
 */
export interface UpdateKnowledgeEntryRequest {
  expected_revision: number;
  canonical_name?: string;
  /**
   * Plain UTF-8 summary stored in body.summary. Null removes the summary member; omission keeps it.
   */
  summary?: string | null;
}
