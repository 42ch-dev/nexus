/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Host tool invocation request: the typed entry point of the capability spine, shared by HTTP, the internal agent-host route and schedule dispatch. `parameters` stays an unrestricted JSON value because that IS the capability contract — a capability declares a JSON Schema and receives arguments matching it, so the capability schema (never a Rust type per tool) is the authority for the payload.
 */
export interface ToolsApi {
  /**
   * The tool id to dispatch.
   */
  tool_name: string;
  parameters: unknown;
  /**
   * Session this call belongs to, when the caller has one.
   */
  session_id?: string;
  /**
   * Caller-supplied request id, for audit correlation.
   */
  request_id?: string;
  /**
   * Who is calling, when it is not the ordinary HTTP path.
   */
  caller_kind?: "schedule";
}
