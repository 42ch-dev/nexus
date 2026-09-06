/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for POST /v1/daemon/agent-host/sessions/{session_id}/operations.
 */
export interface OperationResponse {
  operation_id: string;
  session_id: string;
  status: string;
}
