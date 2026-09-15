/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Host tool execution result: success flag plus the extensible JSON result produced by the tool handler.
 */
export interface CoreToolExecuteResponse {
  success: boolean;
  /**
   * Tool-produced JSON result.
   */
  result: {
    [k: string]: unknown | undefined;
  };
}
