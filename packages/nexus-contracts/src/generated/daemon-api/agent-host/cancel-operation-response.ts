/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for POST /v1/daemon/agent-host/operations/{operation_id}:cancel.
 */
export interface CancelOperationResponse {
  operation_id: string;
  status: string;
}
