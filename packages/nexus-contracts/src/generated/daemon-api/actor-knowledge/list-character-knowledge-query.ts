/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Query parameters for GET /v1/daemon/characters/:character_id/knowledge. Character-owned listing without a World filter; not an unbounded all-World Creator view.
 */
export interface ListCharacterKnowledgeQuery {
  limit?: number;
  cursor?: string;
}
