/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Host tool execution result: success flag plus the extensible JSON result produced by the tool handler. The result is an unrestricted JSON value (object, array, scalar, or null), matching the existing serde_json::Value wire field.
 */
export interface CoreToolExecuteResponse {
  success: boolean;
  result: unknown;
}
