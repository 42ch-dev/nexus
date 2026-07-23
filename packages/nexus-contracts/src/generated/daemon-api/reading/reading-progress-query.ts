/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Query parameters for GET /v1/daemon/reading/progress. Creator scope is inferred from the active session.
 */
export interface ReadingProgressQuery {
  work_id: string;
  chapter: number;
}
