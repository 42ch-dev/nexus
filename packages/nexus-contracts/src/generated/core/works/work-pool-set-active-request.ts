/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Body for `POST /v1/daemon/works/pool` (durable pool-active Work selection). `creator_id` is legacy parity only and never authorizes: the stored Principal is minted natively.
 */
export interface WorkPoolSetActiveRequest {
  action: string;
  work_id: string;
  creator_id?: string;
}
