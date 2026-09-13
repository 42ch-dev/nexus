/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Native hostQuery discriminator over existing HostFacade registry.
 */
export interface CoreHostQuery {
  query: "health" | "catalog" | "list_sessions" | "get_session" | "get_operation" | "scan";
  session_id?: string;
  operation_id?: string;
  limit?: number;
  cursor?: string;
  format?: "catalog" | "scan";
}
