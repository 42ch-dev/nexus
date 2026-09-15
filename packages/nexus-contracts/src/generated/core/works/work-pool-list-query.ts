/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Query for `GET /v1/daemon/works/pool`.
 */
export interface WorkPoolListQuery {
  status?: string;
  limit?: number;
  offset?: number;
}
